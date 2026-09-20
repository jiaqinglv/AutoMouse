# AutoMouse

AutoMouse is a minimal `#![no_std]` USB HID mouse firmware for the ESP32-S3, written in Rust.

It exposes the board as a USB mouse and drives the cursor from two independent input sources: physical GPIO buttons and MQTT messages received over Wi-Fi. Both feed a single shared channel that the HID writer task consumes.

## Features

- USB HID mouse device using `embassy-usb` and `usbd-hid`
- Left-click and right-click buttons via GPIO inputs
- Cursor drag while a button is held
- Remote control over Wi-Fi: subscribes to an MQTT topic and injects whatever mouse reports arrive on it
- Watchdog on `TIMG1` that resets the chip if the firmware hangs
- Report descriptor and `MouseReport` type compatible with the RMK keyboard firmware's mouse implementation

## Hardware Requirements

- ESP32-S3 development board
- USB cable connected to the board's USB pins (GPIO20 D+ / GPIO19 D-)
- Two momentary buttons (optional; internal pull-downs are used)
- A reachable Wi-Fi access point and an MQTT broker on the same network

## Pinout

| Pin | Function | Notes |
|-----|----------|-------|
| GPIO2 | Left mouse button | Pull-down input, active high |
| GPIO4 | Right mouse button | Pull-down input, active high |
| GPIO19 | USB D- | Fixed function for USB OTG |
| GPIO20 | USB D+ | Fixed function for USB OTG |

## Configuration

All configuration is compile-time and lives in the source tree. Nothing is read from the environment, so these values must be edited before flashing:

| Setting | Location |
|---------|----------|
| `WIFI_SSID`, `WIFI_PASSWORD` | `src/wifi.rs` |
| `MQTT_BROKER`, `MQTT_PORT`, `MQTT_CLIENT_ID`, `MQTT_TOPIC` | `src/mqtt.rs` |
| USB VID/PID and descriptor strings | `src/hid/mod.rs` |
| Heap allocator sizes | `src/bin/main.rs` |

Notes:

- The Wi-Fi and MQTT constants ship as placeholders and must be replaced with real values — the firmware cannot connect until you do.
- `MQTT_BROKER` is parsed as an IPv4 address, not a hostname.
- The MQTT client connects over plain TCP: no TLS, no authentication.
- `MQTT_CLIENT_ID` is fixed, so two boards pointed at the same broker will contend for the same session.

Do not commit real credentials. If these values need to be shared across machines, read them from a build-time environment variable via `option_env!` instead of hardcoding them.

## Build

This project targets `xtensa-esp32s3-none-elf` and requires the `esp` Rust toolchain, not stable. `rust-toolchain.toml` pins the channel, and `.cargo/config.toml` sets the target and enables `build-std = ["alloc", "core"]`.

```sh
cargo build          # dev profile
cargo build --release
cargo clippy
```

Both profiles use `opt-level = "s"`; binary size matters for the partition layout.

## Flash

The `.cargo/config.toml` runner is configured for `espflash`. Connect the board and run:

```sh
cargo run
```

This will build the firmware, flash it, and open the serial monitor. `ESP_LOG=info` is set in `.cargo/config.toml` and controls the log level.

## Behavior

On startup the board enumerates as a USB HID mouse. Pressing or releasing a button sends the corresponding HID report to the host:

- Press GPIO2: left button down
- Release GPIO2: left button up
- Press GPIO4: right button down
- Release GPIO4: right button up
- Hold either button: a small cursor drag (x=2, y=2) is reported every 10 ms
- Release: a report with no buttons and zero deltas is sent, which stops the drag

The `x` and `y` values in a HID mouse report are **relative deltas**, not absolute coordinates. The host maintains the cursor position and adds each delta to the current position. For example, `x=2` moves the cursor 2 pixels to the right, and `x=-3` moves it 3 pixels to the left.

## MQTT Protocol

After connecting to Wi-Fi and obtaining a DHCP lease, the firmware subscribes to the configured topic and treats every message on it as one mouse report. The payload is 13 bytes, little-endian:

| Offset | Type | Field |
|--------|------|-------|
| 0..8 | `u64` LE | `msg_id` — correlation id, not interpreted by the firmware |
| 8 | `u8` | `buttons` bit mask |
| 9 | `i8` | `x` delta |
| 10 | `i8` | `y` delta |
| 11 | `i8` | `wheel` — positive scrolls up |
| 12 | `i8` | `pan` — positive scrolls right |

The `buttons` bit mask is:

| Bit | Value | Button |
|-----|-------|--------|
| 0 | `0x01` | Left |
| 1 | `0x02` | Right |
| 2 | `0x04` | Middle |

Any connection or subscription failure is retried after a 10-second delay.

## Architecture

Every input source pushes `MouseReport`s into a single channel rather than touching the HID writer directly:

```text
GPIO buttons ─┐
              ├─► hid::MOUSE_CHANNEL ─► mouse_task ─► HidWriter ─► USB
MQTT publish ─┘
```

`hid::MOUSE_CHANNEL` (`src/hid/mod.rs`) is the integration point. Only `mouse_task` owns the USB HID writer; any new input source must send into the channel instead.

The Embassy tasks spawned from `main` are:

| Task | Responsibility |
|------|----------------|
| `watchdog_task` | Feeds the `TIMG1` watchdog every 500 ms (1 s timeout, resets the chip on expiry) |
| `usb_task` | Runs the `embassy-usb` device stack for the lifetime of the program |
| `mouse_task` | Sole consumer of `MOUSE_CHANNEL`; writes reports to the HID endpoint |
| `button_task` | Polls GPIO2/GPIO4 every 10 ms; emits a report on any state change, and a drag report while a button is held |
| `wifi::net_task` | Runs the `embassy-net` runner so the network stack makes progress |
| `wifi::wifi_task` | Scans, connects, waits for DHCP, then spawns `mqtt::mqtt_task` |

## Project Structure

```text
src/
├── bin/main.rs    # Application entry point, peripheral init, and task spawning
├── hid/mod.rs     # USB HID mouse setup, MOUSE_CHANNEL, and button constants
├── wifi.rs        # Wi-Fi scan/connect and DHCP, then spawns the MQTT task
├── mqtt.rs        # MQTT v5 client subscribed to the mouse topic
└── lib.rs         # Module re-exports and the mk_static! helper
```

## Dependencies

- `esp-hal` 1.1 with the `esp32s3`, `log-04`, and `unstable` features
- `esp-rtos` 0.3 with Embassy integration
- `esp-radio` 0.18 (Wi-Fi + BLE coexistence)
- `embassy-usb` 0.6 and `usbd-hid` 0.10
- `embassy-net` 0.9 (DHCP/TCP) over `smoltcp` 0.13
- `rust-mqtt` 0.5 (MQTT v5, `bump` feature)

`bleps` is pinned to a specific git revision, but BLE is currently commented out in `main.rs`.

## License

Copyright (c) AutoMouse authors. See the source repository for license details.
