import subprocess
import sys
from dotenv import load_dotenv
import os

key = "ALCHEMY"
MAINNET = "MAINNET"
i = sys.argv[1]

load_dotenv()

RPI = os.getenv(key)
MAINNET = os.getenv(MAINNET)
print(MAINNET)
print(RPI)

command = [
    "cargo", "run", "--release",
    "claim",
    "--rpc", MAINNET,
    "--rpc2", RPI,
    #/root/orev2_setup/ids
    "--keypair", f"/root/orev2_setup/ids/id{i}.json",
    "--priority-fee", str(500000),
]

subprocess.run(command)