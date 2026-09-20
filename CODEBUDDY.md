# CODEBUDDY.md

This file provides guidance to CodeBuddy Code when working with code in this repository.

## Project context

AutoMouse is a `#![no_std]` / `#![no_main]` USB HID mouse firmware for the ESP32-S3, written in Rust. It enumerates as a USB mouse and accepts input from two sources: physical GPIO buttons (GPIO2/GPIO4) and MQTT messages received over Wi-Fi. Both producers feed a single channel consumed by the HID writer task.

## Toolchain

- Target triple: `xtensa-esp32s3-none-elf` (set in `.cargo/config.toml`)
- Rust toolchain channel: `esp` (set in `rust-toolchain.toml`) — **not** stable. `cargo build` will fail if the standard stable toolchain is used. rust-analyzer is configured (`.vscode/settings.json`) to use `stable` for indexing but `esp` for actual cargo builds.
- `build-std = ["alloc", "core"]` is required (unstable flag) because the target has no prebuilt std.
- Linker script `linkall.x` is added by `build.rs`; the build.rs also acts as a linker error helper (prints hints for missing `esp-alloc`, `defmt`, `esp-rtos`, etc.).

## Common commands

```sh
cargo build          # Build the firmware (dev profile, opt-level="s")
cargo run            # Build + flash via espflash + open serial monitor (runner in .cargo/config.toml)
cargo clippy         # Lint — note the denies below; do not silence them
cargo build --release
```

There is no test harness, no host-side runner, and no CI configuration in this repo. The `.vscode/tasks.json` only defines a `build-debug` shell task running `cargo build`.

## Clippy rules (do not violate)

In `src/bin/main.rs`:
- `#![deny(clippy::mem_forget)]` — `mem::forget` is unsafe with `esp_hal` types that hold transfer buffers.
- `#![deny(clippy::large_stack_frames)]` — stack is constrained on embedded; `.clippy.toml` sets `stack-size-threshold = 1024`.

`main` itself is `#[allow(clippy::large_stack_frames)]` because large buffers in `main` are expected.

## Build profiles

Both `dev` and `release` use `opt-level = "s"` (size). The dev profile is intentionally not `opt-level=0` — the default debug profile is too large/slow for this chip. `release` additionally uses `lto = 'fat'` and `codegen-units = 1`. Do not change these casually; binary size matters for the partition layout.

## Architecture

### Task graph

`main` (`src/bin/main.rs`) initializes hardware, allocates two heap regions via `esp_alloc::heap_allocator!` (the second 64 KiB exists because Wi-Fi/BLE coexistence needs more RAM), starts `esp_rtos` on `TIMG0`, then spawns these Embassy tasks:

- `watchdog_task` — feeds `TIMG1` WDT every 500 ms (timeout 1 s, reset-system on expiry).
- `usb_task` — drives `embassy_usb::UsbDevice::run()` for the lifetime of the program.
- `mouse_task` — sole consumer of `MOUSE_CHANNEL`; awaits `hid_writer.ready()` then loops on `MOUSE_CHANNEL.receive()` → `hid_writer.write_serialize()`.
- `button_task` — polls GPIO2 (left) / GPIO4 (right) every 10 ms; emits a report on any state change, and a drag report (`x=2, y=2`) every 10 ms while any button is held.
- `wifi::net_task` — runs `embassy_net::Runner` (required for the network stack to make progress).
- `wifi::wifi_task` — scans for the configured SSID, connects, waits for DHCP, then **spawns** `mqtt::mqtt_task` via the `Spawner` passed into it. Re-checks connectivity every 5 s.

### The single mouse pipeline

```
GPIO buttons ─┐
              ├─► hid::MOUSE_CHANNEL ─► mouse_task ─► HidWriter ─► USB
MQTT publish ─┘
```

`hid::MOUSE_CHANNEL` (`Channel<CriticalSectionRawMutex, MouseReport, 8>`, defined in `src/hid/mod.rs`) is the integration point. **Any new input source must push `MouseReport`s into this channel** rather than touching the HID writer directly — only `mouse_task` owns the writer.

The MQTT payload wire format (see `src/mqtt.rs`) is 13 bytes, little-endian:
`[u64 msg_id][u8 buttons][i8 x][i8 y][i8 wheel][i8 pan]`. `AutoMouseMessage` documents this. The `buttons` byte uses the `BUTTON_LEFT` / `BUTTON_RIGHT` / `BUTTON_MIDDLE` masks from `hid`.

### Module layout

- `src/bin/main.rs` — entry point, task spawning, peripheral init. The binary crate is named `AutoMouse` (per `Cargo.toml` `[[bin]]`), so `use AutoMouse::hid::...` works from the binary.
- `src/lib.rs` — re-exports `hid`, `mqtt`, `wifi` modules and defines the `mk_static!` macro used everywhere to pin `&'static mut` values out of `static_cell::StaticCell` (the standard esp-hal pattern for handing ownership to async tasks).
- `src/hid/mod.rs` — USB HID device construction (`init_usb_mouse`), the `MOUSE_CHANNEL`, button bit constants, and `mouse_report()` constructor. VID `0x303a` / PID `0x4001` (Espressif).
- `src/wifi.rs` — Wi-Fi scan/connect, then spawns the MQTT task.
- `src/mqtt.rs` — MQTT v5 client using `rust-mqtt` over a TCP socket; subscribes to `mouse/auto` and forwards decoded reports to `MOUSE_CHANNEL`.

### Configuration that is hardcoded (not env-driven)

- `WIFI_SSID` / `WIFI_PASSWORD` — `src/wifi.rs`.
- `MQTT_BROKER` (IPv4 string) / `MQTT_PORT` / `MQTT_CLIENT_ID` / `MQTT_TOPIC` — `src/mqtt.rs`.
- USB VID/PID/strings — `src/hid/mod.rs`.
- Heap sizes — `src/bin/main.rs` (`73744` + `64 * 1024` bytes).

`ESP_LOG=info` is set in `.cargo/config.toml`; `esp_println::logger::init_logger_from_env()` in `main` reads it.

## Conventions for edits

- New tasks must be `#[embassy_executor::task]` async fns and are spawned from `main` (or from another task that receives a `Spawner`, like `wifi_task` spawning `mqtt_task`).
- Anything that needs to outlive the spawning function must be placed in a `mk_static!` cell — there is no other way to obtain `&'static mut` on this target.
- Keep `#![no_std]` / `#![no_main]` on the binary; do not introduce `std`.
- The `MouseReport` type and its descriptor (from `usbd-hid`) are intentionally compatible with RMK's mouse implementation — do not change the field order or descriptor.
- Heap allocator macros (`esp_alloc::heap_allocator!`) must run before any code that allocates (notably Wi-Fi init).

## Dependencies of note

`esp-hal ~1.1.0` (features `esp32s3`, `log-04`, `unstable`), `esp-rtos 0.3` (Embassy integration), `esp-radio 0.18` (Wi-Fi + BLE coexistence), `embassy-usb 0.6`, `usbd-hid 0.10`, `embassy-net 0.9` (DHCP/TCP/UDP), `rust-mqtt 0.5` (MQTT v5, `bump` feature for the `BumpBuffer` used in `mqtt.rs`), `smoltcp 0.13` (socket feature set pinned in `Cargo.toml`). `bleps` is pinned to a specific git rev but BLE is currently commented out in `main.rs`.
