import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent  # embers/packages

# Proto definitions are sourced directly from the f1r3node-rust `models` subcrate
# (the single source of truth) rather than being vendored into this repository,
# which is error prone to keep in sync. Point F1R3NODE_MODELS_DIR at the `models`
# subcrate of a f1r3node-rust checkout/worktree; the default assumes the
# cost-accounting-transpiler worktree sits alongside this repository.
DEFAULT_MODELS_DIR = ROOT.parent.parent / "f1r3node-rust-cost-accounting-transpiler" / "models"
MODELS_DIR = pathlib.Path(os.environ.get("F1R3NODE_MODELS_DIR", DEFAULT_MODELS_DIR)).resolve()

PROTO_DIR = MODELS_DIR / "src" / "main" / "protobuf"
# `scalapb/scalapb.proto` is imported as "scalapb/scalapb.proto" and lives under
# models/src, so that directory is an additional include root. The google/*
# well-known types (e.g. google/protobuf/empty.proto) are bundled with
# grpcio-tools and resolved automatically.
PROTO_INCLUDE_DIR = MODELS_DIR / "src"
OUT_DIR = ROOT / "embers" / "tests" / "protobuf"

# Client-facing protos only, mirroring firefly-client's
# `pub use f1r3node_models::{casper, rhoapi, servicemodelapi}` (plus routing).
# The internal node protos (RholangScalaRustTypes, RSpacePlusPlusTypes) are not
# part of the client API surface and are intentionally excluded.
PROTO_FILES = [
    "CasperMessage.proto",
    "DeployServiceCommon.proto",
    "DeployServiceV1.proto",
    "ExternalCommunicationServiceCommon.proto",
    "ExternalCommunicationServiceV1.proto",
    "ProposeServiceCommon.proto",
    "ProposeServiceV1.proto",
    "RhoTypes.proto",
    "ServiceError.proto",
    "routing.proto",
]

# The betterproto2 compiler plugin pins an older `ruff` than this project uses,
# so it is NOT a project dependency. It is a build-time-only tool (the tests
# import only the betterproto2 runtime), supplied here from an isolated,
# ephemeral environment via `uv`. Keep the compiler pinned to the betterproto2
# runtime's minor line (see pyproject: betterproto2 >=0.9,<0.10).
COMPILER_PYTHON = "3.13"
COMPILER_PLUGIN = "betterproto2-compiler==0.9.0"


def compile_all():
    if shutil.which("uv") is None:
        sys.exit(
            "This script needs `uv` to run the betterproto2 compiler plugin in an "
            "isolated environment (the plugin conflicts with the project's ruff pin).\n"
            "Install uv: https://docs.astral.sh/uv/",
        )
    if not PROTO_DIR.is_dir():
        sys.exit(
            f"Proto source directory not found: {PROTO_DIR}\n"
            "Set F1R3NODE_MODELS_DIR to the `models` subcrate of a f1r3node-rust checkout.",
        )

    proto_paths = [PROTO_DIR / name for name in PROTO_FILES]
    missing = [str(p) for p in proto_paths if not p.is_file()]
    if missing:
        sys.exit("Missing proto files:\n" + "\n".join(missing))

    OUT_DIR.mkdir(parents=True, exist_ok=True)

    # A single protoc invocation so betterproto2 emits coherent package modules
    # (several files share the `casper` / `casper.v1` packages). `--no-project`
    # stops `uv` from trying to build this (non-package) project.
    cmd = [
        "uv", "run", "--no-project", "--python", COMPILER_PYTHON,
        "--with", "grpcio-tools", "--with", COMPILER_PLUGIN,
        "--",
        "python", "-m", "grpc_tools.protoc",
        f"-I{PROTO_DIR}",
        f"-I{PROTO_INCLUDE_DIR}",
        f"--python_betterproto2_out={OUT_DIR}",
        *[str(p) for p in proto_paths],
    ]
    print("Compiling from:", PROTO_DIR)  # noqa: T201
    print("Protos:", ", ".join(PROTO_FILES))  # noqa: T201
    subprocess.run(cmd, check=True)  # noqa: S603


if __name__ == "__main__":
    compile_all()
