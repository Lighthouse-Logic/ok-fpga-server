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
package okFPGAService.v1;
```

Three data services, organized by concern, plus a server management service:

#### 1. `DeviceService` — Device lifecycle, configuration, resources, users, and events

```protobuf
service DeviceService {
  // Enumeration
  rpc ListDevices(ListDevicesRequest) returns (ListDevicesResponse);
  rpc GetDeviceInfo(DeviceRequest) returns (DeviceInfoResponse);
  
  // Sensors & Settings
  rpc GetDeviceSensors(DeviceRequest) returns (DeviceSensorsResponse);
  rpc GetDeviceSettings(DeviceRequest) returns (DeviceSettingsResponse);
  rpc GetDeviceSetting(GetDeviceSettingRequest) returns (DeviceSettingResponse);
  rpc SetDeviceSetting(SetDeviceSettingRequest) returns (StatusResponse);
  
  // SYZYGY
  rpc GetSyzygyPorts(DeviceRequest) returns (SyzygyPortsResponse);
  
  // FPGA Configuration
  rpc ConfigureFPGA(ConfigureFPGARequest) returns (ConfigureFPGAResponse);
  rpc ConfigureFPGAFromMemory(stream ConfigureFPGAChunk) returns (ConfigureFPGAResponse);
  rpc ClearFPGAConfiguration(DeviceRequest) returns (StatusResponse);
  rpc GetBitfileInfo(DeviceRequest) returns (BitfileInfoResponse);
  rpc GetDataportType(DeviceRequest) returns (DataportTypeResponse);
  
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
  
  // Server
  rpc GetServerStatus(ServerStatusRequest) returns (ServerStatus);
  
  // Events (server streaming)
  rpc WatchDeviceEvents(WatchDeviceEventsRequest) returns (stream DeviceEvent);
  
  // Resource Management (ADR-020)
  rpc ListResources(ListResourcesRequest) returns (ListResourcesResponse);
  rpc GetResource(ResourceRequest) returns (ResourceResponse);
  rpc CreateResource(CreateResourceRequest) returns (ResourceResponse);
  rpc DeleteResource(ResourceRequest) returns (StatusResponse);
  rpc AcquireResource(AcquireResourceRequest) returns (ResourceResponse);
  rpc ReleaseResource(ReleaseResourceRequest) returns (StatusResponse);
  rpc LockResource(LockResourceRequest) returns (ResourceResponse);
  rpc UnlockResource(LockResourceRequest) returns (ResourceResponse);
  
  // Users
  rpc ListUsers(ListUsersRequest) returns (ListUsersResponse);
  rpc GetUser(UserRequest) returns (UserResponse);
  
  // Heartbeat (ADR-020 §5)
  rpc Ping(PingRequest) returns (PingResponse);
  rpc Disconnect(DisconnectRequest) returns (DisconnectResponse);
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
  rpc GetTriggerOutVector(TriggerOutVectorRequest) returns (TriggerOutVectorResponse);
  
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
  rpc AXILiteWriteBulk(AXILiteWriteBulkRequest) returns (AXIOperationStats);
  rpc AXILiteReset(DeviceRequest) returns (StatusResponse);
  
  // AXI Full
  rpc AXIFullGetDatapathWidth(DeviceRequest) returns (DatapathWidthResponse);
  rpc AXIFullRead(AXIFullReadRequest) returns (AXIFullReadResponse);
  rpc AXIFullWrite(AXIFullWriteRequest) returns (AXIOperationStats);
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
  rpc GetServerStatus(StatusResponse) returns (ServerStatusResponse);
  rpc RestartServer(StatusResponse) returns (StatusResponse);

  // Service lifecycle (ADR-010)
  rpc ListServices(StatusResponse) returns (ListServicesResponse);
  rpc StartService(ServiceRequest) returns (ServiceOperationResponse);
  rpc StopService(ServiceRequest) returns (ServiceOperationResponse);
  rpc RestartService(ServiceRequest) returns (ServiceOperationResponse);
}
```

### Key Message Types

```protobuf
// Every response includes an ErrorCluster to indicate success or failure.
message ErrorCluster {
  bool status = 1;    // true if error occurred
  int32 code = 2;     // maps to ok_ErrorCode values (0 = NoError)
  string source = 3;  // human-readable error source or description
}

message DeviceRequest {
  string serial = 1;  // 10-char device serial
}

message StatusResponse {
  ErrorCluster error = 1;
}

