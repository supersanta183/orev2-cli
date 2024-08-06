import subprocess
import sys
from dotenv import load_dotenv
import os

mainnet = "MAINNET"
key = "ALCHEMY"
i = sys.argv[1]
threads = sys.argv[2]

load_dotenv()

RPI = os.getenv(key)
MAINNET_RPI = os.getenv(mainnet)
print(RPI)

command = [
    "cargo", "run", "--release",
    "mine",
    "--rpc", MAINNET_RPI,
    "--rpc2", RPI,
    #/root/orev2_setup/ids
    "--keypair", f"/root/orev2_setup/ids/id{i}.json",
    "--threads", threads,
    "--priority-fee", str(500000),
]

subprocess.run(command)