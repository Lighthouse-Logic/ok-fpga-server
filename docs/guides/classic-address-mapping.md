# Classic Address Mapping — Developer Guide

> **ADR**: [ADR-025 — Developer-Defined Classic Address Mapping](../architecture/adr/ADR-025-classic-address-mapping.md)  
> **Prerequisite reading**: [ADR-024 — Endpoint Access Enforcement](../architecture/adr/ADR-024-endpoint-access-enforcement.md)

---

## 1. When You Need This

You need a custom address mapping policy when your HDL design meets **all three**
of these criteria:

1. **Classic data port** — your FPGA uses FrontPanel Classic endpoints (WireIn,
   WireOut, TriggerIn, TriggerOut, PipeIn, PipeOut), not AXI
2. **Internal memory-mapped address space** — your Classic endpoints access an
   internal address bus, RAM, FIFO bank, or similar addressable region
3. **Fine-grained access control** — you want different users (or different
   resource holders) to have access to different address ranges within that space

If your design uses `okRegisterBridge`, you may **not** need this — the server
already enforces `ReadRegister`/`WriteRegister` operations against
`address_ranges` automatically. This guide is for designs where the addressing
is done through raw Classic endpoints.

---

## 2. What the Server Already Handles

Before your mapping hook ever fires, the server has already verified:

| Check | Description | You need to handle? |
|-------|-------------|---------------------|
| **Authentication** | User identity is present and valid | ❌ No |
| **Resource acquisition** | User holds a resource on this device | ❌ No |
| **Lock enforcement** | Resource is not locked by another user | ❌ No |
| **Endpoint coverage** | Resource's endpoint declarations cover this endpoint | ❌ No |
| **Address resolution** | Map endpoint + value → memory address | ✅ **Yes — this is your job** |
| **Address range check** | Is the resolved address in a declared range? | ❌ No (server does this) |
| **Exclusive range lock** | Does the user hold the lock for exclusive ranges? | ❌ No (server does this) |

Your custom policy only needs to answer one question:

> "Given endpoint `0x01` (WireIn) with value `0x0042`, what memory address is
> being targeted?"

---

## 3. Understanding the Pattern: Shared Endpoints, Exclusive Addresses

This is the key concept. Consider a design where:

- **WireIn 0x01** = address register (any holder can write to this wire)
- **WireIn 0x02** = data register (any holder can write to this wire)
- **TriggerIn 0x40** = write strobe (any holder can fire this trigger)

These endpoints are **shared** — multiple users need physical access to the same
wire to set addresses and data. You cannot make the wire endpoint exclusive,
because then only one user could touch it.

But the **address space** they access has regions with different access modes:

```
Address Range        Access Mode    Purpose
─────────────────────────────────────────────────
0x0000 – 0x00FF     Exclusive      DAC control registers
0x0100 – 0x01FF     Shared         Status/monitoring registers
0x0200 – 0x02FF     Shared         ADC readback data
```

Enforcement happens at the **resolved address** level:

- User writes `0x0150` (status register) to WireIn 0x01 → address is in a Shared
  range → **allowed** for any holder
- User writes `0x0042` (DAC register) to WireIn 0x01 → address is in an Exclusive
  range → **allowed only if the user holds the resource lock**

This way, all users can read status and ADC data (Shared ranges), but only the
user who locked the resource can modify DAC settings (Exclusive range).

---

## 4. Step-by-Step Implementation

### Step 1: Map Your HDL Design

Document how your HDL uses Classic endpoints to access the internal address space.
For our example design `my_memory_controller.bit`:

