# Opal Kelly FrontPanel 6.1.0 SDK Directory

Proprietary Opal Kelly FrontPanel SDK headers, shared libraries, and sample bitfiles are **gitignored** and must not be committed to git.

## Local Setup (Option A — Populate `FrontPanel-6.1.0/`)

Copy or symlink the following files from your licensed [Opal Kelly Pins](https://pins.opalkelly.com/downloads) **FrontPanel 6.1.0** installation into this directory:

- `okFrontPanel.h` (required at compile time by `crates/ok-fpga-ffi/build.rs`)
- `okFrontPanelDLL.h` *(optional)*
- **Linux:** `libokFrontPanel.so`
- **Windows:** `okFrontPanel.dll` and `okFrontPanel.lib`
- **Hardware Tests *(optional)*:** Place sample bitfiles (`Counters.bit`, etc.) under `FrontPanel-6.1.0/ExampleBitfiles/`

### Linux USB Permissions (`udev`)

Install the included udev rule so non-root users can open connected Opal Kelly USB 3.0 devices:

```bash
sudo cp FrontPanel-6.1.0/60-opalkelly.rules /etc/udev/rules.d/60-opalkelly.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## External SDK Directory (Option B — Environment Variables)

Instead of copying files into `FrontPanel-6.1.0/`, you can point the build script and runtime loader at an external SDK installation:

```bash
export OK_FRONTPANEL_SDK_DIR=/opt/OpalKelly/FrontPanel-6.1.0
# Optional explicit library override:
export OKFPGA_DLL_PATH=/opt/OpalKelly/FrontPanel-6.1.0/libokFrontPanel.so
export LD_LIBRARY_PATH=/opt/OpalKelly/FrontPanel-6.1.0:${LD_LIBRARY_PATH:-}
```
