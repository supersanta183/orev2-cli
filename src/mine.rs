use std::{sync::Arc, time::Instant};
use std::fs::File;
use std::io::{self, Write};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering, AtomicU32};
use rayon::prelude::*;

use colored::*;
use drillx::{
    equix::{self},
    Hash, Solution, difficulty,
};
use ore_api::{
    consts::{BUS_ADDRESSES, BUS_COUNT, EPOCH_DURATION},
    state::{Config, Proof},
};
use rand::Rng;
use solana_program::pubkey::Pubkey;
use solana_rpc_client::spinner;
use solana_sdk::signer::Signer;

use crate::{
    args::MineArgs,
    send_and_confirm::ComputeBudget,
    utils::{amount_u64_to_string, get_clock, get_config, get_proof_with_authority, proof_pubkey},
    Miner, constants,
};

impl Miner {
    pub async fn mine(&self, args: MineArgs) {
        // Register, if needed.
        let signer = self.signer();
        self.open().await;

        // Check num threads
        self.check_num_cores(args.threads);

        // Start mining loop
        loop {
            // Fetch proof
            let config = get_config(&self.rpc_client).await;
            let proof = get_proof_with_authority(&self.rpc_client, signer.pubkey()).await;
            println!(
                "\nStake: {} ORE\n Multiplier: {:12}x",
                amount_u64_to_string(proof.balance),
                calculate_multiplier(proof.balance, config.top_balance)
            );

            // Calc cutoff time
            let cutoff_time = self.get_cutoff(proof, args.buffer_time).await;

            // Run drillx
            let config = get_config(&self.rpc_client).await;
            let (solution, best_difficulty) = Self::find_hash_par(
                proof,
                cutoff_time,
                args.threads as usize,
                config.min_difficulty as u32,
            )
            .await;

            // Submit most difficult hash
            let mut compute_budget = 500_000;
            let mut ixs = vec![ore_api::instruction::auth(proof_pubkey(signer.pubkey()))];
            if self.should_reset(config).await && rand::thread_rng().gen_range(0..100).eq(&0) {
                compute_budget += 100_000;
                ixs.push(ore_api::instruction::reset(signer.pubkey()));
            }
            ixs.push(ore_api::instruction::mine(
                signer.pubkey(),
                signer.pubkey(),
                find_bus(),
                solution,
            ));


            //dynamic priorityfee
            let mut priority_fee;

            if best_difficulty < 17 {
                priority_fee = constants::LOW_PRIORITY_FEE;
            } else if best_difficulty < 20 {
                priority_fee = constants::MEDIUM_PRIORITY_FEE;
            } else if best_difficulty < 24 {
                priority_fee = constants::HIGH_PRIORITY_FEE;
            } else {
                priority_fee = constants::ULTRA_PRIORITY_FEE;
            }

            println!("pri fee {}", priority_fee);

            self.send_and_confirm(&ixs, ComputeBudget::Fixed(compute_budget), false, priority_fee)
                .await
                .ok();
        }
    }

    async fn find_hash_par(
        proof: Proof,
        cutoff_time: u64,
        threads: usize,
        min_difficulty: u32,
    ) -> (Solution, u32) {
        let progress_bar = Arc::new(spinner::new_progress_bar());
        progress_bar.set_message("Mining...");
    
        let best_difficulty = Arc::new(AtomicU32::new(0));
        let best_nonce = Arc::new(AtomicU64::new(0));
        let best_hash = Arc::new(std::sync::Mutex::new(Hash::default()));
        let nonce_counter = Arc::new(AtomicU64::new(0));
        let hash_counter = Arc::new(AtomicU64::new(0)); // New hash counter
    
        rayon::scope(|s| {
            for _ in 0..threads {
                let proof = proof.clone();
                let progress_bar = Arc::clone(&progress_bar);
                let best_difficulty = Arc::clone(&best_difficulty);
                let best_nonce = Arc::clone(&best_nonce);
                let best_hash = Arc::clone(&best_hash);
                let nonce_counter = Arc::clone(&nonce_counter);
                let hash_counter = Arc::clone(&hash_counter); // Clone hash counter
    
                s.spawn(move |_| {
                    let timer = Instant::now();
                    let mut memory = equix::SolverMemory::new();
    
                    loop {
                        let nonce = nonce_counter.fetch_add(1, Ordering::Relaxed);
    
                        // Create hash
                        if let Ok(hx) = drillx::hash_with_memory(
                            &mut memory,
                            &proof.challenge,
                            &nonce.to_le_bytes(),
                        ) {
                            hash_counter.fetch_add(1, Ordering::Relaxed); // Increment hash counter
                            let difficulty = hx.difficulty();
                            let current_best_difficulty = best_difficulty.load(Ordering::Relaxed);
    
                            if difficulty > current_best_difficulty {
                                best_difficulty.store(difficulty, Ordering::Relaxed);
                                best_nonce.store(nonce, Ordering::Relaxed);
                                let mut best_hash_lock = best_hash.lock().unwrap();
                                *best_hash_lock = hx;
                            }
                        }
    
                        // Exit if time has elapsed
                        if nonce % 100 == 0 {
                            if timer.elapsed().as_secs() >= cutoff_time {
                                if best_difficulty.load(Ordering::Relaxed) >= min_difficulty {
                                    break;
                                }
                            } else if nonce % 1000 == 0 {
                                progress_bar.set_message(format!(
                                    "Mining... ({} sec remaining)",
                                    cutoff_time.saturating_sub(timer.elapsed().as_secs()),
                                ));
                            }
                        }
                    }
                });
            }
        });
    
        let best_hash_final = best_hash.lock().unwrap();
        // Update log
        progress_bar.finish_with_message(format!(
            "Best hash: {} (difficulty: {})",
            bs58::encode(best_hash_final.h).into_string(),
            best_difficulty.load(Ordering::Relaxed)
        ));
    
        // Print the total number of hashes checked
        println!("Total hashes checked: {}", hash_counter.load(Ordering::Relaxed));
    
        (
            Solution::new(
                best_hash_final.d,
                best_nonce.load(Ordering::Relaxed).to_le_bytes(),
            ),
            best_difficulty.load(Ordering::Relaxed),
        )
    }

    pub fn check_num_cores(&self, threads: u64) {
        // Check num threads
        let num_cores = num_cpus::get() as u64;
        if threads.gt(&num_cores) {
            println!(
                "{} Number of threads ({}) exceeds available cores ({})",
                "WARNING".bold().yellow(),
                threads,
                num_cores
            );
        }
    }

    async fn should_reset(&self, config: Config) -> bool {
        let clock = get_clock(&self.rpc_client).await;
        config
            .last_reset_at
            .saturating_add(EPOCH_DURATION)
            .saturating_sub(5) // Buffer
            .le(&clock.unix_timestamp)
    }

    async fn get_cutoff(&self, proof: Proof, buffer_time: u64) -> u64 {
        let clock = get_clock(&self.rpc_client).await;
        proof
            .last_hash_at
            .saturating_add(60)
            .saturating_sub(buffer_time as i64)
            .saturating_sub(clock.unix_timestamp)
            .max(0) as u64
    }
}

// TODO Pick a better strategy (avoid draining bus)
fn find_bus() -> Pubkey {
    let i = rand::thread_rng().gen_range(0..BUS_COUNT);
    BUS_ADDRESSES[i]
}

fn calculate_multiplier(balance: u64, top_balance: u64) -> f64 {
    1.0 + (balance as f64 / top_balance as f64).min(1.0f64)
}