```
FPGA Internal Architecture:
┌─────────────────────────────────────────────┐
│                                             │
│  WireIn 0x01  ──→ [Address Register]        │
│  WireIn 0x02  ──→ [Write Data Register]     │
│  TriggerIn 0x40 ──→ [Write Strobe]          │
│  WireOut 0x20  ←── [Read Data Register]     │
│                                             │
│  Address Bus  ──→ ┌─────────────────────┐   │
│                   │ 0x0000–0x00FF: DAC  │   │
│                   │ 0x0100–0x01FF: Stat │   │
│                   │ 0x0200–0x02FF: ADC  │   │
│                   └─────────────────────┘   │
└─────────────────────────────────────────────┘
```

The addressing protocol:
1. Write the target address to WireIn 0x01
2. For writes: write data to WireIn 0x02, fire TriggerIn 0x40
3. For reads: fire TriggerIn 0x40 (read strobe), read WireOut 0x20

### Step 2: Implement `ResourceAccessPolicy`

Create a custom policy that implements `resolve_classic_address()`:

```rust
use ok_fpga_core::resource::{
    ResourceAccessPolicy, Resource, AccessMode, ClassicAddressResolution,
    AcquisitionDenied, EndpointType, UserId,
};

/// Custom access policy for the my_memory_controller HDL design.
///
/// This policy maps Classic endpoint operations to memory-mapped addresses
/// based on the HDL design's internal addressing scheme:
///
/// - WireIn 0x01: address register — the value IS the target address
/// - WireIn 0x02: data register — address is whatever was last set via 0x01
/// - TriggerIn 0x40: write/read strobe — triggers the operation
/// - WireOut 0x20: read data — returns data at the current address
struct MemoryControllerPolicy;

impl ResourceAccessPolicy for MemoryControllerPolicy {
    fn can_acquire(
        &self,
        resource: &Resource,
        user: &UserId,
        requested_mode: AccessMode,
    ) -> Result<(), AcquisitionDenied> {
        // Use the default acquisition logic from ADR-006.
        // You can customize this if your design has special acquisition rules.
        ok_fpga_core::resource::DefaultAccessPolicy.can_acquire(
            resource, user, requested_mode,
        )
    }

    fn resolve_classic_address(
        &self,
        _resource: &Resource,
        _user: &UserId,
        endpoint: u8,
        ep_type: &EndpointType,
        value: Option<u32>,
    ) -> ClassicAddressResolution {
        match (endpoint, ep_type) {
            // WireIn 0x01 is the address register.
            // The value being written IS the target address.
            (0x01, EndpointType::WireIn) => {
                match value {
                    Some(addr) => ClassicAddressResolution::MapsToAddress(addr as u64),
                    None => ClassicAddressResolution::NoMapping,
                }
            }

            // WireIn 0x02 is the data register.
            // It doesn't carry an address — the address was set via 0x01.
            // Allow any holder to write data (the address enforcement
            // happened when they wrote to 0x01).
            (0x02, EndpointType::WireIn) => ClassicAddressResolution::AllowAlways,

            // TriggerIn 0x40 is the write/read strobe.
            // It doesn't carry an address. Allow any holder to fire it.
            (0x40, EndpointType::TriggerIn) => ClassicAddressResolution::AllowAlways,

            // WireOut 0x20 is the read data register.
            // No address information available in a read (value is None).
            // Allow any holder to read.
            (0x20, EndpointType::WireOut) => ClassicAddressResolution::AllowAlways,

            // Any other endpoint — use standard endpoint enforcement.
            _ => ClassicAddressResolution::NoMapping,
        }
    }
}
```

### Step 3: Register Resources with Address Ranges

When a client registers a resource, include the address ranges with their
access modes:

```json
POST /api/v1/resources
{
    "name": "dac_controller",
    "device_serial": "ABC1234567",
    "access_mode": "shared",
    "interfaces": { "classic": true },
    "endpoints": {
        "default_access": "shared",
        "entries": [
            { "address": 1,  "ep_type": "wire_in",    "access": "shared" },
            { "address": 2,  "ep_type": "wire_in",    "access": "shared" },
            { "address": 64, "ep_type": "trigger_in",  "access": "shared" },
            { "address": 32, "ep_type": "wire_out",    "access": "shared" }
        ]
    },
    "address_ranges": [
        { "start": "0x0000", "end": "0x00FF", "access": "exclusive" },
        { "start": "0x0100", "end": "0x01FF", "access": "shared" },
        { "start": "0x0200", "end": "0x02FF", "access": "shared" }
    ]
}
```

