# Guide: Building a Python REST Client for ok-FPGA-Server

> **Audience:** An AI assistant that is new to this codebase.
>
> This guide walks through building a Python REST client from scratch that
> discovers the ok-FPGA-Server, connects with a user identity, lists devices
> with SYZYGY ports, programs an FPGA, registers and acquires resources, and
> performs Classic Data Port I/O operations (Wire, Trigger, Pipe, BTPipe).

---

## 1. Prerequisites

```
pip install requests zeroconf
```

- **ok-FPGA-Server** must be running (default REST port: 8088, management/dashboard: 9090).
- **`X-User-Id` header** is **required** on every REST request. The server
  enforces endpoint access control and will return `403 Forbidden` without it.

---

## 2. Server Discovery via mDNS

The server advertises itself as `_okfpga._tcp.local.` via mDNS/DNS-SD.
TXT records carry port numbers:

| TXT Key | Example | Description |
|---------|---------|-------------|
| `rest` | `8088` | REST API port |
| `grpc` | `50051` | gRPC port |
| `devices` | `3` | Number of connected FPGAs |
| `version` | `1.0.0` | API version |

### Discovery Code

```python
import time
from zeroconf import ServiceBrowser, ServiceInfo, Zeroconf

SERVICE_TYPE = "_okfpga._tcp.local."

class _Listener:
    def __init__(self):
        self.servers = []
    
    def add_service(self, zc, type_, name):
        info = zc.get_service_info(type_, name)
        if info:
            addresses = [addr for addr in info.parsed_scoped_addresses()]
            rest_port = int(info.properties.get(b"rest", b"0").decode())
            self.servers.append({
                "name": info.name,
                "addresses": addresses,
                "rest_port": rest_port,
                "rest_url": f"http://{addresses[0]}:{rest_port}" if addresses and rest_port else None,
            })
    
    def remove_service(self, zc, type_, name):
        pass
    
    def update_service(self, zc, type_, name):
        pass

def discover_servers(timeout=3.0):
    """Browse for ok-FPGA-Server instances on the local network."""
    zc = Zeroconf()
    listener = _Listener()
    ServiceBrowser(zc, SERVICE_TYPE, listener)
    time.sleep(timeout)
    zc.close()
    return listener.servers
```

### Usage

```python
servers = discover_servers()
if servers:
    base_url = servers[0]["rest_url"]
    print(f"Found server at {base_url}")
else:
    base_url = "http://localhost:8088"
    print("No servers found, using default")
```

> **Tip:** If you know the server address, skip mDNS and use the URL directly.

---

## 3. Connecting with a User Identity

Every request must include the `X-User-Id` header. Use a `requests.Session`
to set it once:

```python
import requests

class OkFpgaClient:
    def __init__(self, server_url: str, user_id: str):
        self.base_url = f"{server_url.rstrip('/')}/api/v1"
        self.user_id = user_id
        self.session = requests.Session()
        self.session.headers["X-User-Id"] = user_id
    
    def _get(self, path: str, **kwargs):
        resp = self.session.get(f"{self.base_url}{path}", **kwargs)
        resp.raise_for_status()
        return resp.json() if resp.text else None
    
    def _post(self, path: str, json=None, **kwargs):
        resp = self.session.post(f"{self.base_url}{path}", json=json, **kwargs)
        resp.raise_for_status()
        return resp.json() if resp.text else None
    
    def _put(self, path: str, json=None):
        resp = self.session.put(f"{self.base_url}{path}", json=json)
        resp.raise_for_status()
        return resp.json() if resp.text else None
```

### Connecting

```python
client = OkFpgaClient("http://localhost:8088", user_id="Lab-User-Alice")
status = client._get("/server/status")
print(f"Server version: {status['api_version']}, uptime: {status['uptime_secs']}s")
```

---

## 4. Listing Devices with SYZYGY Ports

### List Devices

