# FrontPanel 6.1.0 C API Surface Catalog

**Purpose**: Complete inventory of every C function, struct, enum, and constant that the Rust FFI layer must bind, organized by subsystem. This catalog is derived directly from [okFrontPanel.h](../../FrontPanel-6.1.0/README.md).

**Exclusions**: Functions marked ❌ are excluded from the FFI layer (USB 2.0-only, Lua scripting, or Android-only).

---

## 1. Version & Library Loading

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanel_GetAPIVersionMajor` | `() -> int` | ✅ |
| `okFrontPanel_GetAPIVersionMinor` | `() -> int` | ✅ |
| `okFrontPanel_GetAPIVersionMicro` | `() -> int` | ✅ |
| `okFrontPanel_GetAPIVersionString` | `() -> const char*` | ✅ |
| `okFrontPanel_CheckAPIVersion` | `(int, int, int) -> Bool` | ✅ |
| `okFrontPanel_TryLoadLib` | `() -> Bool` (inline) | ❌ N/A for runtime loading |

**Compile-time constants**:
- `OK_API_VERSION_MAJOR` = 6
- `OK_API_VERSION_MINOR` = 1
- `OK_API_VERSION_MICRO` = 0
- `OK_API_VERSION_STRING` = "6.1.0"

---

## 2. Error Handling

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okError_GetMessage` | `(const okError*) -> const char*` | ✅ |
| `okError_Free` | `(okError*) -> void` | ✅ |
| `okFrontPanel_GetErrorMessage` | `(ok_ErrorCode) -> const char*` | ✅ |
| `okFrontPanel_GetLastError` | `(okFrontPanel_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_GetLastErrorMessage` | `(okFrontPanel_HANDLE) -> const char*` | ✅ |

---

## 3. FrontPanel Device (Core)

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanel_Construct` | `() -> okFrontPanel_HANDLE` | ✅ |
| `okFrontPanel_ConstructFromFD` | `(int) -> okFrontPanel_HANDLE` | ❌ Android only |
| `okFrontPanel_Destruct` | `(okFrontPanel_HANDLE)` | ✅ |
| `okFrontPanel_IsOpen` | `(HANDLE) -> Bool` | ✅ |
| `okFrontPanel_IsRemote` | `(HANDLE) -> Bool` | ✅ |
| `okFrontPanel_Close` | `(HANDLE)` | ✅ |
| `okFrontPanel_SetTimeout` | `(HANDLE, int)` | ✅ |
| `okFrontPanel_SetDeviceID` | `(HANDLE, const char*)` | ✅ |
| `okFrontPanel_GetDeviceInfoWithSize` | `(HANDLE, okTDeviceInfo*, unsigned)` | ✅ |
| `okFrontPanel_IsFrontPanelEnabled` | `(HANDLE) -> Bool` | ✅ |
| `okFrontPanel_GetLastTransferLength` | `(HANDLE) -> long` | ✅ |

**Note**: `okFrontPanel_GetDeviceInfo` is a macro that calls `okFrontPanel_GetDeviceInfoWithSize`.

---

## 4. FPGA Configuration

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanel_ConfigureFPGA` | `(HANDLE, const char*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_ConfigureFPGAWithReset` | `(HANDLE, const char*, const okTFPGAResetProfile*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_ConfigureFPGAFromMemory` | `(HANDLE, const unsigned char*, unsigned long) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_ConfigureFPGAFromMemoryWithProgress` | `(HANDLE, const unsigned char*, unsigned long, okTProgressCallback, void*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_ConfigureFPGAFromMemoryWithReset` | `(HANDLE, const unsigned char*, unsigned long, const okTFPGAResetProfile*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_ConfigureFPGAFromFlash` | `(HANDLE, unsigned long) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_ClearFPGAConfiguration` | `(HANDLE) -> ok_ErrorCode` | ✅ |

---