Note that all endpoints are declared as `shared` — multiple users can physically
access the wires and triggers. The access control is at the **address** level:

- Writing `0x0042` to WireIn 0x01 → address `0x0042` is in the Exclusive DAC
  range → requires the resource lock
- Writing `0x0150` to WireIn 0x01 → address `0x0150` is in the Shared status
  range → any holder can access

### Step 4: Configure the Server

Register your custom policy when constructing the `ResourceRegistry`:

```rust
let policy = Box::new(MemoryControllerPolicy);
let registry = ResourceRegistry::with_policy(policy);
```

Enable enforcement in `okfpga-server.toml`:

```toml
[access_control]
endpoint_enforcement = "warn"   # Start with warn to audit, then switch to enforce
```

---

## 5. Enforcement in Action

Here's what happens when two users interact with the design:

### Scenario: Alice has the lock, Bob does not

```
1. Alice acquires resource "dac_controller" (Shared)
2. Bob acquires resource "dac_controller" (Shared)
3. Alice locks the resource

4. Alice: SetWireInValue(0x01, 0x0042)
   → resolve_classic_address: MapsToAddress(0x0042)
   → 0x0042 is in Exclusive range 0x0000–0x00FF
   → Alice holds the lock → ✅ ALLOWED

5. Bob: SetWireInValue(0x01, 0x0042)
   → resolve_classic_address: MapsToAddress(0x0042)
   → 0x0042 is in Exclusive range 0x0000–0x00FF
   → Bob does NOT hold the lock → ❌ DENIED
   → 403: "User 'Bob' cannot access exclusive address range
           0x00000000–0x000000FF on resource 'dac_controller'
           without holding the lock"

6. Bob: SetWireInValue(0x01, 0x0150)
   → resolve_classic_address: MapsToAddress(0x0150)
   → 0x0150 is in Shared range 0x0100–0x01FF
   → ✅ ALLOWED (Shared range — any holder can access)

7. Bob: GetWireOutValue(0x20)
   → resolve_classic_address: AllowAlways
   → ✅ ALLOWED (read data — always permitted for holders)
```

---

## 6. Testing Your Policy

