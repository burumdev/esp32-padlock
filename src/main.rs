#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![feature(impl_trait_in_assoc_type)]
#![feature(type_alias_impl_trait)]

use core::sync::atomic::AtomicBool;

use static_cell::make_static;

use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;

use esp_mbedtls::Tls;

mod network;
mod server;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Numbers higher than this causes memory errors on a c3
// Otherwise adjust with arena size feature flags for
// embassy executor in Config.toml or try allocating more memory with esp_alloc
pub const WEB_TASK_POOL_SIZE: usize = 1;

static DEVICE_LOCK: AtomicBool = AtomicBool::new(true);

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let mut peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);
    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    let mut rng = Rng::new(peripherals.RNG.reborrow());
    let random_seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let wifi_init = make_static!(
        esp_wifi::init(timer1.timer0, rng).expect("Failed to initialize WIFI/BLE controller")
    );
    let (wifi_controller, interfaces) = esp_wifi::wifi::new(wifi_init, peripherals.WIFI)
        .expect("Failed to initialize WIFI controller");

    let stack = network::init(&spawner, random_seed, wifi_controller, interfaces.sta);

    let tls = make_static!(Tls::new(peripherals.SHA)
        .unwrap()
        .with_hardware_rsa(peripherals.RSA));
    tls.set_debug(0);

    for _ in 0..WEB_TASK_POOL_SIZE {
        spawner.must_spawn(server::serve(stack, tls, &DEVICE_LOCK));
    }
}
