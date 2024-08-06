import subprocess
import sys
from dotenv import load_dotenv
import os

key = "ALCHEMY"
i = sys.argv[1]
threads = sys.argv[2]

load_dotenv()

RPI = os.getenv(key)
print(RPI)

command = [
    "ore",
    "mine",
    "--rpc", RPI,
    #/root/orev2_setup/ids
    "--keypair", f"/root/orev2_setup/ids/id{i}.json",
    "--threads", threads,
    "--priority-fee", str(100000),
]

subprocess.run(command)