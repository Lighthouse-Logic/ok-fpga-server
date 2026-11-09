# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""Proto file compiler for the ok-FPGA-Server gRPC client.

Runs grpcio-tools (protoc) to compile the proto file into Python stubs.
Generated files are placed in okfpga_grpc_client/generated/.

Usage:
    python generate_protos.py
"""

import os
import subprocess
import sys
from pathlib import Path


def main() -> None:
    # Paths relative to this script's directory.
    script_dir = Path(__file__).parent
    proto_file = script_dir / ".." / ".." / "proto" / "okfpga" / "v1" / "okfpga.proto"
    proto_dir = proto_file.parent
    output_dir = script_dir / "okfpga_grpc_client" / "generated"

    if not proto_file.exists():
        print(f"ERROR: Proto file not found at {proto_file.resolve()}")
        sys.exit(1)

    # Create output directory if it doesn't exist.
    output_dir.mkdir(parents=True, exist_ok=True)

    # Create __init__.py in the generated directory.
    init_file = output_dir / "__init__.py"
    if not init_file.exists():
        init_file.write_text("# Auto-generated gRPC stubs.\n")

    print(f"Compiling {proto_file.name} -> {output_dir}/")

    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            f"--proto_path={proto_dir.resolve()}",
            f"--python_out={output_dir.resolve()}",
            f"--grpc_python_out={output_dir.resolve()}",
            f"--pyi_out={output_dir.resolve()}",
            str(proto_file.resolve()),
        ],
        capture_output=True,
        text=True,
    )

    if result.returncode != 0:
        print(f"ERROR: protoc failed:\n{result.stderr}")
        sys.exit(1)

    # Post-process: fix absolute imports to relative imports.
    # grpcio-tools generates `import okfpga_pb2 as ...` but we need
    # `from . import okfpga_pb2 as ...` for package-relative imports.
    grpc_file = output_dir / "okfpga_pb2_grpc.py"
    if grpc_file.exists():
        content = grpc_file.read_text()
        content = content.replace(
            "import okfpga_pb2 as okfpga__pb2",
            "from . import okfpga_pb2 as okfpga__pb2",
        )
        grpc_file.write_text(content)
        print("  Fixed relative imports in okfpga_pb2_grpc.py")

    print("OK: Proto compilation complete.")
    print(f"Generated files in {output_dir}/:")
    for f in sorted(output_dir.iterdir()):
        if f.suffix in (".py", ".pyi"):
            print(f"  {f.name}")


if __name__ == "__main__":
    main()