```python
# GET /api/v1/devices → {"devices": [...]}
def list_devices(client):
    data = client._get("/devices")
    return data.get("devices", [])

devices = list_devices(client)
for d in devices:
    print(f"  Serial: {d['serial']}")
    print(f"  Product: {d['product_name']}")
    print(f"  Programmed: {d.get('is_programmed', False)}")
```

### Get SYZYGY Port Info

Each device may have SYZYGY expansion ports with attached peripherals.

```python
# GET /api/v1/devices/{serial}/syzygy → {"ports": [...]}
def get_syzygy_ports(client, serial: str):
    data = client._get(f"/devices/{serial}/syzygy")
    ports = data.get("ports", [])
    populated = [p for p in ports if p.get("is_populated")]
    for p in populated:
        print(f"  Port {p['index']}: {p['product_name']} ({p['product_model']})")
        print(f"    Manufacturer: {p['manufacturer_name']}")
        print(f"    Serial: {p['serial_number']}")
    return populated

# Example output:
#   Port 0: SZG-ENET1G (SZG-ENET1G)
#     Manufacturer: Opal Kelly Incorporated
#     Serial: 2244000123A
```

---

## 5. Programming the FPGA

There are **two endpoints** for FPGA configuration ([ADR-028](../architecture/adr/ADR-028-configure-fpga-api-alignment.md)):

### Mode A: Server-Side Path (bitfile exists on the server)

The bitfile must exist on the server's local filesystem. The server reads
the file and calls `okFrontPanel_ConfigureFPGA()`.

```python
# POST /api/v1/devices/{serial}/configure_from_server_path
# Content-Type: application/json
# Headers: X-User-Id: <user_id>
# Body: {"bitfile_path": "/path/on/server/Counters.bit", "user_id": "<user_id>"}
def configure_fpga_server_side(client, serial: str, server_path: str, description: str = ""):
    body = {"bitfile_path": server_path, "user_id": client.user_id}
    if description:
        body["description"] = description
    return client._post(f"/devices/{serial}/configure_from_server_path", json=body)

result = configure_fpga_server_side(client, "22520013CC",
    "/opt/fpga/bitfiles/Counters.bit")
print(f"Configured: {result}")
```

### Mode B: Client-Side Upload (bitfile exists on the client)

The client reads the bitfile locally and uploads it via `multipart/form-data`.
The server calls `okFrontPanel_ConfigureFPGAFromMemory()`.

```python
# POST /api/v1/devices/{serial}/configure
# Content-Type: multipart/form-data (set automatically by requests)
# Headers: X-User-Id: <user_id>
# Form parts: bitfile (binary file), filename, description (optional), user_id
def configure_fpga_client_side(client, serial: str, local_path: str, description: str = ""):
    import os
    filename = os.path.basename(local_path)
    url = f"{client.base_url}/devices/{serial}/configure"
    data = {"filename": filename, "user_id": client.user_id}
    if description:
        data["description"] = description
    with open(local_path, "rb") as f:
        files = {"bitfile": (filename, f, "application/octet-stream")}
        resp = client.session.post(url, files=files, data=data)
    resp.raise_for_status()
    return resp.json()

result = configure_fpga_client_side(client, "22520013CC",
    r"C:\fpga\bitfiles\Counters.bit")
print(f"Configured: {result}")
```

### Response (both modes)

```json
{
  "filename": "Counters.bit",
  "file_path": "/opt/fpga/bitfiles/Counters.bit",
  "md5_hash": "a1b2c3d4e5f6...",
  "programmed_at": "2026-08-21T20:00:00Z",
  "programmed_by": "Lab-User-Alice"
}
```

> **Forced Resource Cleanup ([ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md) §14, `D26`):**
> Reprogramming an FPGA (`POST /devices/{serial}/configure` or
> `POST /devices/{serial}/configure_from_server_path`) or clearing its
> configuration (`DELETE /devices/{serial}/configure`) is an unconditional
> forced override: the server automatically unlocks any locked shared
> resources, releases all holders, deletes all existing resources on the
> device (`reason: "reprogram"` or `"clear"`), and notifies all holders via
> SSE / WebSocket / gRPC events before programming.