Write unit tests for your custom policy to verify the mapping logic:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_in_01_maps_to_address() {
        let policy = MemoryControllerPolicy;
        let resource = test_resource(); // helper to create a test resource

        let result = policy.resolve_classic_address(
            &resource,
            &"alice".to_string(),
            0x01,                      // WireIn 0x01 = address register
            &EndpointType::WireIn,
            Some(0x0042),              // Value = address being set
        );

        match result {
            ClassicAddressResolution::MapsToAddress(addr) => {
                assert_eq!(addr, 0x0042);
            }
            _ => panic!("Expected MapsToAddress"),
        }
    }

    #[test]
    fn test_data_register_allows_always() {
        let policy = MemoryControllerPolicy;
        let resource = test_resource();

        let result = policy.resolve_classic_address(
            &resource,
            &"alice".to_string(),
            0x02,                      // WireIn 0x02 = data register
            &EndpointType::WireIn,
            Some(0xDEADBEEF),         // Data value (not an address)
        );

        assert!(matches!(result, ClassicAddressResolution::AllowAlways));
    }

    #[test]
    fn test_unknown_endpoint_returns_no_mapping() {
        let policy = MemoryControllerPolicy;
        let resource = test_resource();

        let result = policy.resolve_classic_address(
            &resource,
            &"alice".to_string(),
            0x05,                      // Unknown endpoint
            &EndpointType::WireIn,
            Some(0x0000),
        );

        assert!(matches!(result, ClassicAddressResolution::NoMapping));
    }
}
```

---

## 7. Common Patterns

### Pattern A: Single Address Register

The simplest case — one WireIn carries the address, everything else is auxiliary:

```rust
fn resolve_classic_address(&self, .., endpoint: u8, ep_type: &EndpointType,
                           value: Option<u32>) -> ClassicAddressResolution {
    if endpoint == ADDRESS_WIRE && *ep_type == EndpointType::WireIn {
        value.map_or(ClassicAddressResolution::NoMapping,
                     |v| ClassicAddressResolution::MapsToAddress(v as u64))
    } else {
        ClassicAddressResolution::AllowAlways
    }
}
```

### Pattern B: Address in Upper Bits

Some designs pack the address into the upper bits of a wire value:

```rust
// WireIn 0x01: bits [31:16] = address, bits [15:0] = data
fn resolve_classic_address(&self, .., endpoint: u8, ep_type: &EndpointType,
                           value: Option<u32>) -> ClassicAddressResolution {
    if endpoint == 0x01 && *ep_type == EndpointType::WireIn {
        value.map_or(ClassicAddressResolution::NoMapping,
                     |v| ClassicAddressResolution::MapsToAddress((v >> 16) as u64))
    } else {
        ClassicAddressResolution::NoMapping
    }
}
```

### Pattern C: Page Register + Offset

Designs with paged addressing where one wire selects the page and another
provides the offset:

```rust
// This requires tracking state across calls — the policy struct can hold state
struct PagedPolicy {
    current_page: std::sync::Mutex<u32>,
}

fn resolve_classic_address(&self, .., endpoint: u8, ep_type: &EndpointType,
                           value: Option<u32>) -> ClassicAddressResolution {
    match (endpoint, ep_type) {
        (PAGE_WIRE, EndpointType::WireIn) => {
            if let Some(page) = value {
                *self.current_page.lock().unwrap() = page;
            }
            ClassicAddressResolution::AllowAlways // Page select is not an address op
        }
        (OFFSET_WIRE, EndpointType::WireIn) => {
            let page = *self.current_page.lock().unwrap();
            value.map_or(ClassicAddressResolution::NoMapping,
                         |offset| ClassicAddressResolution::MapsToAddress(
                             ((page as u64) << 16) | (offset as u64)
                         ))
        }
        _ => ClassicAddressResolution::NoMapping,
    }
}
```

> **Caution**: Stateful policies (Pattern C) require careful thread-safety
> consideration. The `current_page` may be modified concurrently by different
> users. Consider per-user state if your design requires it.

---

## 8. FAQ

**Q: Do I need this if I use `okRegisterBridge`?**

No. Register bridge operations (`ReadRegister`/`WriteRegister`) already have
explicit address parameters that the server enforces via `check_address_access()`.
This hook is for designs where the addressing is done through raw Classic
endpoints.

**Q: What about PipeIn/PipeOut — how do I enforce pipe data?**

For pipe operations, `value` is `None` because pipe data is a byte stream, not a
single address. Your hook receives `None` and can return:
- `NoMapping` — standard endpoint enforcement (is the user a holder? is the
  endpoint covered?)
- `AllowAlways` — any holder can use this pipe
- `MapsToAddress(...)` — if your design's pipe address is determined by device
  state that your policy code tracks

**Q: What happens if my hook returns `MapsToAddress` but the resource has no
`address_ranges`?**

The server treats a resource with no `address_ranges` as covering **all**
addresses (legacy mode, per ADR-024 §3). The address check will pass.

**Q: Can I deny access from the hook itself?**

The hook returns `ClassicAddressResolution`, not `Result`. It cannot directly
deny — it only resolves addresses. Denial happens when the server checks the
resolved address against address ranges. If you need to deny for other reasons,
use `validate_endpoint_access()` which returns `Result`.
