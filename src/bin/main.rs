#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::otg_fs::Usb;
use esp_hal::peripherals::TIMG1;
use esp_hal::timer::timg::{TimerGroup, Wdt};
use log::info;

use AutoMouse::hid::{BUTTON_LEFT, BUTTON_RIGHT, init_usb_mouse, mouse_report};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32s3 -o esp32s3-wroom-1-octal-psram -o unstable-hal -o alloc -o wifi -o ble-bleps -o embassy -o stack-smashing-protection -o log -o esp-backtrace -o vscode

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO3
    // - GPIO45
    // - GPIO46
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO27;
    let _ = peripherals.GPIO28;
    let _ = peripherals.GPIO29;
    let _ = peripherals.GPIO30;
    let _ = peripherals.GPIO31;
    let _ = peripherals.GPIO32;
    let _ = peripherals.GPIO33;
    let _ = peripherals.GPIO34;
    let _ = peripherals.GPIO35;
    let _ = peripherals.GPIO36;
    let _ = peripherals.GPIO37;

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    // COEX needs more RAM - so we've added some more
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    let (wifi_controller, interfaces) = esp_radio::wifi::new(peripherals.WIFI, Default::default())
        .expect("Failed to initialize Wi-Fi controller");
    // let _connector = BleConnector::new(peripherals.BT, Default::default());

    // Configure the watchdog and start the watchdog task.
    let wdt1 = AutoMouse::mk_static!(
        esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG1>,
        TimerGroup::new(peripherals.TIMG1).wdt
    );
    wdt1.set_timeout(
        esp_hal::timer::timg::MwdtStage::Stage0,
        esp_hal::time::Duration::from_secs(1),
    );
    wdt1.set_stage_action(
        esp_hal::timer::timg::MwdtStage::Stage0,
        esp_hal::timer::timg::MwdtStageAction::ResetSystem,
    );
    wdt1.enable();
    wdt1.feed();
    spawner.spawn(watchdog_task(wdt1).unwrap());

    // Initialize USB HID mouse.
    let usb = Usb::new(peripherals.USB0, peripherals.GPIO20, peripherals.GPIO19);
    let (usb_device, hid_writer) = init_usb_mouse(usb);

    // Initialize GPIO2 and GPIO4 as pull-down inputs for left/right mouse buttons.
    let button_config = InputConfig::default().with_pull(Pull::Down);
    let left_button = Input::new(peripherals.GPIO2, button_config);
    let right_button = Input::new(peripherals.GPIO4, button_config);

    // Create network stack with DHCP
    let net_stack_resources = AutoMouse::mk_static!(
        embassy_net::StackResources<3>,
        embassy_net::StackResources::new()
    );
    let (net_stack, net_runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(embassy_net::DhcpConfig::default()),
        net_stack_resources,
        0x080, // random seed
    );
    let net_stack = AutoMouse::mk_static!(embassy_net::Stack<'static>, net_stack);

    // Spawn network task
    spawner.spawn(AutoMouse::wifi::net_task(net_runner).expect("Failed to spawn network task"));

    // Static reference for WiFi controller
    let wifi_controller =
        AutoMouse::mk_static!(esp_radio::wifi::WifiController<'static>, wifi_controller);

    // Spawn WiFi connection task with network stack
    spawner.spawn(
        AutoMouse::wifi::wifi_task(wifi_controller, net_stack, spawner)
            .expect("Failed to spawn WiFi task"),
    );

    // Start the USB, mouse and button tasks.
    spawner.spawn(usb_task(usb_device).unwrap());
    spawner.spawn(mouse_task(hid_writer).unwrap());
    spawner.spawn(button_task(left_button, right_button).unwrap());
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

// Watchdog that gets fed every 500 ms
#[embassy_executor::task]
async fn watchdog_task(watchdog: &'static mut Wdt<TIMG1<'static>>) {
    loop {
        watchdog.feed();
        Timer::after(Duration::from_millis(500)).await;
    }
}

// Drives the USB device stack.
#[embassy_executor::task]
async fn usb_task(
    mut usb_device: embassy_usb::UsbDevice<'static, esp_hal::otg_fs::asynch::Driver<'static>>,
) {
    usb_device.run().await;
}

// Reads mouse reports from the shared channel and sends them over the HID interface.
#[embassy_executor::task]
async fn mouse_task(
    mut hid_writer: embassy_usb::class::hid::HidWriter<
        'static,
        esp_hal::otg_fs::asynch::Driver<'static>,
        8,
    >,
) {
    hid_writer.ready().await;

    loop {
        let report = AutoMouse::hid::MOUSE_CHANNEL.receive().await;
        let _ = hid_writer.write_serialize(&report).await;
    }
}

// Polls GPIO2 (left) and GPIO4 (right) and pushes mouse reports into the shared channel.
#[embassy_executor::task]
async fn button_task(left_button: Input<'static>, right_button: Input<'static>) {
    let mut left_was_pressed = false;
    let mut right_was_pressed = false;

    loop {
        let left_pressed = left_button.is_high();
        let right_pressed = right_button.is_high();

        let mut buttons = 0u8;
        if left_pressed {
            buttons |= BUTTON_LEFT;
        }
        if right_pressed {
            buttons |= BUTTON_RIGHT;
        }

        let state_changed = left_pressed != left_was_pressed || right_pressed != right_was_pressed;

        if state_changed || buttons != 0 {
            // Drag only while a button is held; no movement on release.
            let (x, y) = if buttons != 0 { (2, 2) } else { (0, 0) };
            let _ = AutoMouse::hid::MOUSE_CHANNEL
                .send(mouse_report(buttons, x, y, 0, 0))
                .await;

            if left_pressed != left_was_pressed {
                info!("Left button {}", if left_pressed { "down" } else { "up" });
            }
            if right_pressed != right_was_pressed {
                info!("Right button {}", if right_pressed { "down" } else { "up" });
            }
        }

        left_was_pressed = left_pressed;
        right_was_pressed = right_pressed;
        Timer::after(Duration::from_millis(10)).await;
    }
}