---

## 6. Resource Management

After programming, you must **register** one or more resources and **acquire**
one before performing data port operations ([ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md)).
Resources represent logical ownership of the FPGA hardware.

### Create Resources

```python
# POST /api/v1/resources
# Body: {"name": "...", "device_serial": "...", "access_mode": "exclusive",
#        "interfaces": ["classic"], "endpoints": {"entries": [...]}, "description": "..."}
def create_resource(client, name: str, serial: str, entries: list[dict], mode: str = "exclusive"):
    return client._post("/resources", json={
        "name": name,
        "device_serial": serial,
        "access_mode": mode,
        "interfaces": ["classic"],
        "endpoints": {"entries": entries},
        "description": f"Resource for {name}",
    })

# Create two non-overlapping exclusive resources on the same FPGA
res_a = create_resource(
    client, "ADC Controller", "22520013CC",
    entries=[
        {"address": "0x00", "type": "wire_in", "access": "exclusive"},
        {"address": "0x20", "type": "wire_out", "access": "exclusive"},
        {"address": "0x40", "type": "trigger_in", "access": "exclusive"},
        {"address": "0x60", "type": "trigger_out", "access": "exclusive"},
        {"address": "0x80", "type": "pipe_in", "access": "exclusive"},
        {"address": "0xA0", "type": "pipe_out", "access": "exclusive"},
    ],
    mode="exclusive",
)
res_b = create_resource(
    client, "DAC Controller", "22520013CC",
    entries=[
        {"address": "0x01", "type": "wire_in", "access": "exclusive"},
        {"address": "0x21", "type": "wire_out", "access": "exclusive"},
    ],
    mode="exclusive",
)

resource_id_a = res_a["id"]
resource_id_b = res_b["id"]
```

### Declared vs. Holder Modes ([ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md) §1, §11)

| Mode | Declared `access_mode` | Holder `mode` (`acquire`) | Behavior |
|------|------------------------|---------------------------|----------|
| `"shared"` | ✅ | ✅ | Multiple users can acquire simultaneously. Supports `lock` / `unlock`. |
| `"exclusive"` | ✅ | ✅ | Only one read/write holder at a time (`read_only` observers may still join unless `private: true`). |
| `"read_only"` | ❌ | ✅ | Observe-only holder mode: can read non-private items, never write or reset. |

### Acquire a Resource

```python
# POST /api/v1/resources/{id}/acquire
# Body: {"user_id": "Lab-User-Alice", "mode": "exclusive"} (mode is optional; defaults to resource's access_mode)
def acquire_resource(client, resource_id: str, mode: str | None = None):
    body = {"user_id": client.user_id}
    if mode is not None:
        body["mode"] = mode
    return client._post(f"/resources/{resource_id}/acquire", json=body)

acquire_resource(client, resource_id_a)
# Now this user holds "ADC Controller" in its declared "exclusive" mode
```

> If `mode` is omitted, the server acquires the resource in its declared
> `access_mode` (`"shared"` or `"exclusive"`). Passing `"shared"` on an
> `"exclusive"` resource (or vice versa) returns `409 Conflict`
> (`conflict.code = "mode_mismatch"`). Pass `"mode": "read_only"` to join
> as a read-only observer.

### Release a Resource

```python
# POST /api/v1/resources/{id}/release
# Body: {"user_id": "Lab-User-Alice"}
def release_resource(client, resource_id: str):
    return client._post(f"/resources/{resource_id}/release",
                        json={"user_id": client.user_id})
```

### List Resources

```python
# GET /api/v1/resources → {"resources": [...]}
resources = client._get("/resources")
for r in resources.get("resources", []):
    print(f"  {r['id']}: {r['name']} ({r['access_mode']}) holders={r['holders']}")
```

---

## 7. Classic Data Port Operations

Once you've acquired a resource on a programmed FPGA, you can read/write
endpoints. All Classic Data Port endpoints live under
`/devices/{serial}/classic/`.

