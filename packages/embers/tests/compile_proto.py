import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent

PROTO_DIR = ROOT / "firefly-client" / "protobuf"
PROTO_EXTERNAL_DIR = PROTO_DIR / "protobuf_external"
OUT_DIR = ROOT / "embers" / "tests" / "protobuf"


def compile_all():
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    proto_files = list(PROTO_DIR.rglob("*.proto"))
    if not proto_files:
        print("No .proto files found.")  # noqa: T201
        return

    for proto in proto_files:
        cmd = [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            f"-I{PROTO_DIR}",
            f"-I{PROTO_EXTERNAL_DIR}",
            f"--python_betterproto2_out={OUT_DIR}",
            str(proto),
        ]
        print("Compiling:", proto)  # noqa: T201
        subprocess.run(cmd, check=True)  # noqa: S603


if __name__ == "__main__":
    compile_all()
