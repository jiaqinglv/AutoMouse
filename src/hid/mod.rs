use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::class::hid::{HidBootProtocol, HidSubclass, HidWriter, State};
use embassy_usb::{Builder, Config, UsbDevice};
use esp_hal::otg_fs::Usb;
use esp_hal::otg_fs::asynch::{Config as OtgConfig, Driver};
use usbd_hid::descriptor::{MouseReport, SerializedDescriptor};

/// Channel used by any producer (buttons, MQTT, etc.) to send mouse reports
/// to the single task that owns the USB HID writer.
pub static MOUSE_CHANNEL: Channel<CriticalSectionRawMutex, MouseReport, 8> = Channel::new();

const VID: u16 = 0x303a;
const PID: u16 = 0x4001;
const MANUFACTURER: &str = "AutoMouse";
const PRODUCT: &str = "AutoMouse USB HID Mouse";
const SERIAL: &str = "00000001";

/// Initialize the USB peripheral as a single HID mouse device.
///
/// Returns the `UsbDevice` (must be driven by `usb_device.run().await`) and
/// the `HidWriter` used to send mouse reports.
pub fn init_usb_mouse(
    usb: Usb<'static>,
) -> (
    UsbDevice<'static, Driver<'static>>,
    HidWriter<'static, Driver<'static>, 8>,
) {
    // Static buffers required by embassy-usb and the OTG driver.
    let ep_out_buffer = crate::mk_static!([u8; 1024], [0u8; 1024]);
    let config_descriptor_buf = crate::mk_static!([u8; 256], [0u8; 256]);
    let bos_descriptor_buf = crate::mk_static!([u8; 256], [0u8; 256]);
    let msos_descriptor_buf = crate::mk_static!([u8; 256], [0u8; 256]);
    let control_buf = crate::mk_static!([u8; 64], [0u8; 64]);

    let driver = Driver::new(usb, ep_out_buffer, OtgConfig::default());

    let mut config = Config::new(VID, PID);
    config.manufacturer = Some(MANUFACTURER);
    config.product = Some(PRODUCT);
    config.serial_number = Some(SERIAL);
    config.max_power = 100;
    config.max_packet_size_0 = 64;
    config.composite_with_iads = false;
    config.device_class = 0x00;
    config.device_sub_class = 0x00;
    config.device_protocol = 0x00;

    let mut builder = Builder::new(
        driver,
        config,
        config_descriptor_buf,
        bos_descriptor_buf,
        msos_descriptor_buf,
        control_buf,
    );

    let hid_state = crate::mk_static!(State<'static>, State::new());
    let hid_config = embassy_usb::class::hid::Config {
        report_descriptor: MouseReport::desc(),
        request_handler: None,
        poll_ms: 10,
        max_packet_size: 8,
        hid_subclass: HidSubclass::No,
        hid_boot_protocol: HidBootProtocol::None,
    };

    let hid_writer = HidWriter::<Driver<'static>, 8>::new(&mut builder, hid_state, hid_config);
    let usb_device = builder.build();

    (usb_device, hid_writer)
}

/// Bit mask for the left mouse button.
pub const BUTTON_LEFT: u8 = 1 << 0;
/// Bit mask for the right mouse button.
pub const BUTTON_RIGHT: u8 = 1 << 1;
/// Bit mask for the middle mouse button.
pub const BUTTON_MIDDLE: u8 = 1 << 2;

/// Build a mouse report with the given button mask and optional movement deltas.
pub const fn mouse_report(buttons: u8, x: i8, y: i8, wheel: i8, pan: i8) -> MouseReport {
    MouseReport {
        buttons,
        x,
        y,
        wheel,
        pan,
    }
}