### Endpoint Address Ranges

| Type | Direction | Address Range | HTTP Method |
|------|-----------|---------------|-------------|
| WireIn | Host → FPGA | `0x00–0x1F` | `PUT` |
| WireOut | FPGA → Host | `0x20–0x3F` | `GET` |
| TriggerIn | Host → FPGA | `0x40–0x5F` | `POST` |
| TriggerOut | FPGA → Host | `0x60–0x7F` | `GET` |
| PipeIn | Host → FPGA | `0x80–0x9F` | `POST` |
| PipeOut | FPGA → Host | `0xA0–0xBF` | `GET` |

### 7.1 WireIn / WireOut

**WireIn** — Set a persistent value on an FPGA input wire. When
`auto_update` is `true` (the default), the server automatically calls
`UpdateWireIns()` after setting the value; pass `auto_update: false` to stage
multiple wire changes and then call `POST /devices/{serial}/classic/wires/in/update`.
Note that FrontPanel has **no hardware `WireIn` read-back** ([ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md) §16); clients should cache values they wrote.

```python
# PUT /api/v1/devices/{serial}/classic/wires/in
# Body: {"endpoint": 0, "value": 5, "mask": 7, "auto_update": True}
def set_wire_in(client, serial, endpoint, value, mask=0xFFFFFFFF, auto_update=True):
    return client._put(
        f"/devices/{serial}/classic/wires/in",
        json={"endpoint": endpoint, "value": value, "mask": mask, "auto_update": auto_update},
    )

# Set bits 0-2 of WireIn endpoint 0x00
set_wire_in(client, "22520013CC", 0x00, value=0x05, mask=0x07)
```

**WireOut** — Read the value of a single FPGA output wire ([ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md) §9).
By default (`update=false`), `GET /wires/out` reads the shared device snapshot
from the most recent `UpdateWireOuts()`; pass `?update=true` to refresh the
snapshot from the FPGA and read atomically under the device lock.

```python
# GET /api/v1/devices/{serial}/classic/wires/out?endpoint=0x20&update=true
def get_wire_out(client, serial, endpoint, update=True):
    data = client._get(
        f"/devices/{serial}/classic/wires/out?endpoint={endpoint}&update={str(update).lower()}"
    )
    return data.get("value", 0)

counter_value = get_wire_out(client, "22520013CC", 0x20)
print(f"Counter1 = {counter_value}")
```

### 7.2 TriggerIn / TriggerOut

**TriggerIn** — Fire a one-shot trigger on a specific bit (`0–31`). The server calls
`ActivateTriggerIn(endpoint, bit)`. Unlike wires, triggers are edge-sensitive
and self-clearing.

```python
# POST /api/v1/devices/{serial}/classic/triggers/in
# Body: {"endpoint": 64, "bit": 0}
def activate_trigger(client, serial, endpoint, bit):
    return client._post(f"/devices/{serial}/classic/triggers/in",
                        json={"endpoint": endpoint, "bit": bit})

# Fire TriggerIn 0x40, bit 0 (RESET2 on Counters.bit)
activate_trigger(client, "22520013CC", 0x40, 0)
```

**TriggerOut** — Read which trigger bits have fired ([ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md) §9, `D76`, `D78`).
Pass `update=true` to call `UpdateTriggerOuts()` (which reads and clears all
TriggerOut latches on the device) before reading the snapshot, and pass `mask`
(defaults to `0`) to test specific bits in `triggered`; the full 32-bit latch
word is always returned in `vector`.

```python
# GET /api/v1/devices/{serial}/classic/triggers/out?endpoint=0x60&mask=0x03&update=true
def get_trigger_out(client, serial, endpoint, mask=0xFFFFFFFF, update=True):
    data = client._get(
        f"/devices/{serial}/classic/triggers/out?endpoint={endpoint}&mask={mask}&update={str(update).lower()}"
    )
    return data.get("vector", 0)

triggered = get_trigger_out(client, "22520013CC", 0x60, mask=0x03, update=True)
if triggered & 0x01:
    print("Counter1 reached 0x00!")
if triggered & 0x02:
    print("Counter1 reached 0x80!")
```

