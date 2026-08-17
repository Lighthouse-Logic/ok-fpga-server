# ADR-005: gRPC API Design — Proto3

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The ok-FPGA-Server must expose FPGAs over gRPC using proto3. The gRPC API must:

1. Mirror the FrontPanel 6.1.0 API surface (same operations as REST)
2. Provide server-streaming RPCs for hot-plug events and sensor polling
3. Handle binary data (pipes, bitfiles) efficiently via native bytes type
4. Be spec-first: the `.proto` file is defined before implementation

The gRPC server will be built with **Tonic**, gated behind the `grpc` Cargo feature flag (ADR-010), and can share the same tokio runtime as the management server and REST API.

## Decision

### Proto Package & Service Structure

```protobuf
syntax = "proto3";
package okfpga.v1;
```

Three data services, organized by concern, plus a server management service:

#### 1. `DeviceService` — Device lifecycle and configuration

```protobuf
service DeviceService {
  // Enumeration
  rpc ListDevices(ListDevicesRequest) returns (ListDevicesResponse);
  rpc GetDeviceInfo(DeviceRequest) returns (DeviceInfoResponse);
  
  // Sensors & Settings
  rpc GetDeviceSensors(DeviceRequest) returns (DeviceSensorsResponse);
  rpc GetDeviceSettings(DeviceRequest) returns (DeviceSettingsResponse);
  rpc GetDeviceSetting(GetDeviceSettingRequest) returns (DeviceSettingResponse);
  rpc SetDeviceSetting(SetDeviceSettingRequest) returns (SetDeviceSettingResponse);
  
  // SYZYGY
  rpc GetSyzygyPorts(DeviceRequest) returns (SyzygyPortsResponse);
  
  // FPGA Configuration
  rpc ConfigureFPGA(ConfigureFPGARequest) returns (ConfigureFPGAResponse);
  rpc ConfigureFPGAFromMemory(stream ConfigureFPGAChunk) returns (ConfigureFPGAResponse);
  rpc ClearFPGAConfiguration(DeviceRequest) returns (StatusResponse);
  rpc GetBitfileInfo(DeviceRequest) returns (BitfileInfoResponse);
  
  // PLL Configuration
  rpc GetPLLConfiguration(GetPLLRequest) returns (PLLConfigurationResponse);
  rpc SetPLLConfiguration(SetPLLRequest) returns (StatusResponse);
  rpc LoadDefaultPLLConfiguration(DeviceRequest) returns (StatusResponse);
  
  // Flash
  rpc FlashRead(FlashReadRequest) returns (FlashDataResponse);
  rpc FlashWrite(FlashWriteRequest) returns (StatusResponse);
  rpc FlashEraseSector(FlashEraseSectorRequest) returns (StatusResponse);
  
  // Device management
  rpc SetDeviceID(SetDeviceIDRequest) returns (StatusResponse);
  rpc SetTimeout(SetTimeoutRequest) returns (StatusResponse);
  
  // Events (server streaming)
  rpc WatchDeviceEvents(WatchDeviceEventsRequest) returns (stream DeviceEvent);
}
```

#### 2. `ClassicDataPortService` — Classic wire/trigger/pipe/register operations

```protobuf
service ClassicDataPortService {
  rpc ResetFPGA(DeviceRequest) returns (StatusResponse);
  
  // Wires
  rpc SetWireInValue(SetWireInValueRequest) returns (StatusResponse);
  rpc GetWireInValue(GetWireValueRequest) returns (WireValueResponse);
  rpc UpdateWireIns(DeviceRequest) returns (StatusResponse);
  rpc GetWireOutValue(GetWireValueRequest) returns (WireValueResponse);
  rpc UpdateWireOuts(DeviceRequest) returns (StatusResponse);
  
  // Triggers
  rpc ActivateTriggerIn(ActivateTriggerInRequest) returns (StatusResponse);
  rpc UpdateTriggerOuts(DeviceRequest) returns (StatusResponse);
  rpc IsTriggered(IsTriggeredRequest) returns (IsTriggeredResponse);
  rpc GetTriggerOutVector(GetTriggerOutVectorRequest) returns (TriggerOutVectorResponse);
  
  // Pipes
  rpc WriteToPipeIn(WriteToPipeInRequest) returns (PipeTransferResponse);
  rpc ReadFromPipeOut(ReadFromPipeOutRequest) returns (PipeDataResponse);
  rpc WriteToBlockPipeIn(WriteToBlockPipeInRequest) returns (PipeTransferResponse);
  rpc ReadFromBlockPipeOut(ReadFromBlockPipeOutRequest) returns (PipeDataResponse);
  rpc SetBTPipePollingInterval(SetBTPipePollingIntervalRequest) returns (StatusResponse);
  
  // Registers
  rpc ReadRegister(ReadRegisterRequest) returns (RegisterValueResponse);
  rpc WriteRegister(WriteRegisterRequest) returns (StatusResponse);
  rpc ReadRegisters(ReadRegistersRequest) returns (RegisterEntriesResponse);
  rpc WriteRegisters(WriteRegistersRequest) returns (StatusResponse);
}
```

#### 3. `AXIDataPortService` — AXI Lite/Full/Stream operations

