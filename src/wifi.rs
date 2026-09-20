use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{Ssid, scan::ScanConfig, sta::StationConfig};
use log::{error, info};
extern crate alloc;

// Wi-Fi credentials live in `src/wifi_credentials.rs`, which is git-ignored.
// `include!` bakes them into the binary at compile time.
// Copy `src/wifi_credentials.rs.example` there and fill in your own values.
include!("wifi_credentials.rs");

// Network task - runs the embassy-net runner
#[embassy_executor::task]
pub async fn net_task(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::Interface<'static>>,
) {
    runner.run().await;
}

// WiFi connection task
#[embassy_executor::task]
pub async fn wifi_task(
    wifi_controller: &'static mut esp_radio::wifi::WifiController<'static>,
    net_stack: &'static embassy_net::Stack<'static>,
    spawner: Spawner,
) {
    loop {
        if !wifi_controller.is_connected() {
            find_and_connect_wifi(wifi_controller, net_stack, spawner).await;
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

pub async fn find_and_connect_wifi(
    wifi_controller: &mut esp_radio::wifi::WifiController<'_>,
    net_stack: &'static embassy_net::Stack<'static>,
    spawner: Spawner,
) {
    // Scan for WiFi networks
    let scan_config = ScanConfig::default().with_ssid(WIFI_SSID).with_max(20);
    match wifi_controller.scan_async(&scan_config).await {
        Ok(aps) => {
            let mut found = false;

            for ap in aps {
                let ssid = ap.ssid.as_str();
                if ssid == WIFI_SSID {
                    found = true;
                    break;
                }
            }

            if !found {
                return;
            }

            // Configure WiFi connection with SSID and password
            let station_config = StationConfig::default()
                .with_ssid(Ssid::from(WIFI_SSID))
                .with_password(alloc::string::ToString::to_string(&WIFI_PASSWORD));

            if let Err(e) =
                wifi_controller.set_config(&esp_radio::wifi::Config::Station(station_config))
            {
                error!("Failed to set WiFi config: {:?}", e);
                return;
            }

            // Connect to WiFi
            match wifi_controller.connect_async().await {
                Ok(info) => {
                    info!("Connected to WiFi '{}': {:?}", WIFI_SSID, info);

                    // Wait for DHCP to assign IP address
                    info!("Waiting for DHCP configuration...");
                    net_stack.wait_config_up().await;

                    // Get and print IP address
                    if let Some(config) = net_stack.config_v4() {
                        info!("DHCP configured! IP address: {}", config.address.address());
                        if let Some(gateway) = config.gateway {
                            info!("Gateway: {}", gateway);
                        }
                        for dns in config.dns_servers {
                            info!("DNS server: {}", dns);
                        }
                    }

                    // mqtt 连接
                    spawner.spawn(
                        crate::mqtt::mqtt_task(net_stack).expect("Failed to spawn MQTT task"),
                    );
                }
                Err(e) => {
                    error!("Failed to connect to WiFi '{}': {:?}", WIFI_SSID, e);
                }
            }
        }
        Err(e) => {
            error!("Failed to scan WiFi: {:?}", e);
        }
    }
}