### 7.3 PipeIn / PipeOut

**PipeIn** — Write a block of bytes to the FPGA. The server calls
`WriteToPipeIn(endpoint, data)`. Data is sent as raw `application/octet-stream`.

```python
# POST /api/v1/devices/{serial}/classic/pipes/in?endpoint=0x80
# Content-Type: application/octet-stream
# Body: <raw bytes>
def write_pipe_in(client, serial, endpoint, data: bytes):
    url = f"{client.base_url}/devices/{serial}/classic/pipes/in?endpoint={endpoint}"
    resp = client.session.post(url, data=data,
                               headers={"Content-Type": "application/octet-stream"})
    resp.raise_for_status()
    return resp.json()

# Write 1024 bytes to PipeIn 0x80
payload = bytes(range(256)) * 4  # 1024 bytes
result = write_pipe_in(client, "22520013CC", 0x80, payload)
print(f"Wrote {result.get('bytes_transferred', 0)} bytes")
```

**PipeOut** — Read a block of bytes from the FPGA. The server calls
`ReadFromPipeOut(endpoint, length)` and returns the raw bytes.

```python
# GET /api/v1/devices/{serial}/classic/pipes/out?endpoint=0xA0&length=1024
def read_pipe_out(client, serial, endpoint, length):
    url = f"{client.base_url}/devices/{serial}/classic/pipes/out"
    resp = client.session.get(url, params={"endpoint": endpoint, "length": length})
    resp.raise_for_status()
    return resp.content  # raw bytes

data = read_pipe_out(client, "22520013CC", 0xA0, 1024)
print(f"Read {len(data)} bytes: {data[:16].hex()}...")
```

### 7.4 BlockThrottled PipeIn / PipeOut (BTPipe)

**BTPipeIn** — Same as PipeIn but with a `block_size` parameter. The server
calls `WriteToBlockPipeIn(endpoint, block_size, data)`. Block-throttled
transfers wait for a ready signal from the FPGA between each block,
preventing FIFO overflows.

```python
# POST /api/v1/devices/{serial}/classic/pipes/block-in?endpoint=0x80&block_size=64
# Content-Type: application/octet-stream
# Body: <raw bytes>
def write_btpipe_in(client, serial, endpoint, block_size, data: bytes):
    url = f"{client.base_url}/devices/{serial}/classic/pipes/block-in"
    resp = client.session.post(
        url, data=data,
        params={"endpoint": endpoint, "block_size": block_size},
        headers={"Content-Type": "application/octet-stream"},
    )
    resp.raise_for_status()
    return resp.json()

# Write with 64-byte block throttling
write_btpipe_in(client, "22520013CC", 0x80, 64, payload)
```

**BTPipeOut** — Same as PipeOut but with `block_size` for throttled reads.

```python
# GET /api/v1/devices/{serial}/classic/pipes/block-out?endpoint=0xA0&block_size=64&length=1024
def read_btpipe_out(client, serial, endpoint, block_size, length):
    url = f"{client.base_url}/devices/{serial}/classic/pipes/block-out"
    resp = client.session.get(url, params={
        "endpoint": endpoint, "block_size": block_size, "length": length,
    })
    resp.raise_for_status()
    return resp.content  # raw bytes

data = read_btpipe_out(client, "22520013CC", 0xA0, 64, 1024)
```

---

## 8. Complete Example

