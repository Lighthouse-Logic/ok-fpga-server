In FrontPanel Classic architecture, **Standard Pipes** (`PipeIn` / `PipeOut`) and **Block-Throttled Pipes** (`BTPipeIn` / `BTPipeOut`) both provide synchronous, multi-byte bulk data transfers between the host PC and FPGA fabric over USB endpoints `0x80`–`0xBF`.

The fundamental distinction lies in **hardware flow control (backpressure)** and how handshaking is negotiated.

| Feature | Standard Pipes (`okPipeIn` / `okPipeOut`) | Block-Throttled Pipes (`okBTPipeIn` / `okBTPipeOut`) |
| --- | --- | --- |
| **Flow Control** | **None at HDL level** (Host is absolute master) | **Hardware backpressure** via `ep_ready` |
| **Transfer Granularity** | Byte/Word stream without block boundaries | Fixed **block sizes** (e.g., 64 to 1024 bytes) |
| **HDL Handshake** | `ep_write` / `ep_read` strobe continuously | `ep_ready` checked before each block transfer begins |
| **FPGA FIFO Sizing** | Must be large enough to absorb the **entire transfer** | Only needs to be large enough to hold **one block** |
| **C++ Host API** | `WriteToPipeIn()`, `ReadFromPipeOut()` | `WriteToBlockPipeIn()`, `ReadFromBlockPipeOut()` |

---

### 1. Standard Pipes (`okPipeIn` / `okPipeOut`)

Standard pipes are continuous bulk streams where the **host PC controls the transaction timing unilaterally**.

* **Operation:** When the host calls `WriteToPipeIn(0x80, length, buffer)`, data streams across USB into the FPGA at the full interface clock rate until `length` bytes are exhausted.
* **Lack of Hardware Backpressure:** The FPGA logic cannot pause or stall the transaction midway.
* **Buffer Requirements:** The FPGA gateware must provide a FIFO buffer large enough to store the **entire transfer length**, or the user must coordinate manual software negotiation using FrontPanel Wires or Triggers before initiating the transfer to ensure the FPGA is ready.
* **Best Used For:** Fast bulk transfers where large internal BRAM/DDR buffers exist or continuous streaming data rate is guaranteed.

---

### 2. Block-Throttled Pipes (`okBTPipeIn` / `okBTPipeOut`)

BTPipes introduce **hardware-level handshaking at discrete block intervals**, allowing the FPGA to pause USB transfers automatically.

* **The `ep_ready` Signal:** In gateware, `okBTPipeIn` and `okBTPipeOut` expose an input signal named `ep_ready`.
* Before the host transmits or requests the next block of data, the USB microcontroller inspects `ep_ready`.
* If `ep_ready == 1`, the next block is transferred without interruption.
* If `ep_ready == 0`, the USB controller halts the USB bus transfer and waits until the FPGA re-asserts `ep_ready`.


* **Block Interval Sizing:** Throttling does not happen per-byte; it evaluates once per block boundary (typically 64B, 128B, 256B, 512B, or 1024B depending on the carrier and USB speed).
* **Buffer Sizing Advantage:** The FPGA only needs a small FIFO large enough to accommodate **a single block size** plus minimal margin, enabling streaming of multi-megabyte payloads without exhausting on-chip BRAM.

---

### HDL Signal Differences

```text
// Standard Pipe In
okPipeIn ep80 (
    .okHE(okHE), .okEH(okEH),
    .ep_addr(8'h80),
    .ep_write(pipe_write),       // Pulses high for each incoming data word
    .ep_dataout(pipe_data)
);

// Block-Throttled Pipe In
okBTPipeIn ep81 (
    .okHE(okHE), .okEH(okEH),
    .ep_addr(8'h81),
    .ep_write(btpipe_write),
    .ep_dataout(btpipe_data),
    .ep_ready(fifo_has_space)    // FPGA asserts when FIFO has room for >= 1 block
);

```

---

### C++ Host API Calling Convention

```cpp
// Standard Pipe: (Address, Total Length, Data Pointer)
dev->WriteToPipeIn(0x80, 1048576, buffer);

// Block-Throttled Pipe: (Address, Block Size, Total Length, Data Pointer)
// Total length must typically be an exact integer multiple of blockSize
dev->WriteToBlockPipeIn(0x81, 1024, 1048576, buffer);

```