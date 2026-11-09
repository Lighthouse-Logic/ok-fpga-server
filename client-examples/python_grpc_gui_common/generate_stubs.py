#!/usr/bin/env python3
# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
"""Generate gRPC Python stubs from the ok-FPGA-Server proto file.

Usage:
    pip install grpcio-tools
    python generate_stubs.py
"""

import os
import subprocess
import sys

PROTO_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "proto", "okfpga", "v1")
PROTO_FILE = os.path.join(PROTO_DIR, "okfpga.proto")
OUT_DIR = os.path.join(os.path.dirname(__file__), "gen")


def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    # Create __init__.py in gen/ so it's importable.
    init_path = os.path.join(OUT_DIR, "__init__.py")
    if not os.path.exists(init_path):
        with open(init_path, "w") as f:
            f.write("# Generated gRPC stubs\n")

    cmd = [
        sys.executable, "-m", "grpc_tools.protoc",
        f"--proto_path={PROTO_DIR}",
        f"--python_out={OUT_DIR}",
        f"--grpc_python_out={OUT_DIR}",
        PROTO_FILE,
    ]
    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"Error:\n{result.stderr}", file=sys.stderr)
        sys.exit(1)
    print(f"Stubs generated in {OUT_DIR}")


if __name__ == "__main__":
    main()
