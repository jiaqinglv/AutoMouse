# AutoMouse

AutoMouse is a minimal USB HID mouse firmware for the ESP32-S3, written in Rust.

## Features

- USB HID mouse device using `embassy-usb` and `usbd-hid`
- Left-click and right-click buttons via GPIO inputs
- Cursor drag while a button is held
- Report descriptor and `MouseReport` type compatible with the RMK keyboard firmware's mouse implementation

## Hardware Requirements

- ESP32-S3 development board
- USB cable connected to the board's USB pins (GPIO20 D+ / GPIO19 D-)
- Two momentary buttons (optional; internal pull-downs are used)

## Pinout

| Pin | Function | Notes |
|-----|----------|-------|
| GPIO2 | Left mouse button | Pull-down input, active high |
| GPIO4 | Right mouse button | Pull-down input, active high |
| GPIO19 | USB D- | Fixed function for USB OTG |
| GPIO20 | USB D+ | Fixed function for USB OTG |

## Build

This project targets `xtensa-esp32s3-none-elf` and uses the `esp` Rust toolchain.

```sh
cargo build
```

## Flash

The `.cargo/config.toml` runner is configured for `espflash`. Connect the board and run:

```sh
cargo run
```

This will build the firmware, flash it, and open the serial monitor.

## Behavior

On startup the board enumerates as a USB HID mouse. When you press or release a button, the corresponding HID report is sent to the host:

- Press GPIO2: left button down
- Release GPIO2: left button up
- Press GPIO4: right button down
- Release GPIO4: right button up
- Hold either button: a small cursor drag (x=2, y=2) is reported every 10 ms

The `x` and `y` values in a HID mouse report are **relative deltas**, not absolute coordinates. The host maintains the cursor position and adds each delta to the current position. For example, `x=2` moves the cursor 2 pixels to the right, and `x=-3` moves it 3 pixels to the left.

## Project Structure

```text
src/
├── bin/main.rs    # Application entry point, USB/GPIO setup, and tasks
├── hid/mod.rs     # USB HID mouse setup and button constants
└── lib.rs         # Shared `mk_static!` helper
```

## Dependencies

- `esp-hal` 1.x with the `esp32s3` feature
- `embassy-usb` 0.6
- `usbd-hid` 0.10
- `esp-rtos` 0.3 with Embassy integration

## License

Copyright (c) AutoMouse authors. See the source repository for license details.
