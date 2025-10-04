#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
//#![deny(unused_crate_dependencies)]
#![feature(impl_trait_in_assoc_type)]
#![feature(type_alias_impl_trait)]

use core::sync::atomic::AtomicBool;

use static_cell::make_static;

use embassy_executor::Spawner;

//use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;

use esp_mbedtls::Tls;

use panic_halt as _;

mod network;
mod server;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Numbers higher than this cause memory errors on a c3
// With a c6 increasing this number and allocating more memory with esp_alloc
// can make 2 simultaneous tasks work
pub const SERVER_SOCKETS: usize = 1;
pub const TOTAL_SOCKETS: usize = SERVER_SOCKETS + 1;

pub static DEVICE_LOCK: AtomicBool = AtomicBool::new(true);

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);
    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    let mut rng = Rng::new(peripherals.RNG);
    let random_seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let wifi_init = make_static!(
        esp_wifi::init(timg0.timer0, rng).expect("Failed to initialize WIFI/BLE controller")
    );
    let (wifi_controller, interfaces) = esp_wifi::wifi::new(wifi_init, peripherals.WIFI)
        .expect("Failed to initialize WIFI controller");

    let stack = network::init(&spawner, random_seed, wifi_controller, interfaces.sta);

    let tls = make_static!(Tls::new(peripherals.SHA)
        .unwrap()
        .with_hardware_rsa(peripherals.RSA));
    tls.set_debug(0);

    spawner.must_spawn(server::serve(stack, tls));
}
