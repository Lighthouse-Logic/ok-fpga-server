# `OpalKelly::FPGADataPortAXI` Class Reference

**Type:** Abstract Class

The `FPGADataPortAXI` class provides access to the AXI data transfer interfaces on supported FPGA devices by aggregating the AXI-Lite, AXI-Full, and AXI-Stream protocol interfaces.

---

### Accessing the Class

An instance of `FPGADataPortAXI` is obtained by calling:

```cpp
OpalKelly::FrontPanel::GetFPGADataPortAXI()

```

---

### Public Member Functions

| Return Type | Function | Description |
| --- | --- | --- |
| `virtual OpalKelly::FrontPanelAXILite &` | `GetAXILite(void) = 0` | Returns a reference to the **AXI-Lite** sub-interface. |
| `virtual OpalKelly::FrontPanelAXIFull &` | `GetAXIFull(void) = 0` | Returns a reference to the **AXI-Full** sub-interface. |
| `virtual OpalKelly::FrontPanelAXIStream &` | `GetAXIStream(void) = 0` | Returns a reference to the **AXI-Stream** sub-interface. |


# `OpalKelly::FrontPanelAXILite` Class Reference

**Type:** Abstract Class

The `FrontPanelAXILite` class provides 32-bit register-level read/write access to the FPGA over the AXI-Lite protocol. It supports single-register and multi-register (bulk) read/write operations with optional timeout overrides and operation statistics tracking.

---

### Accessing the Class

An instance of this class is obtained via:

```cpp
OpalKelly::FPGADataPortAXI::GetAXILite()

```

---

### Public Member Functions

| Return Type | Function Signature | Description |
| --- | --- | --- |
| `virtual ErrorCode` | `Read(UINT64 address, UINT32 *pBuffer, std::size_t count, UINT32 timeoutMilliseconds, UINT64 &transferByteCount) = 0` | Reads multiple 32-bit registers starting at the given address with an explicit timeout. |
| `virtual ErrorCode` | `Write(UINT64 address, const UINT32 *pData, std::size_t count, UINT32 timeoutMilliseconds, okTOperationStatistics &operationStats) = 0` | Writes multiple 32-bit registers starting at the given address with an explicit timeout. |
| `virtual ErrorCode` | `Reset(void) = 0` | Resets the AXI-Lite interface. |
| `ErrorCode` | `Read(UINT64 address, UINT32 &value, UINT32 timeoutMilliseconds, UINT64 &transferByteCount)` | Reads a single register with an explicit timeout. |
| `ErrorCode` | `Write(UINT64 address, UINT32 value, UINT32 timeoutMilliseconds, okTOperationStatistics &operationStats)` | Writes a single register with an explicit timeout. |
| `ErrorCode` | `Read(UINT64 address, UINT32 *pBuffer, std::size_t count, UINT64 &transferByteCount)` | Reads multiple registers using the default timeout. |
| `ErrorCode` | `Write(UINT64 address, const UINT32 *pData, std::size_t count, okTOperationStatistics &operationStats)` | Writes multiple registers using the default timeout. |
| `ErrorCode` | `Read(UINT64 address, UINT32 &value)` | Reads a single register using the default timeout. |
| `ErrorCode` | `Write(UINT64 address, UINT32 value)` | Writes a single register using the default timeout. |


# `OpalKelly::FrontPanelAXIFull` Class Reference

**Type:** Abstract Class

The `FrontPanelAXIFull` class provides byte-level, burst-capable memory-mapped read and write access to the FPGA over the AXI-Full protocol. It supports configurable burst lengths, variable datapath widths, explicit timeout handling, and operation statistics tracking.

---

### Accessing the Class

An instance of this class is obtained via:

```cpp
OpalKelly::FPGADataPortAXI::GetAXIFull()

```

---

### Public Member Functions

| Return Type | Function Signature | Description |
| --- | --- | --- |
| `virtual UINT8` | `GetDatapathWidthByteCount(void) const = 0` | Returns the datapath width in bytes. |
| `virtual ErrorCode` | `Read(UINT64 address, unsigned char *pBuffer, std::size_t bufferSize, UINT32 burst, UINT32 timeoutMilliseconds, UINT64 &transferByteCount) = 0` | Reads data from the given address with specified burst length and timeout. |
| `virtual ErrorCode` | `Write(UINT64 address, const unsigned char *pData, std::size_t dataSize, UINT32 burst, UINT32 timeoutMilliseconds, okTOperationStatistics &operationStats) = 0` | Writes data to the given address with specified burst length and timeout. |
| `virtual ErrorCode` | `Reset(void) = 0` | Resets the AXI-Full interface. |
| `ErrorCode` | `Read(UINT64 address, unsigned char *pBuffer, std::size_t bufferSize, UINT64 &transferByteCount)` | Reads data using the default burst length and default timeout. |
| `ErrorCode` | `Write(UINT64 address, const unsigned char *pData, std::size_t dataSize, okTOperationStatistics &operationStats)` | Writes data using the default burst length and default timeout. |


# `OpalKelly::FrontPanelAXIStream` Class Reference

**Type:** Abstract Class

The `FrontPanelAXIStream` class provides streaming read and write data transfers to and from the FPGA over the AXI-Stream protocol. Unlike AXI-Lite and AXI-Full, stream transfers are addressless and support distinct read and write datapath widths as well as explicit timeout overrides.

---

### Accessing the Class

An instance of this class is obtained via:

```cpp
OpalKelly::FPGADataPortAXI::GetAXIStream()

```

---

### Public Member Functions

