# Getting Started — Python gRPC GUI (tkinter)

**Demo name**: Pythos Garpac  
**Protocol**: gRPC (HTTP/2 + Protocol Buffers)  
**GUI framework**: tkinter (Python standard library)

## Prerequisites

- Python 3.10+
- pip packages: `grpcio`, `grpcio-tools`
- ok-FPGA-Server running with gRPC enabled (default port 50051)
- A Counters.bit bitfile for your board

## Install Dependencies

```bash
pip install grpcio grpcio-tools
```

## Generate Proto Stubs (one-time setup)

The gRPC client requires generated Python stubs from the `.proto` file.
These are committed to the repository, but if they are missing or outdated:

```bash
cd client-examples/python_grpc_gui_common
python generate_stubs.py
```

This creates the `gen/` directory with `okfpga_pb2.py` and `okfpga_pb2_grpc.py`.

## Run

```bash
cd client-examples/python-grpc-gui-tkinter
python main.py
```

## Default Configuration

| Setting | Default | Override |
|---------|---------|---------|
| gRPC address | `localhost:50051` | Edit in GUI |
| Bitfile path | `FrontPanel-6.1.0/ExampleBitfiles/...` | Edit in GUI |
| User ID | `Pythos Garpac` | Hardcoded in `main.py` |

## Workflow

1. Click **Connect to Server** — discovers devices via `ListDevices` RPC
2. Select a device from the dropdown
3. Enter bitfile path and click **Program Bitfile**
4. Use the Counter Control Panel to interact with the FPGA