```protobuf
service AXIDataPortService {
  // AXI Lite
  rpc AXILiteRead(AXILiteReadRequest) returns (AXILiteReadResponse);
  rpc AXILiteWrite(AXILiteWriteRequest) returns (StatusResponse);
  rpc AXILiteReadBulk(AXILiteReadBulkRequest) returns (AXILiteReadBulkResponse);
  rpc AXILiteWriteBulk(AXILiteWriteBulkRequest) returns (AXIOperationStatsResponse);
  rpc AXILiteReset(DeviceRequest) returns (StatusResponse);
  
  // AXI Full
  rpc AXIFullGetDatapathWidth(DeviceRequest) returns (DatapathWidthResponse);
  rpc AXIFullRead(AXIFullReadRequest) returns (AXIFullReadResponse);
  rpc AXIFullWrite(AXIFullWriteRequest) returns (AXIOperationStatsResponse);
  rpc AXIFullReset(DeviceRequest) returns (StatusResponse);
  
  // AXI Stream
  rpc AXIStreamGetDatapathWidths(DeviceRequest) returns (AXIStreamDatapathWidthsResponse);
  rpc AXIStreamRead(AXIStreamReadRequest) returns (AXIStreamReadResponse);
  rpc AXIStreamWrite(AXIStreamWriteRequest) returns (AXIStreamWriteResponse);
  rpc AXIStreamReset(DeviceRequest) returns (StatusResponse);
}
```

#### 4. `ServerService` — Server and service lifecycle management

```protobuf
service ServerService {
  // Server status
  rpc GetServerStatus(google.protobuf.Empty) returns (ServerStatusResponse);
  rpc RestartServer(google.protobuf.Empty) returns (StatusResponse);

  // Service lifecycle (ADR-010)
  rpc ListServices(google.protobuf.Empty) returns (ListServicesResponse);
  rpc StartService(ServiceRequest) returns (ServiceResponse);
  rpc StopService(ServiceRequest) returns (ServiceResponse);
  rpc RestartService(ServiceRequest) returns (ServiceResponse);
}

message ServiceRequest {
  string name = 1;  // e.g., "rest", "grpc", "mqtt", "mdns"
}

message ServiceResponse {
  string name = 1;
  string status = 2;  // "running", "stopped", "starting", etc.
  string message = 3;
}

message ListServicesResponse {
  repeated ServiceInfo services = 1;
}

message ServiceInfo {
  string name = 1;
  string status = 2;
  bool compiled = 3;   // whether the feature flag was enabled
  bool enabled = 4;    // whether enabled in TOML config
}

message ServerStatusResponse {
  string api_version = 1;
  uint64 uptime_secs = 2;
  int32 device_count = 3;
  repeated ServiceInfo services = 4;
}
```

### Key Message Types

```protobuf
message DeviceRequest {
  string serial = 1;  // 10-char device serial
}

message StatusResponse {
  ErrorCode error_code = 1;
  string message = 2;
}

message ErrorCode {
  int32 code = 1;       // maps to ok_ErrorCode values
  string name = 2;      // e.g. "NoError", "Failed", "Timeout"
}

message DeviceEvent {
  oneof event {
    DeviceAdded device_added = 1;
    DeviceRemoved device_removed = 2;
    BitfileProgrammed bitfile_programmed = 3;
    ResourceStateChanged resource_state_changed = 4;
  }
}

message DeviceInfoResponse {
  string serial = 1;
  string device_id = 2;
  string product_name = 3;
  int32 product_id = 4;
  DeviceInterface device_interface = 5;
  USBSpeed usb_speed = 6;
  int32 device_major_version = 7;
  int32 device_minor_version = 8;
  int32 wire_width = 9;
  int32 trigger_width = 10;
  int32 pipe_width = 11;
  FPGAVendor fpga_vendor = 12;
  DataportType dataport = 13;
  bool is_front_panel_enabled = 14;
  optional BitfileInfo bitfile = 15;
}

// Error cluster pattern (from project goals proto example)
message ErrorCluster {
  bool status = 1;    // true if error occurred
  int32 code = 2;
  string source = 3;
}
```

### Streaming for Bulk Data

- `ConfigureFPGAFromMemory` uses **client streaming** — the bitfile is sent in chunks
- `WatchDeviceEvents` uses **server streaming** — events pushed as they occur
- Pipe read/write use `bytes` type for efficient binary transfer (no base64 encoding needed)

### Error Handling

All RPCs return an `ErrorCode` message that maps directly to `ok_ErrorCode`:

| ok_ErrorCode | gRPC code | Proto enum |
|-------------|-----------|------------|
| `ok_NoError` (0) | OK | `NO_ERROR` |
| `ok_Failed` (-1) | INTERNAL | `FAILED` |
| `ok_Timeout` (-2) | DEADLINE_EXCEEDED | `TIMEOUT` |
| `ok_DeviceNotOpen` (-8) | NOT_FOUND | `DEVICE_NOT_OPEN` |
| `ok_InvalidEndpoint` (-9) | INVALID_ARGUMENT | `INVALID_ENDPOINT` |
| ... | ... | ... |

### Proto File Location

The proto file will be created at `proto/okfpga/v1/okfpga.proto` before any implementation code.

## Consequences

### Positive
- **Spec-first** — `.proto` file drives code generation via `tonic-build`
- **Efficient binary transfer** — native `bytes` type for pipes/bitfiles (no base64 overhead)
- **Server streaming** — natural fit for hot-plug events and continuous sensor monitoring
- **Client streaming** — efficient bitfile upload in chunks
- **Service separation** — three services cleanly separate device management from data operations

### Negative
- **Proto maintenance** — the `.proto` file must stay synchronized with the REST OpenAPI spec
- **Large message catalog** — ~50+ message types reflecting the FrontPanel API breadth

## References

- [Tonic gRPC framework](https://docs.rs/tonic)
- [Proto3 Language Guide](https://protobuf.dev/programming-guides/proto3/)
- [Project goals proto example](../../ok-FPGA-Server_Project-goals.txt) — lines 104-144
- [ADR-010 Service Manager](ADR-010-service-manager.md) — feature flags and service lifecycle
