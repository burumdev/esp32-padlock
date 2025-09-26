use core::net::Ipv4Addr;

use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};

use esp_println::println;

use esp_wifi::wifi::ClientConfiguration as WifiClientConfig;
use esp_wifi::wifi::Configuration as WifiConfig;
use esp_wifi::wifi::WifiController;
use esp_wifi::wifi::WifiDevice;
use esp_wifi::wifi::WifiEvent;
use esp_wifi::wifi::WifiState;

use static_cell::make_static;

use crate::WEB_TASK_POOL_SIZE;

const SSID: &str = env!["SSID"];
const PASSWORD_WIFI: &str = env!["PASSWORD_WIFI"];
pub const STATIC_IP: &str = env!["STATIC_IP"];

const MOD: &str = "WIFI";

pub fn init(
    spawner: &Spawner,
    random_seed: u64,
    wifi_controller: WifiController<'static>,
    wifi_device: WifiDevice<'static>,
) -> Stack<'static> {
    let static_ip = parse_ip(STATIC_IP);

    println!("{MOD}: Env static IP: {}", static_ip);

    let (stack, runner) = embassy_net::new(
        wifi_device,
        embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
            address: embassy_net::Ipv4Cidr::new(static_ip, 24),
            gateway: None,
            dns_servers: Default::default(),
        }),
        make_static!(embassy_net::StackResources::<WEB_TASK_POOL_SIZE>::new()),
        random_seed,
    );

    spawner.must_spawn(connection(wifi_controller));
    spawner.must_spawn(net_task(runner));

    stack
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("{MOD}: Begin WIFI connection");
    println!(
        "{MOD}: Device capabilities: {:?}",
        controller.capabilities()
    );

    loop {
        if esp_wifi::wifi::sta_state() == WifiState::StaConnected {
            // wait until we're no longer connected
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            Timer::after(Duration::from_millis(5000)).await
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = WifiConfig::Client(WifiClientConfig {
                ssid: SSID.into(),
                password: PASSWORD_WIFI.into(),
                bssid: None,
                auth_method: Default::default(),
                channel: None,
            });
            controller.set_configuration(&client_config).unwrap();
            println!("{MOD}: Starting");
            controller.start_async().await.unwrap();
            println!("{MOD}: Started successfully");
        }

        println!("{MOD}: Attempting to connect...");

        match controller.connect_async().await {
            Ok(_) => {
                println!("{MOD}: Connection successful.");
            }
            Err(e) => {
                println!("{MOD}: Failed to connect to wifi: {:?}", e);
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, WifiDevice<'static>>) -> ! {
    println!("{MOD}: Network task running forever...");
    runner.run().await
}

pub fn parse_ip(static_ip_str: &str) -> Ipv4Addr {
    let mut ip_array = [0u8; 4];
    static_ip_str
        .split(".")
        .enumerate()
        .for_each(|(index, u8str)| ip_array[index] = u8str.parse().unwrap());

    Ipv4Addr::new(ip_array[0], ip_array[1], ip_array[2], ip_array[3])
}
