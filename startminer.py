import subprocess
import sys
from dotenv import load_dotenv
import os

mainnet = "MAINNET"
key = "ALCHEMY"
i = sys.argv[1]
cores = sys.argv[2]

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
    "--cores", cores,
    "--priority-fee", str(2000),
    "--buffer-time", str(2),
]

subprocess.run(command)