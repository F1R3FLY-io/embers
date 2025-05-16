import subprocess
import sys
from time import sleep


def run_and_stream_output(command: list[str]):
    output = []
    process = subprocess.Popen(command, stdout=subprocess.PIPE)
    for c in iter(lambda: process.stdout.read(1), b""):
        output.append(c)
        sys.stdout.buffer.write(c)

    return b"".join(output).decode()


def find_did(service: str, prefix: str) -> str:
    while True:
        process = subprocess.Popen(
            ["docker", "compose", "logs", service],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        for line in process.stdout:
            _, _, after = line.partition(prefix)
            if len(after) != 0:
                return after.strip()

        sleep(1)


run_and_stream_output(
    [
        "docker",
        "compose",
        "up",
        "-d",
        "firesky-ts",
        "postgresql-rs-1",
        "postgresql-rs-2",
        "minio",
    ]
)

did1 = find_did("firesky-ts", "Bsky Appview#1 DID")
with open(".env-1", "w") as f:
    f.write(f"PDS_BSKY_APP_VIEW_DID = {did1}")

did2 = find_did("firesky-ts", "Bsky Appview#2 DID")
with open(".env-2", "w") as f:
    f.write(f"PDS_BSKY_APP_VIEW_DID = {did2}")

run_and_stream_output(["docker", "compose", "up", "-d"])