```python
#!/usr/bin/env python3
"""Complete ok-FPGA-Server REST client example."""

import requests

# --- Connect ---
SERVER = "http://localhost:8088"
USER = "Lab-User-Alice"

session = requests.Session()
session.headers["X-User-Id"] = USER
BASE = f"{SERVER}/api/v1"

def get(path, **kw):
    r = session.get(f"{BASE}{path}", **kw)
    r.raise_for_status()
    return r.json() if r.headers.get("content-type","").startswith("application/json") else r.content

def post(path, json=None, **kw):
    r = session.post(f"{BASE}{path}", json=json, **kw)
    r.raise_for_status()
    return r.json() if r.text else None

def put(path, json=None):
    r = session.put(f"{BASE}{path}", json=json)
    r.raise_for_status()
    return r.json() if r.text else None

# --- 1. List Devices ---
devices = get("/devices")["devices"]
serial = devices[0]["serial"]
print(f"Device: {devices[0]['product_name']} ({serial})")

# --- 2. SYZYGY Ports ---
ports = get(f"/devices/{serial}/syzygy").get("ports", [])
for p in ports:
    if p.get("is_populated"):
        print(f"  SYZYGY {p['index']}: {p['product_name']}")

# --- 3. Program FPGA (server-side path, ADR-028 / D26) ---
post(f"/devices/{serial}/configure_from_server_path",
     json={"bitfile_path": r"C:\fpga\Counters.bit", "user_id": USER})
print("FPGA programmed.")

# --- 4. Create Two Non-Overlapping Exclusive Resources (ADR-034) ---
res_a = post("/resources", json={
    "name": "ADC", "device_serial": serial,
    "access_mode": "exclusive", "interfaces": ["classic"],
    "endpoints": {"entries": [
        {"address": "0x00", "type": "wire_in", "access": "exclusive"},
        {"address": "0x20", "type": "wire_out", "access": "exclusive"},
        {"address": "0x40", "type": "trigger_in", "access": "exclusive"},
        {"address": "0x60", "type": "trigger_out", "access": "exclusive"},
        {"address": "0x80", "type": "pipe_in", "access": "exclusive"},
        {"address": "0xA0", "type": "pipe_out", "access": "exclusive"},
    ]},
    "description": "ADC resource"})
res_b = post("/resources", json={
    "name": "DAC", "device_serial": serial,
    "access_mode": "exclusive", "interfaces": ["classic"],
    "endpoints": {"entries": [
        {"address": "0x01", "type": "wire_in", "access": "exclusive"},
        {"address": "0x21", "type": "wire_out", "access": "exclusive"},
    ]},
    "description": "DAC resource"})

# --- 5. Acquire One Resource ---
post(f"/resources/{res_a['id']}/acquire",
     json={"mode": "exclusive", "user_id": USER})
print(f"Acquired: {res_a['id']}")

# --- 6. WireIn/Out ---
put(f"/devices/{serial}/classic/wires/in",
    json={"endpoint": 0x00, "value": 0x04, "mask": 0x07})  # AUTOCOUNT2=on
counter = get(f"/devices/{serial}/classic/wires/out?endpoint=0x20&update=true")["value"]
print(f"Counter1 = {counter & 0xFF}")

# --- 7. TriggerIn/Out ---
post(f"/devices/{serial}/classic/triggers/in",
     json={"endpoint": 0x40, "bit": 0})  # RESET2
trig = get(f"/devices/{serial}/classic/triggers/out?endpoint=0x60&mask=0x03&update=true")["vector"]
print(f"TriggerOut 0x60 = {trig:#06x}")

# --- 8. PipeIn/Out ---
payload = bytes(1024)
session.post(f"{BASE}/devices/{serial}/classic/pipes/in?endpoint=0x80",
             data=payload, headers={"Content-Type": "application/octet-stream"})
data = session.get(f"{BASE}/devices/{serial}/classic/pipes/out",
                   params={"endpoint": 0xA0, "length": 1024}).content
print(f"Pipe round-trip: wrote 1024, read {len(data)} bytes")

# --- 9. Cleanup ---
post(f"/resources/{res_a['id']}/release", json={"user_id": USER})
print("Released resource, done.")
```

---

## 9. Reference: Key API Endpoints

