#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

extern crate alloc;
use bt_hci::controller::ExternalController;
use defmt::{error, info};
use embassy_sync::channel::{Channel as SyncChannel, Receiver, Sender};
use embassy_time::{Duration, Timer};

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_hal_02::blocking::i2c::{Read, Write};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Io;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::Blocking;
use esp_sgp41_voc_nox::hal::{HalI2c, I2cCompat};
use esp_sgp41_voc_nox::led::{Led, LedCommand};
use esp_sgp41_voc_nox::tasks::conditioning::{sgp41_conditioning_task, SGP41_ADDR};
use esp_sgp41_voc_nox::tasks::led::led_task;
use esp_sgp41_voc_nox::tasks::sgp41_measurement::sgp41_measurement_task;
use esp_wifi::ble::controller::BleConnector;
use panic_rtt_target as _;
use static_cell::StaticCell;

use esp_hal::rmt::{Channel as RmtChannel, Rmt};

// ── shared state between the two tasks ───────────────────────────────────────
static I2C_BUS_CELL: StaticCell<Mutex<NoopRawMutex, I2cCompat<'static>>> = StaticCell::new();

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// A bounded queue for LED commands (4 entries)
static LED_QUEUE: StaticCell<SyncChannel<NoopRawMutex, LedCommand, 4>> = StaticCell::new();

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    rtt_target::rtt_init_defmt!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let _io = Io::new(peripherals.IO_MUX);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // Initialize I2C for SGP41 sensor on GPIO4 (SDA) and GPIO5 (SCL)
    let sda = peripherals.GPIO4; // SDA pin
    let scl = peripherals.GPIO5; // SCL pin

    let i2c_config = I2cConfig::default().with_frequency(Rate::from_khz(400));

    static RAW_I2C_CELL: StaticCell<HalI2c<'static>> = StaticCell::new();

    let raw = match I2c::new(peripherals.I2C0, i2c_config) {
        Ok(i2c) => i2c.with_sda(sda).with_scl(scl),
        Err(_) => {
            error!("I2C initialization failed");
            loop {
                Timer::after(Duration::from_millis(1000)).await;
            }
        }
    };
    let raw_i2c = RAW_I2C_CELL.init(raw);

    // ── wrap esp-hal I²C so it satisfies the driver (eh-0.2) traits ────
    let mut i2c = I2cCompat::new(raw_i2c);

    // Test I2C communication by reading serial number
    info!("Testing SGP41 communication...");
    let get_serial_cmd = [0x36, 0x82];
    let mut serial_buffer = [0u8; 9]; // 6 bytes data + 3 CRC bytes

    if i2c.write(SGP41_ADDR, &get_serial_cmd).is_ok() {
        embassy_time::Timer::after(Duration::from_millis(1)).await;
        if i2c.read(SGP41_ADDR, &mut serial_buffer).is_ok() {
            info!(
                "SGP41 connected! Serial: {:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                serial_buffer[0],
                serial_buffer[1],
                serial_buffer[3],
                serial_buffer[4],
                serial_buffer[6],
                serial_buffer[7]
            );
        } else {
            error!("Failed to read SGP41 serial number");
        }
    } else {
        error!("Failed to communicate with SGP41 sensor");
        error!("Check connections: SDA=GPIO4, SCL=GPIO5, VCC=3.3V, GND=GND");
    }

    // ── LED setup for XIAO ESP32-S3 (built-in LED on GPIO21) ──────────
    // Create unified LED API for different chips
    #[cfg(feature = "esp32s3")]
    let mut led = Led::new_gpio(Output::new(peripherals.GPIO21, Level::Low, Default::default()));

    #[cfg(feature = "esp32c6")]
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("Failed to initialize RMT");

    #[cfg(feature = "esp32c6")]
    let mut led_hw = Led::new_ws2812(
        rmt.channel0,
        peripherals.GPIO8,  // WS2812 LED pin for ESP32-C6
    );
    led_hw.set_color_rgb(30, 0, 0);

    static LED_CELL: StaticCell<
        Mutex<NoopRawMutex, Led<RmtChannel<Blocking, 0>>>
    > = StaticCell::new();
    let led: &'static _ = LED_CELL.init(Mutex::new(led_hw));

    // Initialize LED command queue and split sender/receiver
    let led_queue = LED_QUEUE.init(SyncChannel::new());
    let led_sender: Sender<'static, NoopRawMutex, LedCommand, 4> = led_queue.sender();
    let led_sender2 = led_sender;
    let led_receiver: Receiver<'static, NoopRawMutex, LedCommand, 4> = led_queue.receiver();

    // Initialize WiFi/BLE
    let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let wifi_init = esp_wifi::init(timer1.timer0, rng, peripherals.RADIO_CLK)
        .expect("Failed to initialize WIFI/BLE controller");

    let transport = BleConnector::new(&wifi_init, peripherals.BT);
    let _ble_controller = ExternalController::<_, 20>::new(transport);

    // Initialize the shared I2C bus mutex
    let i2c_bus: &'static Mutex<NoopRawMutex, I2cCompat<'static>> =
        I2C_BUS_CELL.init(Mutex::new(i2c));


    // Run the burn‑in first; it will spawn the measurement task when done.
    _spawner.spawn(sgp41_conditioning_task(i2c_bus, 10, led_sender)).unwrap();
    _spawner.spawn(sgp41_measurement_task(i2c_bus, led_sender2)).unwrap();
    _spawner.spawn(led_task(led_receiver, led)).unwrap();

    // Nothing else to do here; park the main task.
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}