message DeviceInfoResponse {
  DeviceInfo device = 1;     // Full device info record
  ErrorCluster error = 2;
}

message DeviceInfo {
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
  int32 register_address_width = 12;
  int32 register_data_width = 13;
  FPGAVendor fpga_vendor = 14;
  DataportType dataport = 15;
  bool is_front_panel_enabled = 16;
  optional BitfileInfo bitfile = 17;
}

message DeviceEvent {
  string timestamp = 1;      // ISO 8601 timestamp
  oneof event {
    DeviceAdded device_added = 2;
    DeviceRemoved device_removed = 3;
    BitfileProgrammed bitfile_programmed = 4;
    ResourceStateChanged resource_state_changed = 5;
  }
}

// Server Management (ADR-010)
message ServiceRequest {
  string name = 1;  // e.g., "rest", "grpc", "mqtt", "mdns"
}

message ServiceOperationResponse {
  ErrorCluster error = 1;
  ServiceInfo service = 2;   // Updated service info after operation
}

message ServiceInfo {
  string name = 1;
  string status = 2;    // "running", "stopped", "starting", etc.
  bool compiled = 3;    // whether the feature flag was enabled
  bool enabled = 4;     // whether enabled in TOML config
}

message ListServicesResponse {
  ErrorCluster error = 1;
  repeated ServiceInfo services = 2;
}

message ServerStatusResponse {
  ErrorCluster error = 1;
  string api_version = 2;
  uint64 uptime_secs = 3;
  int32 device_count = 4;
  repeated ServiceInfo services = 5;
}
```

### Streaming for Bulk Data

- `ConfigureFPGAFromMemory` uses **client streaming** — the bitfile is sent in chunks
- `WatchDeviceEvents` uses **server streaming** — events pushed as they occur
- Pipe read/write use `bytes` type for efficient binary transfer (no base64 encoding needed)

### Error Handling

All RPCs include an `ErrorCluster` in their response message. The `ErrorCluster`
pattern maps directly to `ok_ErrorCode`:

| ok_ErrorCode | gRPC code | ErrorCluster.code |
|-------------|-----------|-------------------|
| `ok_NoError` (0) | OK | `0` |
| `ok_Failed` (-1) | INTERNAL | `-1` |
| `ok_Timeout` (-2) | DEADLINE_EXCEEDED | `-2` |
| `ok_DeviceNotOpen` (-8) | NOT_FOUND | `-8` |
| `ok_InvalidEndpoint` (-9) | INVALID_ARGUMENT | `-9` |
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

## Addendum A: Resource & User RPCs (2026-08-17)

See [ADR-020](ADR-020-client-side-resource-registration.md) for full rationale.

The following RPCs were added to `DeviceService`:

- **Resource CRUD**: `CreateResource`, `DeleteResource` — client-side resource
  registration after bitfile programming
- **Resource lifecycle**: `ListResources`, `GetResource`, `AcquireResource`,
  `ReleaseResource` — resource discovery and acquisition
- **Lock/Unlock**: `LockResource`, `UnlockResource` — temporary exclusive access
  to shared resources (ADR-020 §3)
- **Users**: `ListUsers`, `GetUser` — connected user session management
- **Heartbeat**: `Ping` — client liveness heartbeat (ADR-020 §5)
- **Disconnect**: `Disconnect` — clean session teardown (Phase 25-Fix-H, F10)

The service listings above have been updated to include all RPCs.

## Addendum B: Endpoint Access Enforcement (2026-08-18)

See [ADR-024](ADR-024-endpoint-access-enforcement.md).

When `endpoint_enforcement` is set to `"enforce"`, Classic and AXI data port
RPCs may return `PERMISSION_DENIED` if the calling user does not hold a resource
covering the requested endpoint. No new RPCs are added — enforcement is
interceptor-level.

## References

- [Tonic gRPC framework](https://docs.rs/tonic)
- [Proto3 Language Guide](https://protobuf.dev/programming-guides/proto3/)
- [Project goals proto example](../../ok-FPGA-Server_Project-goals.txt) — lines 104-144
- [ADR-010 Service Manager](ADR-010-service-manager.md) — feature flags and service lifecycle
- [ADR-020 Client-Side Resource Registration](ADR-020-client-side-resource-registration.md) — resource CRUD, lock/unlock, heartbeat
- [ADR-024 Endpoint Access Enforcement](ADR-024-endpoint-access-enforcement.md) — runtime endpoint gating