## 5. FPGA Reset Profile

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanel_GetFPGAResetProfile` | `(HANDLE, ok_FPGAConfigurationMethod, okTFPGAResetProfile*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_GetFPGAResetProfileWithSize` | `(HANDLE, ok_FPGAConfigurationMethod, okTFPGAResetProfile*, unsigned) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_SetFPGAResetProfile` | `(HANDLE, ok_FPGAConfigurationMethod, const okTFPGAResetProfile*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_SetFPGAResetProfileWithSize` | `(HANDLE, ok_FPGAConfigurationMethod, const okTFPGAResetProfile*, unsigned) -> ok_ErrorCode` | ✅ |

---

## 6. Data Port Acquisition

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanel_GetFPGADataPortClassic` | `(HANDLE, okFPGADataPortClassic_HANDLE*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_GetFPGADataPortAXI` | `(HANDLE, okFPGADataPortAXI_HANDLE*) -> ok_ErrorCode` | ✅ |

---

## 7. Classic Data Port Operations

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFPGADataPortClassic_ResetFPGA` | `(HANDLE) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_SetWireInValue` | `(HANDLE, int ep, unsigned long val, unsigned long mask) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_GetWireInValue` | `(HANDLE, int epAddr, UINT32* val) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_UpdateWireIns` | `(HANDLE) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_GetWireOutValue` | `(HANDLE, int epAddr) -> unsigned long` | ✅ |
| `okFPGADataPortClassic_UpdateWireOuts` | `(HANDLE) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_ActivateTriggerIn` | `(HANDLE, int epAddr, int bit) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_UpdateTriggerOuts` | `(HANDLE) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_IsTriggered` | `(HANDLE, int epAddr, unsigned long mask) -> Bool` | ✅ |
| `okFPGADataPortClassic_GetTriggerOutVector` | `(HANDLE, int epAddr) -> UINT32` | ✅ |
| `okFPGADataPortClassic_WriteToPipeIn` | `(HANDLE, int epAddr, long length, const unsigned char* data) -> long` | ✅ |
| `okFPGADataPortClassic_ReadFromPipeOut` | `(HANDLE, int epAddr, long length, unsigned char* data) -> long` | ✅ |
| `okFPGADataPortClassic_WriteToBlockPipeIn` | `(HANDLE, int epAddr, int blockSize, long length, const unsigned char* data) -> long` | ✅ |
| `okFPGADataPortClassic_ReadFromBlockPipeOut` | `(HANDLE, int epAddr, int blockSize, long length, unsigned char* data) -> long` | ✅ |
| `okFPGADataPortClassic_SetBTPipePollingInterval` | `(HANDLE, int interval) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_ReadRegister` | `(HANDLE, UINT32 addr, UINT32* data) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_WriteRegister` | `(HANDLE, UINT32 addr, UINT32 data) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_ReadRegisters` | `(HANDLE, unsigned num, okTRegisterEntry* regs) -> ok_ErrorCode` | ✅ |
| `okFPGADataPortClassic_WriteRegisters` | `(HANDLE, unsigned num, const okTRegisterEntry* regs) -> ok_ErrorCode` | ✅ |

**Total: 19 functions**

---

## 8. AXI Data Port

### AXI Data Port Root

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFPGADataPortAXI_GetAXILite` | `(HANDLE) -> okFrontPanelAXILite_HANDLE` | ✅ |
| `okFPGADataPortAXI_GetAXIFull` | `(HANDLE) -> okFrontPanelAXIFull_HANDLE` | ✅ |
| `okFPGADataPortAXI_GetAXIStream` | `(HANDLE) -> okFrontPanelAXIStream_HANDLE` | ✅ |

### AXI Lite

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanelAXILite_Read` | `(HANDLE, UINT64 address, UINT32* pValue) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_Write` | `(HANDLE, UINT64 address, UINT32 value) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_ReadBulk` | `(HANDLE, UINT64 addr, UINT32* buf, UINT64 count, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_WriteBulk` | `(HANDLE, UINT64 addr, const UINT32* data, UINT64 count, okTOperationStatistics*) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_ReadEx` | `(HANDLE, UINT64 addr, UINT32* pValue, UINT32 timeoutMs) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_WriteEx` | `(HANDLE, UINT64 addr, UINT32 value, UINT32 timeoutMs) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_ReadBulkEx` | `(HANDLE, UINT64 addr, UINT32* buf, UINT64 count, UINT32 timeoutMs, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_WriteBulkEx` | `(HANDLE, UINT64 addr, const UINT32* data, UINT64 count, UINT32 timeoutMs, okTOperationStatistics*) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXILite_Reset` | `(HANDLE) -> ok_ErrorCode` | ✅ |

### AXI Full

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanelAXIFull_GetDatapathWidthByteCount` | `(HANDLE) -> UINT8` | ✅ |
| `okFrontPanelAXIFull_Read` | `(HANDLE, UINT64 addr, unsigned char* buf, UINT64 bufSize, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIFull_Write` | `(HANDLE, UINT64 addr, const unsigned char* data, UINT64 dataSize, okTOperationStatistics*) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIFull_ReadEx` | `(HANDLE, UINT64 addr, unsigned char* buf, UINT64 bufSize, UINT32 burst, UINT32 timeoutMs, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIFull_WriteEx` | `(HANDLE, UINT64 addr, const unsigned char* data, UINT64 dataSize, UINT32 burst, UINT32 timeoutMs, okTOperationStatistics*) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIFull_Reset` | `(HANDLE) -> ok_ErrorCode` | ✅ |

### AXI Stream

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanelAXIStream_GetWriteDatapathWidthByteCount` | `(HANDLE) -> UINT8` | ✅ |
| `okFrontPanelAXIStream_GetReadDatapathWidthByteCount` | `(HANDLE) -> UINT8` | ✅ |
| `okFrontPanelAXIStream_Read` | `(HANDLE, unsigned char* buf, UINT64 bufSize, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIStream_Write` | `(HANDLE, const unsigned char* data, UINT64 dataSize, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIStream_ReadEx` | `(HANDLE, unsigned char* buf, UINT64 bufSize, UINT32 timeoutMs, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIStream_WriteEx` | `(HANDLE, const unsigned char* data, UINT64 dataSize, UINT32 timeoutMs, UINT64* transferBytes) -> ok_ErrorCode` | ✅ |
| `okFrontPanelAXIStream_Reset` | `(HANDLE) -> ok_ErrorCode` | ✅ |

**Total AXI: 22 functions**

---

## 9. Device Settings

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okDeviceSettings_Construct` | `() -> okDeviceSettings_HANDLE` | ✅ |
| `okDeviceSettings_Destruct` | `(HANDLE)` | ✅ |
| `okDeviceSettings_GetString` | `(HANDLE, const char* key, int length, char* buf) -> ok_ErrorCode` | ✅ |
| `okDeviceSettings_SetString` | `(HANDLE, const char* key, const char* buf) -> ok_ErrorCode` | ✅ |
| `okDeviceSettings_GetInt` | `(HANDLE, const char* key, UINT32* value) -> ok_ErrorCode` | ✅ |
| `okDeviceSettings_SetInt` | `(HANDLE, const char* key, UINT32 value) -> ok_ErrorCode` | ✅ |
| `okDeviceSettings_Delete` | `(HANDLE, const char* key) -> ok_ErrorCode` | ✅ |
| `okDeviceSettings_Save` | `(HANDLE) -> ok_ErrorCode` | ✅ |
| `okDeviceSettings_List` | `(HANDLE, okDeviceSettingNames_HANDLE names) -> ok_ErrorCode` | ✅ |
| `okDeviceSettingNames_Construct` | `() -> okDeviceSettingNames_HANDLE` | ✅ |
| `okDeviceSettingNames_Destruct` | `(HANDLE)` | ✅ |
| `okDeviceSettingNames_GetCount` | `(HANDLE) -> int` | ✅ |
| `okDeviceSettingNames_Get` | `(HANDLE, int n) -> const char*` | ✅ |
| `okFrontPanel_GetDeviceSettings` | `(okFrontPanel_HANDLE, okDeviceSettings_HANDLE) -> ok_ErrorCode` | ✅ |

---

## 10. Device Sensors

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okDeviceSensors_Construct` | `() -> okDeviceSensors_HANDLE` | ✅ |
| `okDeviceSensors_Destruct` | `(HANDLE)` | ✅ |
| `okDeviceSensors_GetSensorCount` | `(HANDLE) -> int` | ✅ |
| `okDeviceSensors_GetSensor` | `(HANDLE, int n) -> okTDeviceSensor` | ✅ |
| `okFrontPanel_GetDeviceSensors` | `(okFrontPanel_HANDLE, okDeviceSensors_HANDLE) -> ok_ErrorCode` | ✅ |

---

## 11. PLL Configuration

### PLL22150

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okPLL22150_Construct` | `() -> okPLL22150_HANDLE` | ✅ |
| `okPLL22150_Destruct` | `(HANDLE)` | ✅ |
| `okPLL22150_SetCrystalLoad` | `(HANDLE, double)` | ✅ |
| `okPLL22150_SetReference` | `(HANDLE, double, Bool)` | ✅ |
| `okPLL22150_GetReference` | `(HANDLE) -> double` | ✅ |
| `okPLL22150_SetVCOParameters` | `(HANDLE, int, int) -> Bool` | ✅ |
| `okPLL22150_GetVCOP` | `(HANDLE) -> int` | ✅ |
| `okPLL22150_GetVCOQ` | `(HANDLE) -> int` | ✅ |
| `okPLL22150_GetVCOFrequency` | `(HANDLE) -> double` | ✅ |
| `okPLL22150_SetDiv1` | `(HANDLE, ok_DividerSource, int)` | ✅ |
| `okPLL22150_SetDiv2` | `(HANDLE, ok_DividerSource, int)` | ✅ |
| `okPLL22150_GetDiv1Source` | `(HANDLE) -> ok_DividerSource` | ✅ |
| `okPLL22150_GetDiv2Source` | `(HANDLE) -> ok_DividerSource` | ✅ |
| `okPLL22150_GetDiv1Divider` | `(HANDLE) -> int` | ✅ |
| `okPLL22150_GetDiv2Divider` | `(HANDLE) -> int` | ✅ |
| `okPLL22150_SetOutputSource` | `(HANDLE, int, ok_ClockSource_22150)` | ✅ |
| `okPLL22150_SetOutputEnable` | `(HANDLE, int, Bool)` | ✅ |
| `okPLL22150_GetOutputSource` | `(HANDLE, int) -> ok_ClockSource_22150` | ✅ |
| `okPLL22150_GetOutputFrequency` | `(HANDLE, int) -> double` | ✅ |
| `okPLL22150_IsOutputEnabled` | `(HANDLE, int) -> Bool` | ✅ |
| `okFrontPanel_GetPLL22150Configuration` | `(HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_SetPLL22150Configuration` | `(HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_GetEepromPLL22150Configuration` | `(HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_SetEepromPLL22150Configuration` | `(HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode` | ✅ |

### PLL22393

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okPLL22393_Construct` | `() -> okPLL22393_HANDLE` | ✅ |
| `okPLL22393_Destruct` | `(HANDLE)` | ✅ |
| `okPLL22393_SetCrystalLoad` | `(HANDLE, double) -> Bool` | ✅ |
| `okPLL22393_SetReference` | `(HANDLE, double)` | ✅ |
| `okPLL22393_GetReference` | `(HANDLE) -> double` | ✅ |
| `okPLL22393_SetPLLParameters` | `(HANDLE, int, int, int, Bool) -> Bool` | ✅ |
| `okPLL22393_SetPLLLF` | `(HANDLE, int, int) -> Bool` | ✅ |
| `okPLL22393_SetOutputDivider` | `(HANDLE, int, int) -> Bool` | ✅ |
| `okPLL22393_SetOutputSource` | `(HANDLE, int, ok_ClockSource_22393) -> Bool` | ✅ |
| `okPLL22393_SetOutputEnable` | `(HANDLE, int, Bool)` | ✅ |
| `okPLL22393_GetPLLP` | `(HANDLE, int) -> int` | ✅ |
| `okPLL22393_GetPLLQ` | `(HANDLE, int) -> int` | ✅ |
| `okPLL22393_GetPLLFrequency` | `(HANDLE, int) -> double` | ✅ |
| `okPLL22393_GetOutputDivider` | `(HANDLE, int) -> int` | ✅ |
| `okPLL22393_GetOutputSource` | `(HANDLE, int) -> ok_ClockSource_22393` | ✅ |
| `okPLL22393_GetOutputFrequency` | `(HANDLE, int) -> double` | ✅ |
| `okPLL22393_IsOutputEnabled` | `(HANDLE, int) -> Bool` | ✅ |
| `okPLL22393_IsPLLEnabled` | `(HANDLE, int) -> Bool` | ✅ |
| `okFrontPanel_GetPLL22393Configuration` | `(HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_SetPLL22393Configuration` | `(HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_GetEepromPLL22393Configuration` | `(HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_SetEepromPLL22393Configuration` | `(HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_LoadDefaultPLLConfiguration` | `(HANDLE) -> ok_ErrorCode` | ✅ |

---

## 12. Flash Memory

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanel_FlashEraseSector` | `(HANDLE, UINT32 address) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_FlashWrite` | `(HANDLE, UINT32 address, UINT32 length, const UINT8* buf) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_FlashRead` | `(HANDLE, UINT32 address, UINT32 length, UINT8* buf) -> ok_ErrorCode` | ✅ |

---

## 13. Custom Device Registration

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanel_AddCustomDevice` | `(const okTDeviceMatchInfo*, const okTDeviceInfo*) -> ok_ErrorCode` | ✅ |
| `okFrontPanel_RemoveCustomDevice` | `(int productID) -> ok_ErrorCode` | ✅ |

---

## 14. FrontPanelManager (Hot-Plug)

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanelManager_Construct` | `(okFrontPanelManager_HANDLE self, const char* realm) -> okCFrontPanelManager_HANDLE` | ✅ |
| `okFrontPanelManager_ConstructWithCallbacks` | `(HANDLE self, const char* realm, callback onAdded, callback onRemoved) -> okCFrontPanelManager_HANDLE` | ✅ |
| `okFrontPanelManager_Destruct` | `(HANDLE)` | ✅ |
| `okFrontPanelManager_StartMonitoring` | `(HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanelManager_StartMonitoringWithCBInfo` | `(HANDLE, okTCallbackInfo*) -> ok_ErrorCode` | ✅ |
| `okFrontPanelManager_StopMonitoring` | `(HANDLE) -> ok_ErrorCode` | ✅ |
| `okFrontPanelManager_EnterMonitorLoop` | `(HANDLE, const okTCallbackInfo*) -> int` | ✅ |
| `okFrontPanelManager_EnterMonitorLoopWithTimeout` | `(HANDLE, const okTCallbackInfo*, int ms) -> int` | ✅ |
| `okFrontPanelManager_ExitMonitorLoop` | `(HANDLE, int exitCode)` | ✅ |
| `okFrontPanelManager_Open` | `(HANDLE, const char* serial) -> okFrontPanel_HANDLE` | ✅ |
| `okFrontPanelManager_EmulateTestDeviceConnection` | `(const char* serial, Bool connect) -> ok_ErrorCode` | ✅ |

---

## 15. FrontPanelDevices (Enumeration Snapshot)

| C Function | Signature | Include |
|-----------|-----------|---------|
| `okFrontPanelDevices_Construct` | `(const char* realm, okError**) -> okCFrontPanelDevices_HANDLE` | ✅ |
| `okFrontPanelDevices_Destruct` | `(HANDLE)` | ✅ |
| `okFrontPanelDevices_GetCount` | `(HANDLE) -> int` | ✅ |
| `okFrontPanelDevices_GetSerial` | `(HANDLE, int num, char* buf)` | ✅ |
| `okFrontPanelDevices_Open` | `(HANDLE, const char* serial) -> okFrontPanel_HANDLE` | ✅ |

---

## 16. Excluded Functions (USB 2.0 / Lua / Android)

| C Function | Reason |
|-----------|--------|
| `okFrontPanel_WriteI2C` | USB 2.0 only |
| `okFrontPanel_ReadI2C` | USB 2.0 only |
| `okFrontPanel_ConstructFromFD` | Android only |
| `okBuffer_*` (all) | Lua scripting API |
| `okScriptValue_*` (all) | Lua scripting API |
| `okScriptValues_*` (all) | Lua scripting API |
| `okScriptEngine_*` (all) | Lua scripting API |
| `okFrontPanelDLL_LoadLib` | Legacy compat, not needed |
| `okFrontPanelDLL_FreeLib` | Legacy compat, not needed |
| `okFrontPanelDLL_GetVersion` | Legacy compat, not needed |
| `okFrontPanelDLL_GetAPIVersion*` | Legacy compat synonyms |
| `okFrontPanelDLL_CheckAPIVersion` | Legacy compat synonym |

---

## 17. C Structs

| Struct | Size (approx) | Key Fields | Include |
|--------|--------------|-----------|---------|
| `okTDeviceInfo` | ~640 bytes | `serialNumber[11]`, `productName[128]`, `productID`, `deviceInterface`, `usbSpeed`, `deviceMajorVersion`, `deviceMinorVersion`, `wireWidth`, `triggerWidth`, `pipeWidth`, `registerAddressWidth`, `registerDataWidth`, `fpgaVendor`, `family`, `dataport`, `flashSystem`, `flashFPGA`, `supportedBandwidthModesMask` | ✅ |
| `okTDeviceMatchInfo` | ~148 bytes | `productName[128]`, `productBaseID`, `productID`, `usbVID`, `usbPID`, `pciDID` | ✅ |
| `okTDeviceSensor` | ~336 bytes | `id`, `type`, `name[64]`, `description[256]`, `min`, `max`, `step`, `value` | ✅ |
| `okTRegisterEntry` | 8 bytes | `address` (UINT32), `data` (UINT32) | ✅ |
| `okTTriggerEntry` | 8 bytes | `address` (UINT32), `mask` (UINT32) | ✅ |
| `okTFPGAResetProfile` | 4096 bytes | `magic`, `wireInValues[32]`, `registerEntries[256]`, `triggerEntries[32]`, etc. | ✅ |
| `okTFlashLayout` | 20 bytes | `sectorCount`, `sectorSize`, `pageSize`, `minUserSector`, `maxUserSector` | ✅ |
| `okTOperationStatistics` | 16 bytes | `TransferByteCount` (UINT64), `TransactionCount` (UINT64) | ✅ |
| `okTCallbackInfo` | ~24 bytes | `fd`, `callback` (fn ptr), `param` (void ptr) | ✅ |

---

## 18. C Enums & Constants

| Enum/Constants | Values | Include |
|---------------|--------|---------|
| `ok_ErrorCode` | `ok_NoError` (0) through `ok_FrontPanelNotEnabled` (-26) | ✅ |
| `ok_BoardModel` | `ok_brdUnknown` (0) through `ok_brdXEM7460K410T` (64) | ✅ |
| `OK_PRODUCT_*` | `OK_PRODUCT_UNKNOWN` (0) through `OK_PRODUCT_XEM7460K410T` (64) | ✅ |
| `OK_INTERFACE_*` | `UNKNOWN` (0), `USB2` (1), `PCIE` (2), `USB3` (3), `GEN3` (4) | ✅ |
| `OK_USBSPEED_*` | `UNKNOWN` (0) through `SS_GEN2X2` (6) | ✅ |
| `OK_DEVICEFAMILY_*` | `UNKNOWN` (0) through `PCIEGEN1` (4) | ✅ |
| `OK_DATAPORT_*` | `UNKNOWN` (0), `LEGACY` (1), `AXI` (2) | ✅ |
| `okEDeviceSensorType` | `INVALID`, `BOOL`, `INTEGER`, `FLOAT`, `VOLTAGE`, `CURRENT`, `TEMPERATURE`, `FAN_RPM`, `POWER` | ✅ |
| `ok_ClockSource_22150` | `Ref` (0) through `Div2By4` (6) | ✅ |
| `ok_ClockSource_22393` | `Ref` (0), `PLL0_0` (2) through `PLL2_180` (7) | ✅ |
| `ok_DividerSource` | `Ref` (0), `VCO` (1) | ✅ |
| `okEFPGAConfigurationMethod` | `NVRAM` (0), `JTAG` (1) | ✅ |
| `okEFPGAVendor` | `UNKNOWN`, `XILINX`, `INTEL` | ✅ |
| `okEBandwidthMode` | `UNCONFIGURED` (0), modes 1-7 as bitmask values | ✅ |

---

## 19. Handle Types

| Handle Type | Construct | Destruct | Include |
|------------|-----------|----------|---------|
| `okFrontPanel_HANDLE` | `okFrontPanel_Construct` | `okFrontPanel_Destruct` | ✅ |
| `okPLL22150_HANDLE` | `okPLL22150_Construct` | `okPLL22150_Destruct` | ✅ |
| `okPLL22393_HANDLE` | `okPLL22393_Construct` | `okPLL22393_Destruct` | ✅ |
| `okDeviceSensors_HANDLE` | `okDeviceSensors_Construct` | `okDeviceSensors_Destruct` | ✅ |
| `okDeviceSettings_HANDLE` | `okDeviceSettings_Construct` | `okDeviceSettings_Destruct` | ✅ |
| `okDeviceSettingNames_HANDLE` | `okDeviceSettingNames_Construct` | `okDeviceSettingNames_Destruct` | ✅ |
| `okFPGADataPortClassic_HANDLE` | (from `GetFPGADataPortClassic`) | (owned by device) | ✅ |
| `okFPGADataPortAXI_HANDLE` | (from `GetFPGADataPortAXI`) | (owned by device) | ✅ |
| `okFrontPanelAXILite_HANDLE` | (from `GetAXILite`) | (owned by AXI port) | ✅ |
| `okFrontPanelAXIFull_HANDLE` | (from `GetAXIFull`) | (owned by AXI port) | ✅ |
| `okFrontPanelAXIStream_HANDLE` | (from `GetAXIStream`) | (owned by AXI port) | ✅ |
| `okCFrontPanelManager_HANDLE` | `okFrontPanelManager_Construct*` | `okFrontPanelManager_Destruct` | ✅ |
| `okCFrontPanelDevices_HANDLE` | `okFrontPanelDevices_Construct` | `okFrontPanelDevices_Destruct` | ✅ |

---

## Summary

| Category | Included Functions | Excluded Functions |
|----------|-------------------|-------------------|
| Version/Library | 5 | 1 |
| Error Handling | 5 | 0 |
| Device Core | 9 | 1 (Android) |
| FPGA Configuration | 7 | 0 |
| Reset Profile | 4 | 0 |
| Data Port Acquisition | 2 | 0 |
| Classic Data Port | 19 | 0 |
| AXI Data Port | 22 | 0 |
| Device Settings | 14 | 0 |
| Device Sensors | 5 | 0 |
| PLL22150 | 24 | 0 |
| PLL22393 | 23 | 0 |
| Flash | 3 | 0 |
| Custom Device | 2 | 0 |
| Manager (Hot-Plug) | 11 | 0 |
| Devices (Snapshot) | 5 | 0 |
| I2C (excluded) | 0 | 2 |
| Lua (excluded) | 0 | ~20 |
| Legacy DLL (excluded) | 0 | 5 |
| **Total** | **~160** | **~29** |
