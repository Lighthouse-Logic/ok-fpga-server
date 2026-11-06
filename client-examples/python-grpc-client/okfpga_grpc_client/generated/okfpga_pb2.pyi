from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class DeviceInterface(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    DEVICE_INTERFACE_UNKNOWN: _ClassVar[DeviceInterface]
    DEVICE_INTERFACE_USB2: _ClassVar[DeviceInterface]
    DEVICE_INTERFACE_PCIE: _ClassVar[DeviceInterface]
    DEVICE_INTERFACE_USB3: _ClassVar[DeviceInterface]
    DEVICE_INTERFACE_GEN3: _ClassVar[DeviceInterface]

class USBSpeed(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    USB_SPEED_UNKNOWN: _ClassVar[USBSpeed]
    USB_SPEED_FULL: _ClassVar[USBSpeed]
    USB_SPEED_HIGH: _ClassVar[USBSpeed]
    USB_SPEED_SUPER: _ClassVar[USBSpeed]
    USB_SPEED_SS_GEN1: _ClassVar[USBSpeed]
    USB_SPEED_SS_GEN2: _ClassVar[USBSpeed]
    USB_SPEED_SS_GEN2X2: _ClassVar[USBSpeed]

class FPGAVendor(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    FPGA_VENDOR_UNKNOWN: _ClassVar[FPGAVendor]
    FPGA_VENDOR_XILINX: _ClassVar[FPGAVendor]
    FPGA_VENDOR_INTEL: _ClassVar[FPGAVendor]

class DataportType(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    DATAPORT_LEGACY: _ClassVar[DataportType]
    DATAPORT_AXI: _ClassVar[DataportType]

class ResourceState(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    RESOURCE_STATE_UNAVAILABLE: _ClassVar[ResourceState]
    RESOURCE_STATE_INITIALIZING: _ClassVar[ResourceState]
    RESOURCE_STATE_AVAILABLE: _ClassVar[ResourceState]
    RESOURCE_STATE_IN_USE_SHARED: _ClassVar[ResourceState]
    RESOURCE_STATE_IN_USE_LOCKED: _ClassVar[ResourceState]

class SensorType(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    SENSOR_TYPE_INVALID: _ClassVar[SensorType]
    SENSOR_TYPE_BOOL: _ClassVar[SensorType]
    SENSOR_TYPE_INTEGER: _ClassVar[SensorType]
    SENSOR_TYPE_FLOAT: _ClassVar[SensorType]
    SENSOR_TYPE_VOLTAGE: _ClassVar[SensorType]
    SENSOR_TYPE_CURRENT: _ClassVar[SensorType]
    SENSOR_TYPE_TEMPERATURE: _ClassVar[SensorType]
    SENSOR_TYPE_FAN_RPM: _ClassVar[SensorType]
    SENSOR_TYPE_POWER: _ClassVar[SensorType]

class AccessMode(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    ACCESS_MODE_SHARED: _ClassVar[AccessMode]
    ACCESS_MODE_EXCLUSIVE: _ClassVar[AccessMode]
    ACCESS_MODE_READ_ONLY: _ClassVar[AccessMode]

class ResourceAccessMode(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    RESOURCE_ACCESS_MODE_SHARED: _ClassVar[ResourceAccessMode]
    RESOURCE_ACCESS_MODE_EXCLUSIVE: _ClassVar[ResourceAccessMode]

class DeleteReason(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    DELETE_REASON_CLIENT: _ClassVar[DeleteReason]
    DELETE_REASON_REPROGRAM: _ClassVar[DeleteReason]
    DELETE_REASON_CLEAR: _ClassVar[DeleteReason]
    DELETE_REASON_DEVICE_REMOVED: _ClassVar[DeleteReason]
DEVICE_INTERFACE_UNKNOWN: DeviceInterface
DEVICE_INTERFACE_USB2: DeviceInterface
DEVICE_INTERFACE_PCIE: DeviceInterface
DEVICE_INTERFACE_USB3: DeviceInterface
DEVICE_INTERFACE_GEN3: DeviceInterface
USB_SPEED_UNKNOWN: USBSpeed
USB_SPEED_FULL: USBSpeed
USB_SPEED_HIGH: USBSpeed
USB_SPEED_SUPER: USBSpeed
USB_SPEED_SS_GEN1: USBSpeed
USB_SPEED_SS_GEN2: USBSpeed
USB_SPEED_SS_GEN2X2: USBSpeed
FPGA_VENDOR_UNKNOWN: FPGAVendor
FPGA_VENDOR_XILINX: FPGAVendor
FPGA_VENDOR_INTEL: FPGAVendor
DATAPORT_LEGACY: DataportType
DATAPORT_AXI: DataportType
RESOURCE_STATE_UNAVAILABLE: ResourceState
RESOURCE_STATE_INITIALIZING: ResourceState
RESOURCE_STATE_AVAILABLE: ResourceState
RESOURCE_STATE_IN_USE_SHARED: ResourceState
RESOURCE_STATE_IN_USE_LOCKED: ResourceState
SENSOR_TYPE_INVALID: SensorType
SENSOR_TYPE_BOOL: SensorType
SENSOR_TYPE_INTEGER: SensorType
SENSOR_TYPE_FLOAT: SensorType
SENSOR_TYPE_VOLTAGE: SensorType
SENSOR_TYPE_CURRENT: SensorType
SENSOR_TYPE_TEMPERATURE: SensorType
SENSOR_TYPE_FAN_RPM: SensorType
SENSOR_TYPE_POWER: SensorType
ACCESS_MODE_SHARED: AccessMode
ACCESS_MODE_EXCLUSIVE: AccessMode
ACCESS_MODE_READ_ONLY: AccessMode
RESOURCE_ACCESS_MODE_SHARED: ResourceAccessMode
RESOURCE_ACCESS_MODE_EXCLUSIVE: ResourceAccessMode
DELETE_REASON_CLIENT: DeleteReason
DELETE_REASON_REPROGRAM: DeleteReason
DELETE_REASON_CLEAR: DeleteReason
DELETE_REASON_DEVICE_REMOVED: DeleteReason

class ErrorCluster(_message.Message):
    __slots__ = ("status", "code", "source")
    STATUS_FIELD_NUMBER: _ClassVar[int]
    CODE_FIELD_NUMBER: _ClassVar[int]
    SOURCE_FIELD_NUMBER: _ClassVar[int]
    status: bool
    code: int
    source: str
    def __init__(self, status: _Optional[bool] = ..., code: _Optional[int] = ..., source: _Optional[str] = ...) -> None: ...

class DeviceRequest(_message.Message):
    __slots__ = ("serial",)
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    serial: str
    def __init__(self, serial: _Optional[str] = ...) -> None: ...

class StatusResponse(_message.Message):
    __slots__ = ("error",)
    ERROR_FIELD_NUMBER: _ClassVar[int]
    error: ErrorCluster
    def __init__(self, error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class DeviceInfo(_message.Message):
    __slots__ = ("serial", "device_id", "product_name", "product_id", "device_interface", "usb_speed", "device_major_version", "device_minor_version", "wire_width", "trigger_width", "pipe_width", "register_address_width", "register_data_width", "fpga_vendor", "dataport", "is_front_panel_enabled", "bitfile")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    DEVICE_ID_FIELD_NUMBER: _ClassVar[int]
    PRODUCT_NAME_FIELD_NUMBER: _ClassVar[int]
    PRODUCT_ID_FIELD_NUMBER: _ClassVar[int]
    DEVICE_INTERFACE_FIELD_NUMBER: _ClassVar[int]
    USB_SPEED_FIELD_NUMBER: _ClassVar[int]
    DEVICE_MAJOR_VERSION_FIELD_NUMBER: _ClassVar[int]
    DEVICE_MINOR_VERSION_FIELD_NUMBER: _ClassVar[int]
    WIRE_WIDTH_FIELD_NUMBER: _ClassVar[int]
    TRIGGER_WIDTH_FIELD_NUMBER: _ClassVar[int]
    PIPE_WIDTH_FIELD_NUMBER: _ClassVar[int]
    REGISTER_ADDRESS_WIDTH_FIELD_NUMBER: _ClassVar[int]
    REGISTER_DATA_WIDTH_FIELD_NUMBER: _ClassVar[int]
    FPGA_VENDOR_FIELD_NUMBER: _ClassVar[int]
    DATAPORT_FIELD_NUMBER: _ClassVar[int]
    IS_FRONT_PANEL_ENABLED_FIELD_NUMBER: _ClassVar[int]
    BITFILE_FIELD_NUMBER: _ClassVar[int]
    serial: str
    device_id: str
    product_name: str
    product_id: int
    device_interface: DeviceInterface
    usb_speed: USBSpeed
    device_major_version: int
    device_minor_version: int
    wire_width: int
    trigger_width: int
    pipe_width: int
    register_address_width: int
    register_data_width: int
    fpga_vendor: FPGAVendor
    dataport: DataportType
    is_front_panel_enabled: bool
    bitfile: BitfileInfo
    def __init__(self, serial: _Optional[str] = ..., device_id: _Optional[str] = ..., product_name: _Optional[str] = ..., product_id: _Optional[int] = ..., device_interface: _Optional[_Union[DeviceInterface, str]] = ..., usb_speed: _Optional[_Union[USBSpeed, str]] = ..., device_major_version: _Optional[int] = ..., device_minor_version: _Optional[int] = ..., wire_width: _Optional[int] = ..., trigger_width: _Optional[int] = ..., pipe_width: _Optional[int] = ..., register_address_width: _Optional[int] = ..., register_data_width: _Optional[int] = ..., fpga_vendor: _Optional[_Union[FPGAVendor, str]] = ..., dataport: _Optional[_Union[DataportType, str]] = ..., is_front_panel_enabled: _Optional[bool] = ..., bitfile: _Optional[_Union[BitfileInfo, _Mapping]] = ...) -> None: ...

class BitfileInfo(_message.Message):
    __slots__ = ("filename", "description", "programmed_at", "programmed_by", "file_path", "md5_hash")
    FILENAME_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    PROGRAMMED_AT_FIELD_NUMBER: _ClassVar[int]
    PROGRAMMED_BY_FIELD_NUMBER: _ClassVar[int]
    FILE_PATH_FIELD_NUMBER: _ClassVar[int]
    MD5_HASH_FIELD_NUMBER: _ClassVar[int]
    filename: str
    description: str
    programmed_at: str
    programmed_by: str
    file_path: str
    md5_hash: str
    def __init__(self, filename: _Optional[str] = ..., description: _Optional[str] = ..., programmed_at: _Optional[str] = ..., programmed_by: _Optional[str] = ..., file_path: _Optional[str] = ..., md5_hash: _Optional[str] = ...) -> None: ...

class DeviceSensor(_message.Message):
    __slots__ = ("id", "sensor_type", "name", "description", "min", "max", "step", "value")
    ID_FIELD_NUMBER: _ClassVar[int]
    SENSOR_TYPE_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    MIN_FIELD_NUMBER: _ClassVar[int]
    MAX_FIELD_NUMBER: _ClassVar[int]
    STEP_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    id: int
    sensor_type: SensorType
    name: str
    description: str
    min: float
    max: float
    step: float
    value: float
    def __init__(self, id: _Optional[int] = ..., sensor_type: _Optional[_Union[SensorType, str]] = ..., name: _Optional[str] = ..., description: _Optional[str] = ..., min: _Optional[float] = ..., max: _Optional[float] = ..., step: _Optional[float] = ..., value: _Optional[float] = ...) -> None: ...

class DeviceSetting(_message.Message):
    __slots__ = ("key", "string_value", "int_value")
    KEY_FIELD_NUMBER: _ClassVar[int]
    STRING_VALUE_FIELD_NUMBER: _ClassVar[int]
    INT_VALUE_FIELD_NUMBER: _ClassVar[int]
    key: str
    string_value: str
    int_value: int
    def __init__(self, key: _Optional[str] = ..., string_value: _Optional[str] = ..., int_value: _Optional[int] = ...) -> None: ...

class SyzygyPort(_message.Message):
    __slots__ = ("index", "port_status_mv", "is_populated", "manufacturer_name", "product_name", "product_model", "product_version", "serial_number")
    INDEX_FIELD_NUMBER: _ClassVar[int]
    PORT_STATUS_MV_FIELD_NUMBER: _ClassVar[int]
    IS_POPULATED_FIELD_NUMBER: _ClassVar[int]
    MANUFACTURER_NAME_FIELD_NUMBER: _ClassVar[int]
    PRODUCT_NAME_FIELD_NUMBER: _ClassVar[int]
    PRODUCT_MODEL_FIELD_NUMBER: _ClassVar[int]
    PRODUCT_VERSION_FIELD_NUMBER: _ClassVar[int]
    SERIAL_NUMBER_FIELD_NUMBER: _ClassVar[int]
    index: int
    port_status_mv: int
    is_populated: bool
    manufacturer_name: str
    product_name: str
    product_model: str
    product_version: str
    serial_number: str
    def __init__(self, index: _Optional[int] = ..., port_status_mv: _Optional[int] = ..., is_populated: _Optional[bool] = ..., manufacturer_name: _Optional[str] = ..., product_name: _Optional[str] = ..., product_model: _Optional[str] = ..., product_version: _Optional[str] = ..., serial_number: _Optional[str] = ...) -> None: ...

class Resource(_message.Message):
    __slots__ = ("id", "name", "description", "device_serial", "state", "private", "locked_by", "holders", "access_mode", "interfaces", "endpoints", "address_ranges", "metadata")
    class MetadataEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    DEVICE_SERIAL_FIELD_NUMBER: _ClassVar[int]
    STATE_FIELD_NUMBER: _ClassVar[int]
    PRIVATE_FIELD_NUMBER: _ClassVar[int]
    LOCKED_BY_FIELD_NUMBER: _ClassVar[int]
    HOLDERS_FIELD_NUMBER: _ClassVar[int]
    ACCESS_MODE_FIELD_NUMBER: _ClassVar[int]
    INTERFACES_FIELD_NUMBER: _ClassVar[int]
    ENDPOINTS_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_RANGES_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    id: str
    name: str
    description: str
    device_serial: str
    state: ResourceState
    private: bool
    locked_by: str
    holders: _containers.RepeatedCompositeFieldContainer[Holder]
    access_mode: ResourceAccessMode
    interfaces: _containers.RepeatedScalarFieldContainer[str]
    endpoints: EndpointDeclaration
    address_ranges: _containers.RepeatedCompositeFieldContainer[AddressRange]
    metadata: _containers.ScalarMap[str, str]
    def __init__(self, id: _Optional[str] = ..., name: _Optional[str] = ..., description: _Optional[str] = ..., device_serial: _Optional[str] = ..., state: _Optional[_Union[ResourceState, str]] = ..., private: _Optional[bool] = ..., locked_by: _Optional[str] = ..., holders: _Optional[_Iterable[_Union[Holder, _Mapping]]] = ..., access_mode: _Optional[_Union[ResourceAccessMode, str]] = ..., interfaces: _Optional[_Iterable[str]] = ..., endpoints: _Optional[_Union[EndpointDeclaration, _Mapping]] = ..., address_ranges: _Optional[_Iterable[_Union[AddressRange, _Mapping]]] = ..., metadata: _Optional[_Mapping[str, str]] = ...) -> None: ...

class Holder(_message.Message):
    __slots__ = ("user_id", "mode")
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    mode: AccessMode
    def __init__(self, user_id: _Optional[str] = ..., mode: _Optional[_Union[AccessMode, str]] = ...) -> None: ...

class EndpointDeclaration(_message.Message):
    __slots__ = ("default_access", "entries")
    DEFAULT_ACCESS_FIELD_NUMBER: _ClassVar[int]
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    default_access: ResourceAccessMode
    entries: _containers.RepeatedCompositeFieldContainer[EndpointEntry]
    def __init__(self, default_access: _Optional[_Union[ResourceAccessMode, str]] = ..., entries: _Optional[_Iterable[_Union[EndpointEntry, _Mapping]]] = ...) -> None: ...

class EndpointEntry(_message.Message):
    __slots__ = ("address", "type", "access", "private", "description")
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    TYPE_FIELD_NUMBER: _ClassVar[int]
    ACCESS_FIELD_NUMBER: _ClassVar[int]
    PRIVATE_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    address: int
    type: str
    access: ResourceAccessMode
    private: bool
    description: str
    def __init__(self, address: _Optional[int] = ..., type: _Optional[str] = ..., access: _Optional[_Union[ResourceAccessMode, str]] = ..., private: _Optional[bool] = ..., description: _Optional[str] = ...) -> None: ...

class AddressRange(_message.Message):
    __slots__ = ("name", "start", "end", "access", "private", "description")
    NAME_FIELD_NUMBER: _ClassVar[int]
    START_FIELD_NUMBER: _ClassVar[int]
    END_FIELD_NUMBER: _ClassVar[int]
    ACCESS_FIELD_NUMBER: _ClassVar[int]
    PRIVATE_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    name: str
    start: int
    end: int
    access: ResourceAccessMode
    private: bool
    description: str
    def __init__(self, name: _Optional[str] = ..., start: _Optional[int] = ..., end: _Optional[int] = ..., access: _Optional[_Union[ResourceAccessMode, str]] = ..., private: _Optional[bool] = ..., description: _Optional[str] = ...) -> None: ...

class Holding(_message.Message):
    __slots__ = ("resource_id", "mode", "locked")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    LOCKED_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    mode: AccessMode
    locked: bool
    def __init__(self, resource_id: _Optional[str] = ..., mode: _Optional[_Union[AccessMode, str]] = ..., locked: _Optional[bool] = ...) -> None: ...

class UserSession(_message.Message):
    __slots__ = ("user_id", "connected_at", "held_resource_ids", "holdings")
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    CONNECTED_AT_FIELD_NUMBER: _ClassVar[int]
    HELD_RESOURCE_IDS_FIELD_NUMBER: _ClassVar[int]
    HOLDINGS_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    connected_at: str
    held_resource_ids: _containers.RepeatedScalarFieldContainer[str]
    holdings: _containers.RepeatedCompositeFieldContainer[Holding]
    def __init__(self, user_id: _Optional[str] = ..., connected_at: _Optional[str] = ..., held_resource_ids: _Optional[_Iterable[str]] = ..., holdings: _Optional[_Iterable[_Union[Holding, _Mapping]]] = ...) -> None: ...

class ResourceConflictDetail(_message.Message):
    __slots__ = ("code", "resource_id", "resource_name", "item_type", "item", "items")
    CODE_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_NAME_FIELD_NUMBER: _ClassVar[int]
    ITEM_TYPE_FIELD_NUMBER: _ClassVar[int]
    ITEM_FIELD_NUMBER: _ClassVar[int]
    ITEMS_FIELD_NUMBER: _ClassVar[int]
    code: str
    resource_id: str
    resource_name: str
    item_type: str
    item: str
    items: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, code: _Optional[str] = ..., resource_id: _Optional[str] = ..., resource_name: _Optional[str] = ..., item_type: _Optional[str] = ..., item: _Optional[str] = ..., items: _Optional[_Iterable[str]] = ...) -> None: ...

class ServerStatus(_message.Message):
    __slots__ = ("version", "frontpanel_version", "platform", "uptime", "uptime_seconds", "connected_devices", "active_resources")
    VERSION_FIELD_NUMBER: _ClassVar[int]
    FRONTPANEL_VERSION_FIELD_NUMBER: _ClassVar[int]
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    UPTIME_FIELD_NUMBER: _ClassVar[int]
    UPTIME_SECONDS_FIELD_NUMBER: _ClassVar[int]
    CONNECTED_DEVICES_FIELD_NUMBER: _ClassVar[int]
    ACTIVE_RESOURCES_FIELD_NUMBER: _ClassVar[int]
    version: str
    frontpanel_version: str
    platform: str
    uptime: str
    uptime_seconds: int
    connected_devices: int
    active_resources: int
    def __init__(self, version: _Optional[str] = ..., frontpanel_version: _Optional[str] = ..., platform: _Optional[str] = ..., uptime: _Optional[str] = ..., uptime_seconds: _Optional[int] = ..., connected_devices: _Optional[int] = ..., active_resources: _Optional[int] = ...) -> None: ...

class DeviceEvent(_message.Message):
    __slots__ = ("timestamp", "device_added", "device_removed", "bitfile_programmed", "resource_state_changed", "resource_created", "resource_deleted", "resource_locked", "resource_unlocked", "user_connected", "user_disconnected", "server_shutdown")
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    DEVICE_ADDED_FIELD_NUMBER: _ClassVar[int]
    DEVICE_REMOVED_FIELD_NUMBER: _ClassVar[int]
    BITFILE_PROGRAMMED_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_STATE_CHANGED_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_CREATED_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_DELETED_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_LOCKED_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_UNLOCKED_FIELD_NUMBER: _ClassVar[int]
    USER_CONNECTED_FIELD_NUMBER: _ClassVar[int]
    USER_DISCONNECTED_FIELD_NUMBER: _ClassVar[int]
    SERVER_SHUTDOWN_FIELD_NUMBER: _ClassVar[int]
    timestamp: str
    device_added: DeviceAdded
    device_removed: DeviceRemoved
    bitfile_programmed: BitfileProgrammed
    resource_state_changed: ResourceStateChanged
    resource_created: ResourceCreatedEvent
    resource_deleted: ResourceDeletedEvent
    resource_locked: ResourceLockedEvent
    resource_unlocked: ResourceUnlockedEvent
    user_connected: UserConnectedEvent
    user_disconnected: UserDisconnectedEvent
    server_shutdown: ServerShutdownEvent
    def __init__(self, timestamp: _Optional[str] = ..., device_added: _Optional[_Union[DeviceAdded, _Mapping]] = ..., device_removed: _Optional[_Union[DeviceRemoved, _Mapping]] = ..., bitfile_programmed: _Optional[_Union[BitfileProgrammed, _Mapping]] = ..., resource_state_changed: _Optional[_Union[ResourceStateChanged, _Mapping]] = ..., resource_created: _Optional[_Union[ResourceCreatedEvent, _Mapping]] = ..., resource_deleted: _Optional[_Union[ResourceDeletedEvent, _Mapping]] = ..., resource_locked: _Optional[_Union[ResourceLockedEvent, _Mapping]] = ..., resource_unlocked: _Optional[_Union[ResourceUnlockedEvent, _Mapping]] = ..., user_connected: _Optional[_Union[UserConnectedEvent, _Mapping]] = ..., user_disconnected: _Optional[_Union[UserDisconnectedEvent, _Mapping]] = ..., server_shutdown: _Optional[_Union[ServerShutdownEvent, _Mapping]] = ...) -> None: ...

class ResourceCreatedEvent(_message.Message):
    __slots__ = ("resource_id", "device_serial", "name")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    DEVICE_SERIAL_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    device_serial: str
    name: str
    def __init__(self, resource_id: _Optional[str] = ..., device_serial: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class ResourceDeletedEvent(_message.Message):
    __slots__ = ("resource_id", "reason")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    reason: DeleteReason
    def __init__(self, resource_id: _Optional[str] = ..., reason: _Optional[_Union[DeleteReason, str]] = ...) -> None: ...

class ResourceLockedEvent(_message.Message):
    __slots__ = ("resource_id", "user_id")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    user_id: str
    def __init__(self, resource_id: _Optional[str] = ..., user_id: _Optional[str] = ...) -> None: ...

class ResourceUnlockedEvent(_message.Message):
    __slots__ = ("resource_id", "user_id")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    user_id: str
    def __init__(self, resource_id: _Optional[str] = ..., user_id: _Optional[str] = ...) -> None: ...

class UserConnectedEvent(_message.Message):
    __slots__ = ("user_id",)
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    def __init__(self, user_id: _Optional[str] = ...) -> None: ...

class UserDisconnectedEvent(_message.Message):
    __slots__ = ("user_id", "reason")
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    reason: str
    def __init__(self, user_id: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class ServerShutdownEvent(_message.Message):
    __slots__ = ("reason",)
    REASON_FIELD_NUMBER: _ClassVar[int]
    reason: str
    def __init__(self, reason: _Optional[str] = ...) -> None: ...

class DeviceAdded(_message.Message):
    __slots__ = ("serial", "product_name", "device_interface")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    PRODUCT_NAME_FIELD_NUMBER: _ClassVar[int]
    DEVICE_INTERFACE_FIELD_NUMBER: _ClassVar[int]
    serial: str
    product_name: str
    device_interface: DeviceInterface
    def __init__(self, serial: _Optional[str] = ..., product_name: _Optional[str] = ..., device_interface: _Optional[_Union[DeviceInterface, str]] = ...) -> None: ...

class DeviceRemoved(_message.Message):
    __slots__ = ("serial",)
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    serial: str
    def __init__(self, serial: _Optional[str] = ...) -> None: ...

class BitfileProgrammed(_message.Message):
    __slots__ = ("serial", "filename", "programmed_by")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    FILENAME_FIELD_NUMBER: _ClassVar[int]
    PROGRAMMED_BY_FIELD_NUMBER: _ClassVar[int]
    serial: str
    filename: str
    programmed_by: str
    def __init__(self, serial: _Optional[str] = ..., filename: _Optional[str] = ..., programmed_by: _Optional[str] = ...) -> None: ...

class ResourceStateChanged(_message.Message):
    __slots__ = ("resource_id", "previous_state", "new_state", "device_serial")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    PREVIOUS_STATE_FIELD_NUMBER: _ClassVar[int]
    NEW_STATE_FIELD_NUMBER: _ClassVar[int]
    DEVICE_SERIAL_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    previous_state: ResourceState
    new_state: ResourceState
    device_serial: str
    def __init__(self, resource_id: _Optional[str] = ..., previous_state: _Optional[_Union[ResourceState, str]] = ..., new_state: _Optional[_Union[ResourceState, str]] = ..., device_serial: _Optional[str] = ...) -> None: ...

class ListDevicesRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ListDevicesResponse(_message.Message):
    __slots__ = ("devices", "error")
    DEVICES_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    devices: _containers.RepeatedCompositeFieldContainer[DeviceInfo]
    error: ErrorCluster
    def __init__(self, devices: _Optional[_Iterable[_Union[DeviceInfo, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class DeviceInfoResponse(_message.Message):
    __slots__ = ("device", "error")
    DEVICE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    device: DeviceInfo
    error: ErrorCluster
    def __init__(self, device: _Optional[_Union[DeviceInfo, _Mapping]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class DeviceSensorsResponse(_message.Message):
    __slots__ = ("sensors", "error")
    SENSORS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    sensors: _containers.RepeatedCompositeFieldContainer[DeviceSensor]
    error: ErrorCluster
    def __init__(self, sensors: _Optional[_Iterable[_Union[DeviceSensor, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class DeviceSettingsResponse(_message.Message):
    __slots__ = ("settings", "error")
    SETTINGS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    settings: _containers.RepeatedCompositeFieldContainer[DeviceSetting]
    error: ErrorCluster
    def __init__(self, settings: _Optional[_Iterable[_Union[DeviceSetting, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class GetDeviceSettingRequest(_message.Message):
    __slots__ = ("serial", "key")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    KEY_FIELD_NUMBER: _ClassVar[int]
    serial: str
    key: str
    def __init__(self, serial: _Optional[str] = ..., key: _Optional[str] = ...) -> None: ...

class DeviceSettingResponse(_message.Message):
    __slots__ = ("setting", "error")
    SETTING_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    setting: DeviceSetting
    error: ErrorCluster
    def __init__(self, setting: _Optional[_Union[DeviceSetting, _Mapping]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class SetDeviceSettingRequest(_message.Message):
    __slots__ = ("serial", "setting")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    SETTING_FIELD_NUMBER: _ClassVar[int]
    serial: str
    setting: DeviceSetting
    def __init__(self, serial: _Optional[str] = ..., setting: _Optional[_Union[DeviceSetting, _Mapping]] = ...) -> None: ...

class SyzygyPortsResponse(_message.Message):
    __slots__ = ("ports", "error")
    PORTS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    ports: _containers.RepeatedCompositeFieldContainer[SyzygyPort]
    error: ErrorCluster
    def __init__(self, ports: _Optional[_Iterable[_Union[SyzygyPort, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class ConfigureFPGARequest(_message.Message):
    __slots__ = ("serial", "bitfile_path", "user_id")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    BITFILE_PATH_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    serial: str
    bitfile_path: str
    user_id: str
    def __init__(self, serial: _Optional[str] = ..., bitfile_path: _Optional[str] = ..., user_id: _Optional[str] = ...) -> None: ...

class ConfigureFPGAChunk(_message.Message):
    __slots__ = ("serial", "data", "is_last", "user_id", "filename")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    IS_LAST_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    FILENAME_FIELD_NUMBER: _ClassVar[int]
    serial: str
    data: bytes
    is_last: bool
    user_id: str
    filename: str
    def __init__(self, serial: _Optional[str] = ..., data: _Optional[bytes] = ..., is_last: _Optional[bool] = ..., user_id: _Optional[str] = ..., filename: _Optional[str] = ...) -> None: ...

class ConfigureFPGAResponse(_message.Message):
    __slots__ = ("bitfile", "error")
    BITFILE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    bitfile: BitfileInfo
    error: ErrorCluster
    def __init__(self, bitfile: _Optional[_Union[BitfileInfo, _Mapping]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class BitfileInfoResponse(_message.Message):
    __slots__ = ("bitfile", "error")
    BITFILE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    bitfile: BitfileInfo
    error: ErrorCluster
    def __init__(self, bitfile: _Optional[_Union[BitfileInfo, _Mapping]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class DataportTypeResponse(_message.Message):
    __slots__ = ("dataport", "error")
    DATAPORT_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    dataport: DataportType
    error: ErrorCluster
    def __init__(self, dataport: _Optional[_Union[DataportType, str]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class SetDeviceIDRequest(_message.Message):
    __slots__ = ("serial", "device_id")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    DEVICE_ID_FIELD_NUMBER: _ClassVar[int]
    serial: str
    device_id: str
    def __init__(self, serial: _Optional[str] = ..., device_id: _Optional[str] = ...) -> None: ...

class SetTimeoutRequest(_message.Message):
    __slots__ = ("serial", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class GetPLLRequest(_message.Message):
    __slots__ = ("serial", "pll_index")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    PLL_INDEX_FIELD_NUMBER: _ClassVar[int]
    serial: str
    pll_index: int
    def __init__(self, serial: _Optional[str] = ..., pll_index: _Optional[int] = ...) -> None: ...

class PLLConfigurationResponse(_message.Message):
    __slots__ = ("reference_frequency", "outputs", "error")
    REFERENCE_FREQUENCY_FIELD_NUMBER: _ClassVar[int]
    OUTPUTS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    reference_frequency: float
    outputs: _containers.RepeatedCompositeFieldContainer[PLLOutput]
    error: ErrorCluster
    def __init__(self, reference_frequency: _Optional[float] = ..., outputs: _Optional[_Iterable[_Union[PLLOutput, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class PLLOutput(_message.Message):
    __slots__ = ("index", "frequency", "enabled")
    INDEX_FIELD_NUMBER: _ClassVar[int]
    FREQUENCY_FIELD_NUMBER: _ClassVar[int]
    ENABLED_FIELD_NUMBER: _ClassVar[int]
    index: int
    frequency: float
    enabled: bool
    def __init__(self, index: _Optional[int] = ..., frequency: _Optional[float] = ..., enabled: _Optional[bool] = ...) -> None: ...

class SetPLLRequest(_message.Message):
    __slots__ = ("serial", "pll_index", "reference_frequency", "outputs")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    PLL_INDEX_FIELD_NUMBER: _ClassVar[int]
    REFERENCE_FREQUENCY_FIELD_NUMBER: _ClassVar[int]
    OUTPUTS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    pll_index: int
    reference_frequency: float
    outputs: _containers.RepeatedCompositeFieldContainer[PLLOutput]
    def __init__(self, serial: _Optional[str] = ..., pll_index: _Optional[int] = ..., reference_frequency: _Optional[float] = ..., outputs: _Optional[_Iterable[_Union[PLLOutput, _Mapping]]] = ...) -> None: ...

class FlashReadRequest(_message.Message):
    __slots__ = ("serial", "address", "length")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    LENGTH_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    length: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., length: _Optional[int] = ...) -> None: ...

class FlashDataResponse(_message.Message):
    __slots__ = ("data", "error")
    DATA_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    error: ErrorCluster
    def __init__(self, data: _Optional[bytes] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class FlashWriteRequest(_message.Message):
    __slots__ = ("serial", "address", "data")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    data: bytes
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., data: _Optional[bytes] = ...) -> None: ...

class FlashEraseSectorRequest(_message.Message):
    __slots__ = ("serial", "address")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ...) -> None: ...

class WatchDeviceEventsRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ServerStatusRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class SetWireInValueRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "value", "mask", "auto_update")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    MASK_FIELD_NUMBER: _ClassVar[int]
    AUTO_UPDATE_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    value: int
    mask: int
    auto_update: bool
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., value: _Optional[int] = ..., mask: _Optional[int] = ..., auto_update: _Optional[bool] = ...) -> None: ...

class GetWireValueRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "update")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    UPDATE_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    update: bool
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., update: _Optional[bool] = ...) -> None: ...

class WireValueResponse(_message.Message):
    __slots__ = ("value", "error")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    value: int
    error: ErrorCluster
    def __init__(self, value: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class ActivateTriggerInRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "bit")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    BIT_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    bit: int
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., bit: _Optional[int] = ...) -> None: ...

class IsTriggeredRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "mask", "update")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    MASK_FIELD_NUMBER: _ClassVar[int]
    UPDATE_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    mask: int
    update: bool
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., mask: _Optional[int] = ..., update: _Optional[bool] = ...) -> None: ...

class IsTriggeredResponse(_message.Message):
    __slots__ = ("triggered", "error")
    TRIGGERED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    triggered: bool
    error: ErrorCluster
    def __init__(self, triggered: _Optional[bool] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class TriggerOutVectorRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "update")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    UPDATE_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    update: bool
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., update: _Optional[bool] = ...) -> None: ...

class TriggerOutVectorResponse(_message.Message):
    __slots__ = ("vector", "error")
    VECTOR_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    vector: int
    error: ErrorCluster
    def __init__(self, vector: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class WriteToPipeInRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "data")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    data: bytes
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., data: _Optional[bytes] = ...) -> None: ...

class ReadFromPipeOutRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "length")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    LENGTH_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    length: int
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., length: _Optional[int] = ...) -> None: ...

class PipeTransferResponse(_message.Message):
    __slots__ = ("bytes_transferred", "error")
    BYTES_TRANSFERRED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    bytes_transferred: int
    error: ErrorCluster
    def __init__(self, bytes_transferred: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class PipeDataResponse(_message.Message):
    __slots__ = ("data", "bytes_transferred", "error")
    DATA_FIELD_NUMBER: _ClassVar[int]
    BYTES_TRANSFERRED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    bytes_transferred: int
    error: ErrorCluster
    def __init__(self, data: _Optional[bytes] = ..., bytes_transferred: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class WriteToBlockPipeInRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "block_size", "data")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    BLOCK_SIZE_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    block_size: int
    data: bytes
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., block_size: _Optional[int] = ..., data: _Optional[bytes] = ...) -> None: ...

class ReadFromBlockPipeOutRequest(_message.Message):
    __slots__ = ("serial", "endpoint", "block_size", "length")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    BLOCK_SIZE_FIELD_NUMBER: _ClassVar[int]
    LENGTH_FIELD_NUMBER: _ClassVar[int]
    serial: str
    endpoint: int
    block_size: int
    length: int
    def __init__(self, serial: _Optional[str] = ..., endpoint: _Optional[int] = ..., block_size: _Optional[int] = ..., length: _Optional[int] = ...) -> None: ...

class SetBTPipePollingIntervalRequest(_message.Message):
    __slots__ = ("serial", "interval")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    INTERVAL_FIELD_NUMBER: _ClassVar[int]
    serial: str
    interval: int
    def __init__(self, serial: _Optional[str] = ..., interval: _Optional[int] = ...) -> None: ...

class RegisterEntry(_message.Message):
    __slots__ = ("address", "data")
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    address: int
    data: int
    def __init__(self, address: _Optional[int] = ..., data: _Optional[int] = ...) -> None: ...

class ReadRegisterRequest(_message.Message):
    __slots__ = ("serial", "address")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ...) -> None: ...

class RegisterValueResponse(_message.Message):
    __slots__ = ("value", "error")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    value: int
    error: ErrorCluster
    def __init__(self, value: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class WriteRegisterRequest(_message.Message):
    __slots__ = ("serial", "address", "data")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    data: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., data: _Optional[int] = ...) -> None: ...

class ReadRegistersRequest(_message.Message):
    __slots__ = ("serial", "addresses")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESSES_FIELD_NUMBER: _ClassVar[int]
    serial: str
    addresses: _containers.RepeatedScalarFieldContainer[int]
    def __init__(self, serial: _Optional[str] = ..., addresses: _Optional[_Iterable[int]] = ...) -> None: ...

class RegisterEntriesResponse(_message.Message):
    __slots__ = ("entries", "error")
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.RepeatedCompositeFieldContainer[RegisterEntry]
    error: ErrorCluster
    def __init__(self, entries: _Optional[_Iterable[_Union[RegisterEntry, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class WriteRegistersRequest(_message.Message):
    __slots__ = ("serial", "entries")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    serial: str
    entries: _containers.RepeatedCompositeFieldContainer[RegisterEntry]
    def __init__(self, serial: _Optional[str] = ..., entries: _Optional[_Iterable[_Union[RegisterEntry, _Mapping]]] = ...) -> None: ...

class AXILiteReadRequest(_message.Message):
    __slots__ = ("serial", "address", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXILiteReadResponse(_message.Message):
    __slots__ = ("value", "error")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    value: int
    error: ErrorCluster
    def __init__(self, value: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class AXILiteWriteRequest(_message.Message):
    __slots__ = ("serial", "address", "value", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    value: int
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., value: _Optional[int] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXILiteReadBulkRequest(_message.Message):
    __slots__ = ("serial", "address", "count", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    COUNT_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    count: int
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., count: _Optional[int] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXILiteReadBulkResponse(_message.Message):
    __slots__ = ("values", "bytes_transferred", "error")
    VALUES_FIELD_NUMBER: _ClassVar[int]
    BYTES_TRANSFERRED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    values: _containers.RepeatedScalarFieldContainer[int]
    bytes_transferred: int
    error: ErrorCluster
    def __init__(self, values: _Optional[_Iterable[int]] = ..., bytes_transferred: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class AXILiteWriteBulkRequest(_message.Message):
    __slots__ = ("serial", "address", "values", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    VALUES_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    values: _containers.RepeatedScalarFieldContainer[int]
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., values: _Optional[_Iterable[int]] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXIOperationStats(_message.Message):
    __slots__ = ("transfer_byte_count", "transaction_count", "error")
    TRANSFER_BYTE_COUNT_FIELD_NUMBER: _ClassVar[int]
    TRANSACTION_COUNT_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    transfer_byte_count: int
    transaction_count: int
    error: ErrorCluster
    def __init__(self, transfer_byte_count: _Optional[int] = ..., transaction_count: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class DatapathWidthResponse(_message.Message):
    __slots__ = ("width_bytes", "error")
    WIDTH_BYTES_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    width_bytes: int
    error: ErrorCluster
    def __init__(self, width_bytes: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class AXIFullReadRequest(_message.Message):
    __slots__ = ("serial", "address", "length", "burst", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    LENGTH_FIELD_NUMBER: _ClassVar[int]
    BURST_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    length: int
    burst: int
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., length: _Optional[int] = ..., burst: _Optional[int] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXIFullReadResponse(_message.Message):
    __slots__ = ("data", "bytes_transferred", "error")
    DATA_FIELD_NUMBER: _ClassVar[int]
    BYTES_TRANSFERRED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    bytes_transferred: int
    error: ErrorCluster
    def __init__(self, data: _Optional[bytes] = ..., bytes_transferred: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class AXIFullWriteRequest(_message.Message):
    __slots__ = ("serial", "address", "data", "burst", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    BURST_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    address: int
    data: bytes
    burst: int
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., address: _Optional[int] = ..., data: _Optional[bytes] = ..., burst: _Optional[int] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXIStreamDatapathWidthsResponse(_message.Message):
    __slots__ = ("write_width_bytes", "read_width_bytes", "error")
    WRITE_WIDTH_BYTES_FIELD_NUMBER: _ClassVar[int]
    READ_WIDTH_BYTES_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    write_width_bytes: int
    read_width_bytes: int
    error: ErrorCluster
    def __init__(self, write_width_bytes: _Optional[int] = ..., read_width_bytes: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class AXIStreamReadRequest(_message.Message):
    __slots__ = ("serial", "length", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    LENGTH_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    length: int
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., length: _Optional[int] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXIStreamReadResponse(_message.Message):
    __slots__ = ("data", "bytes_transferred", "error")
    DATA_FIELD_NUMBER: _ClassVar[int]
    BYTES_TRANSFERRED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    bytes_transferred: int
    error: ErrorCluster
    def __init__(self, data: _Optional[bytes] = ..., bytes_transferred: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class AXIStreamWriteRequest(_message.Message):
    __slots__ = ("serial", "data", "timeout_ms")
    SERIAL_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    serial: str
    data: bytes
    timeout_ms: int
    def __init__(self, serial: _Optional[str] = ..., data: _Optional[bytes] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class AXIStreamWriteResponse(_message.Message):
    __slots__ = ("bytes_transferred", "error")
    BYTES_TRANSFERRED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    bytes_transferred: int
    error: ErrorCluster
    def __init__(self, bytes_transferred: _Optional[int] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class ListResourcesRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ListResourcesResponse(_message.Message):
    __slots__ = ("resources", "error")
    RESOURCES_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    resources: _containers.RepeatedCompositeFieldContainer[Resource]
    error: ErrorCluster
    def __init__(self, resources: _Optional[_Iterable[_Union[Resource, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class ResourceRequest(_message.Message):
    __slots__ = ("resource_id",)
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    def __init__(self, resource_id: _Optional[str] = ...) -> None: ...

class ResourceResponse(_message.Message):
    __slots__ = ("resource", "error", "warnings")
    RESOURCE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    WARNINGS_FIELD_NUMBER: _ClassVar[int]
    resource: Resource
    error: ErrorCluster
    warnings: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, resource: _Optional[_Union[Resource, _Mapping]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ..., warnings: _Optional[_Iterable[str]] = ...) -> None: ...

class AcquireResourceRequest(_message.Message):
    __slots__ = ("resource_id", "user_id", "mode")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    user_id: str
    mode: AccessMode
    def __init__(self, resource_id: _Optional[str] = ..., user_id: _Optional[str] = ..., mode: _Optional[_Union[AccessMode, str]] = ...) -> None: ...

class ReleaseResourceRequest(_message.Message):
    __slots__ = ("resource_id", "user_id")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    user_id: str
    def __init__(self, resource_id: _Optional[str] = ..., user_id: _Optional[str] = ...) -> None: ...

class CreateResourceRequest(_message.Message):
    __slots__ = ("name", "description", "device_serial", "access_mode", "interfaces", "endpoints", "address_ranges", "metadata", "private")
    class MetadataEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    NAME_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    DEVICE_SERIAL_FIELD_NUMBER: _ClassVar[int]
    ACCESS_MODE_FIELD_NUMBER: _ClassVar[int]
    INTERFACES_FIELD_NUMBER: _ClassVar[int]
    ENDPOINTS_FIELD_NUMBER: _ClassVar[int]
    ADDRESS_RANGES_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    PRIVATE_FIELD_NUMBER: _ClassVar[int]
    name: str
    description: str
    device_serial: str
    access_mode: ResourceAccessMode
    interfaces: _containers.RepeatedScalarFieldContainer[str]
    endpoints: EndpointDeclaration
    address_ranges: _containers.RepeatedCompositeFieldContainer[AddressRange]
    metadata: _containers.ScalarMap[str, str]
    private: bool
    def __init__(self, name: _Optional[str] = ..., description: _Optional[str] = ..., device_serial: _Optional[str] = ..., access_mode: _Optional[_Union[ResourceAccessMode, str]] = ..., interfaces: _Optional[_Iterable[str]] = ..., endpoints: _Optional[_Union[EndpointDeclaration, _Mapping]] = ..., address_ranges: _Optional[_Iterable[_Union[AddressRange, _Mapping]]] = ..., metadata: _Optional[_Mapping[str, str]] = ..., private: _Optional[bool] = ...) -> None: ...

class ListUsersRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ListUsersResponse(_message.Message):
    __slots__ = ("users", "error")
    USERS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    users: _containers.RepeatedCompositeFieldContainer[UserSession]
    error: ErrorCluster
    def __init__(self, users: _Optional[_Iterable[_Union[UserSession, _Mapping]]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class UserRequest(_message.Message):
    __slots__ = ("user_id",)
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    def __init__(self, user_id: _Optional[str] = ...) -> None: ...

class UserResponse(_message.Message):
    __slots__ = ("user", "error")
    USER_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    user: UserSession
    error: ErrorCluster
    def __init__(self, user: _Optional[_Union[UserSession, _Mapping]] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class PingRequest(_message.Message):
    __slots__ = ("user_id",)
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    def __init__(self, user_id: _Optional[str] = ...) -> None: ...

class PingResponse(_message.Message):
    __slots__ = ("server_time", "error")
    SERVER_TIME_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    server_time: str
    error: ErrorCluster
    def __init__(self, server_time: _Optional[str] = ..., error: _Optional[_Union[ErrorCluster, _Mapping]] = ...) -> None: ...

class DisconnectRequest(_message.Message):
    __slots__ = ("user_id",)
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    def __init__(self, user_id: _Optional[str] = ...) -> None: ...

class DisconnectResponse(_message.Message):
    __slots__ = ("user_id", "released_resources", "message")
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    RELEASED_RESOURCES_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    released_resources: int
    message: str
    def __init__(self, user_id: _Optional[str] = ..., released_resources: _Optional[int] = ..., message: _Optional[str] = ...) -> None: ...

class LockResourceRequest(_message.Message):
    __slots__ = ("resource_id", "user_id")
    RESOURCE_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    resource_id: str
    user_id: str
    def __init__(self, resource_id: _Optional[str] = ..., user_id: _Optional[str] = ...) -> None: ...

class ServiceInfo(_message.Message):
    __slots__ = ("name", "status", "compiled", "enabled")
    NAME_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    COMPILED_FIELD_NUMBER: _ClassVar[int]
    ENABLED_FIELD_NUMBER: _ClassVar[int]
    name: str
    status: str
    compiled: bool
    enabled: bool
    def __init__(self, name: _Optional[str] = ..., status: _Optional[str] = ..., compiled: _Optional[bool] = ..., enabled: _Optional[bool] = ...) -> None: ...

class ServiceRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class ServiceOperationResponse(_message.Message):
    __slots__ = ("error", "service")
    ERROR_FIELD_NUMBER: _ClassVar[int]
    SERVICE_FIELD_NUMBER: _ClassVar[int]
    error: ErrorCluster
    service: ServiceInfo
    def __init__(self, error: _Optional[_Union[ErrorCluster, _Mapping]] = ..., service: _Optional[_Union[ServiceInfo, _Mapping]] = ...) -> None: ...

class ListServicesResponse(_message.Message):
    __slots__ = ("error", "services")
    ERROR_FIELD_NUMBER: _ClassVar[int]
    SERVICES_FIELD_NUMBER: _ClassVar[int]
    error: ErrorCluster
    services: _containers.RepeatedCompositeFieldContainer[ServiceInfo]
    def __init__(self, error: _Optional[_Union[ErrorCluster, _Mapping]] = ..., services: _Optional[_Iterable[_Union[ServiceInfo, _Mapping]]] = ...) -> None: ...

class ServerStatusResponse(_message.Message):
    __slots__ = ("error", "api_version", "uptime_secs", "device_count", "services")
    ERROR_FIELD_NUMBER: _ClassVar[int]
    API_VERSION_FIELD_NUMBER: _ClassVar[int]
    UPTIME_SECS_FIELD_NUMBER: _ClassVar[int]
    DEVICE_COUNT_FIELD_NUMBER: _ClassVar[int]
    SERVICES_FIELD_NUMBER: _ClassVar[int]
    error: ErrorCluster
    api_version: str
    uptime_secs: int
    device_count: int
    services: _containers.RepeatedCompositeFieldContainer[ServiceInfo]
    def __init__(self, error: _Optional[_Union[ErrorCluster, _Mapping]] = ..., api_version: _Optional[str] = ..., uptime_secs: _Optional[int] = ..., device_count: _Optional[int] = ..., services: _Optional[_Iterable[_Union[ServiceInfo, _Mapping]]] = ...) -> None: ...