| Return Type | Function Signature | Description |
| --- | --- | --- |
| `virtual UINT8` | `GetWriteDatapathWidthByteCount(void) const = 0` | Returns the write datapath width in bytes. |
| `virtual UINT8` | `GetReadDatapathWidthByteCount(void) const = 0` | Returns the read datapath width in bytes. |
| `virtual ErrorCode` | `Read(unsigned char *pBuffer, std::size_t bufferSize, UINT32 timeoutMilliseconds, UINT64 &transferByteCount) = 0` | Reads data from the stream with an explicit timeout. |
| `virtual ErrorCode` | `Write(const unsigned char *pData, std::size_t dataSize, UINT32 timeoutMilliseconds, UINT64 &transferByteCount) = 0` | Writes data to the stream with an explicit timeout. |
| `virtual ErrorCode` | `Reset(void) = 0` | Resets the AXI-Stream interface. |
| `ErrorCode` | `Read(unsigned char *pBuffer, std::size_t bufferSize, UINT64 &transferByteCount)` | Reads data from the stream using the default timeout. |
| `ErrorCode` | `Write(const unsigned char *pData, std::size_t dataSize, UINT64 &transferByteCount)` | Writes data to the stream using the default timeout. |




# Summary of the differences between AXI-Lite, AXI-Full and AXI-Stream, 
And what should be considered if exposing each one of these interface types from a server connected to such FPGA hardware and exposing the interfaces via REST (with SSE), or gRPC. 

| Interface Type | Addressing | Transfer Unit & Behavior | Primary Use Case |
| --- | --- | --- | --- |
| **AXI-Lite** | Explicit 32/64-bit addresses | Single 32-bit register reads/writes (low throughput, low latency) | Control & Status Registers (CSRs), peripheral configuration |
| **AXI-Full** | Explicit 32/64-bit addresses | Burst-capable, high-throughput memory-mapped byte arrays | Accessing large memory buffers (BRAM, DDR4), bulk block transfers |
| **AXI-Stream** | **Addressless** (FIFO / point-to-point) | Continuous ordered byte streaming (`tdata`, `tvalid`, `tready`, `tlast`) | DSP pipelines, ADC/DAC sample streams, packet processing |

---

### Exposing AXI-Lite

* **Characteristics:** Low-payload, discrete, request-response register operations.
* **REST & SSE Design:**
* Map registers naturally to standard HTTP verbs: `GET /api/v1/devices/{serial}/axi-lite/{address}` to read, and `PUT /api/v1/devices/{serial}/axi-lite/{address}` with a JSON body (`{"value": 305419896}`) to write.
* Use **SSE** to stream periodic register polling updates or status register push notifications to web dashboards.


* **gRPC Design:**
* Implement simple unary RPCs: `rpc ReadRegister(AxiLiteReadRequest) returns (AxiLiteReadResponse)` and `rpc WriteRegister(AxiLiteWriteRequest) returns (AxiLiteWriteResponse)`.
* Support batch/multi-register operations via vector requests to minimize round-trip network latency.



---

### Exposing AXI-Full

* **Characteristics:** Memory-mapped bulk payloads where the client requests specific address ranges and offsets.
* **REST & SSE Design:**
* `GET /api/v1/devices/{serial}/axi-full/{address}?length=4096` returning `application/octet-stream` (binary) or base64-encoded JSON for reads.
* `PUT /api/v1/devices/{serial}/axi-full/{address}` accepting raw binary bytes in the request body.
* **Considerations:** Chunk large transfers to prevent HTTP request timeouts or buffer memory exhaustion in your web server layer.


* **gRPC Design:**
* For small buffers, use unary RPCs with `bytes data = 3;`.
* For large DMA-sized memory dumps (e.g., megabytes to gigabytes), use client-streaming (`rpc WriteBlock(stream AxiFullChunk)`) and server-streaming (`rpc ReadBlock(AxiFullRequest) returns (stream AxiFullChunk)`) to pipeline data chunks cleanly across the wire.



---

### Exposing AXI-Stream

* **Characteristics:** High-speed, unidirectional or bidirectional continuous byte streams without addresses.
* **REST & SSE Design:**
* **Reads (FPGA ➔ Client):** Perfect fit for **Server-Sent Events (SSE)** or raw chunked HTTP streaming (`Transfer-Encoding: chunked`). The server continuously pushes incoming stream frames as they arrive from the hardware.
* **Writes (Client ➔ FPGA):** `POST /api/v1/devices/{serial}/axi-stream` carrying payload batches into the FPGA FIFO.
* **Limitations:** SSE is strictly server-to-client; full-duplex interactive streaming is awkward over pure REST/SSE.


* **gRPC Design:**
* Ideal model: utilize **Bidirectional Streaming** (`rpc StreamData(stream AxiStreamWriteRequest) returns (stream AxiStreamReadResponse)`).
* Enables zero-copy serialization, high throughput over HTTP/2 framing, and native backpressure management (preventing buffer overflow when the host or FPGA FIFO is full).



---

### Key Server Architecture & Concurrency Considerations

1. **Hardware Locking & Serialization:** The underlying USB controller/FPGA data port cannot handle overlapping asynchronous requests across threads without hardware bus corruption. Wrap access to each sub-interface in an exclusive mutex/channel worker per physical device.
2. **Backpressure & Buffer Overruns:** For high-speed AXI-Stream feeds, client network drops or slow consumers will quickly exhaust server RAM. Enforce strict bounded channel capacities (dropping frames or pausing hardware transfers when buffers saturate).
3. **Payload Framing & Encodings:** While JSON or SSE is suitable for AXI-Lite and telemetry, use raw Protobuf byte slices or binary octet streams for AXI-Full and AXI-Stream to avoid base64 encoding overhead and CPU serialization bottlenecks.
