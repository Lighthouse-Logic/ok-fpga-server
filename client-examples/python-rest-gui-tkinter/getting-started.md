# Getting Started — Python REST GUI (tkinter)

**Demo name**: Pythos Rester  
**Protocol**: REST (HTTP/JSON)  
**GUI framework**: tkinter (Python standard library)

## Prerequisites

- Python 3.10+
- pip packages: `requests`
- ok-FPGA-Server running with at least one connected FPGA
- A Counters.bit bitfile for your board

## Install Dependencies

```bash
pip install requests
```

## Run

```bash
cd client-examples/python-rest-gui-tkinter
python main.py
```

## Default Configuration

| Setting | Default | Override |
|---------|---------|---------|
| Server URL | `http://localhost:8088` | Edit in GUI |
| Bitfile path | `FrontPanel-6.1.0/ExampleBitfiles/...` | Edit in GUI |
| User ID | `Pythos Rester` | Hardcoded in `main.py` |

## Workflow

1. Click **Connect to Server** — discovers devices via `GET /api/v1/devices`
2. Select a device from the dropdown
3. Enter bitfile path and click **Program Bitfile**
4. Use the Counter Control Panel to interact with the FPGA