| Operation | Method | Path | Body / Params |
|-----------|--------|------|---------------|
| Server status | `GET` | `/server/status` | — |
| List devices | `GET` | `/devices` | — |
| Device info | `GET` | `/devices/{serial}` | — |
| SYZYGY ports | `GET` | `/devices/{serial}/syzygy` | — |
| Program (server path) | `POST` | `/devices/{serial}/configure_from_server_path` | `{"bitfile_path": "...", "user_id": "..."}` |
| Program (upload) | `POST` | `/devices/{serial}/configure` | `multipart/form-data` (`bitfile`, `filename`, `user_id`) |
| Clear configuration | `DELETE` | `/devices/{serial}/configure` | — |
| Create resource | `POST` | `/resources` | `{"name":"...", "device_serial":"...", "access_mode":"...", "interfaces":["classic"], "endpoints":{...}}` |
| Acquire resource | `POST` | `/resources/{id}/acquire` | `{"user_id":"...", "mode":"shared\|exclusive\|read_only"}` |
| Release resource | `POST` | `/resources/{id}/release` | `{"user_id":"..."}` |
| Set WireIn | `PUT` | `/devices/{serial}/classic/wires/in` | `{"endpoint":0, "value":5, "mask":7, "auto_update":true}` |
| Read WireOut | `GET` | `/devices/{serial}/classic/wires/out?endpoint=0x20&update=true` | — |
| Fire TriggerIn | `POST` | `/devices/{serial}/classic/triggers/in` | `{"endpoint":64, "bit":0}` |
| Read TriggerOut | `GET` | `/devices/{serial}/classic/triggers/out?endpoint=0x60&mask=0x03&update=true` | — |
| Write PipeIn | `POST` | `/devices/{serial}/classic/pipes/in?endpoint=0x80` | Body: raw bytes (`application/octet-stream`) |
| Read PipeOut | `GET` | `/devices/{serial}/classic/pipes/out?endpoint=0xA0&length=1024` | — |
| Write BTPipeIn | `POST` | `/devices/{serial}/classic/pipes/block-in?endpoint=0x80&block_size=64` | Body: raw bytes (`application/octet-stream`) |
| Read BTPipeOut | `GET` | `/devices/{serial}/classic/pipes/block-out?endpoint=0xA0&block_size=64&length=1024` | — |
| Lock resource | `POST` | `/resources/{id}/lock` | `{"user_id":"..."}` |
| Unlock resource | `POST` | `/resources/{id}/unlock` | `{"user_id":"..."}` |
| Heartbeat | `POST` | `/heartbeat` | `{"user_id":"..."}` |
| Disconnect | `POST` | `/disconnect` | `{"user_id":"..."}` |

---

## 10. Authoritative Sources

- **OpenAPI Spec:** [`api/openapi.yaml`](../../api/openapi.yaml) — the single source of truth for all REST endpoints
- **API Specification:** [`docs/API-Specification/ok-fpga-server-api-specification.pdf`](../API-Specification/ok-fpga-server-api-specification.pdf) — full Resource Model and Access Enforcement specification
- **Existing Python REST client:** [`python_rest_gui_common/counters_client.py`](../../client-examples/python_rest_gui_common/counters_client.py)
- **Existing Python REST demo:** [`python-rest-client/`](../../client-examples/python-rest-client/)
- **[ADR-004](../architecture/adr/ADR-004-rest-api-design.md):** REST API design decisions
- **[ADR-015](../architecture/adr/ADR-015-http-method-semantics.md):** HTTP method conventions (`PUT` = idempotent, `POST` = action)
- **[ADR-028](../architecture/adr/ADR-028-configure-fpga-api-alignment.md):** Configure FPGA API alignment (`/configure` multipart vs `/configure_from_server_path` JSON)
- **[ADR-029](../architecture/adr/ADR-029-user-identification-consistency.md):** User identification consistency (`user_id` body + `X-User-Id` header)
- **[ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md):** Consolidated resource model, registration, acquisition, locking, and enforcement (supersedes parts of ADR-020 and ADR-024)
- **ai-collaboration rules:** [`.agents/rules/ai-collaboration.md`](../../.agents/rules/ai-collaboration.md) — §4: API Specs (Single Source of Truth)
