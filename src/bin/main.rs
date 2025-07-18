#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use bt_hci::controller::ExternalController;
use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_hal_02::blocking::i2c::{Read, Write, WriteRead};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Io;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::ble::controller::BleConnector;
use panic_rtt_target as _;
use static_cell::StaticCell;
use esp_sgp41_VOC_NOx::{calculate_crc, prepare_temp_hum_params};
use esp_sgp41_VOC_NOx::hal::{HalI2c, I2cCompat};

extern crate alloc;

// SGP41 Commands
const SGP41_ADDR: u8 = 0x59;
const CMD_EXECUTE_CONDITIONING: [u8; 2] = [0x26, 0x12];
const CMD_MEASURE_RAW_SIGNALS: [u8; 2] = [0x26, 0x19];


/// Runs the mandatory 10‑second SGP41 conditioning phase.
/// Blocks until the phase is finished.
async fn sgp41_conditioning_task(i2c: &mut I2cCompat<'static>, duration_secs: u8) {
    info!(
        "Starting SGP41 conditioning phase ({} seconds)…",
        duration_secs
    );

    for i in 1..=duration_secs {
        info!("  Conditioning {}/{}", i, duration_secs);

        // 25°C / 50%RH dummy compensation values
        let params = prepare_temp_hum_params(25.0, 50.0);
        let mut cmd = [0u8; 8];
        cmd[0..2].copy_from_slice(&CMD_EXECUTE_CONDITIONING);
        cmd[2..8].copy_from_slice(&params);

        if i2c.write(SGP41_ADDR, &cmd).is_ok() {
            Timer::after(Duration::from_millis(50)).await;

            // Read VOC raw value (3‑byte reply: 2 data + CRC)
            let mut buf = [0u8; 3];
            match i2c.read(SGP41_ADDR, &mut buf) {
                Ok(()) => {
                    let voc_raw = u16::from_be_bytes([buf[0], buf[1]]);
                    info!("    VOC raw: {}", voc_raw);
                }
                Err(_) => warn!("    Failed to read conditioning result"),
            }
        } else {
            warn!("    Conditioning command failed");
        }

        Timer::after(Duration::from_secs(1)).await;
    }

    info!("Conditioning complete!");
}

#[embassy_executor::task]
async fn sgp41_measurement_task(mut i2c: I2cCompat<'static>) {
    // --- VOC/NOx index calibration constants ---
    const VOC_OFFSET: f32 = 25000.0;
    const VOC_SCALE: f32 = 50.0;   // tune so that raw≈30449 → index≈104
    const NOX_OFFSET: f32 = 25000.0;
    const NOX_SCALE: f32 = 50.0;

    info!("Starting normal measurements…");

    loop {
        // Prepare measurement command with temperature (25 °C) and humidity (50 % RH).
        let params = prepare_temp_hum_params(25.0, 50.0);
        let mut cmd_with_params = [0u8; 8];
        cmd_with_params[0] = CMD_MEASURE_RAW_SIGNALS[0];
        cmd_with_params[1] = CMD_MEASURE_RAW_SIGNALS[1];
        cmd_with_params[2..8].copy_from_slice(&params);

        if i2c.write(SGP41_ADDR, &cmd_with_params).is_err() {
            error!("Failed to send measurement command");
            Timer::after(Duration::from_secs(1)).await;
            continue;
        }

        Timer::after(Duration::from_millis(50)).await;

        let mut buffer = [0u8; 6];
        if i2c.read(SGP41_ADDR, &mut buffer).is_err() {
            error!("Failed to read SGP41 measurement data");
            Timer::after(Duration::from_secs(1)).await;
            continue;
        }

        let voc_raw = u16::from_be_bytes([buffer[0], buffer[1]]);
        let nox_raw = u16::from_be_bytes([buffer[3], buffer[4]]);

        info!("SGP41 Raw Measurements:");
        info!("  VOC Raw: {} ticks", voc_raw);
        info!("  NOx Raw: {} ticks", nox_raw);

        let voc_index = ((voc_raw as f32 - VOC_OFFSET) / VOC_SCALE).max(0.0);
        let nox_index = ((nox_raw as f32 - NOX_OFFSET) / NOX_SCALE).max(0.0);

        info!("  VOC Index (approx): {}", voc_index as u32);
        info!("  NOx Index (approx): {}", nox_index as u32);

        if voc_index > 180.0 {
            warn!("High VOC levels detected!");
        }
        if nox_index > 30.0 {
            warn!("High NOx levels detected!");
        }

        Timer::after(Duration::from_secs(1)).await;
    }
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

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

    let i2c_config = I2cConfig::default().with_frequency(esp_hal::time::Rate::from_khz(100));

    static RAW_I2C_CELL: StaticCell<HalI2c<'static>> = StaticCell::new();

    let raw_i2c = match I2c::new(peripherals.I2C0, i2c_config) {
        Ok(i2c) => i2c.with_sda(sda).with_scl(scl),
        Err(_) => {
            error!("I2C initialization failed");
            loop {
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    };

    let raw_i2c = RAW_I2C_CELL.init(raw_i2c);

    // Wrap esp-hal I2C so it satisfies the embedded-hal 0.2 traits
    let mut i2c = I2cCompat::new(raw_i2c);

    info!("I2C initialized on SDA=GPIO4, SCL=GPIO5");

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

    // Initialize WiFi/BLE
    let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let wifi_init = esp_wifi::init(timer1.timer0, rng, peripherals.RADIO_CLK)
        .expect("Failed to initialize WIFI/BLE controller");

    let transport = BleConnector::new(&wifi_init, peripherals.BT);
    let _ble_controller = ExternalController::<_, 20>::new(transport);

    // Run the conditioning phase before entering the main measurement loop.
    sgp41_conditioning_task(&mut i2c, 10).await;

    // Hand the I²C bus to the measurement task and let it run in the background.
    _spawner.spawn(sgp41_measurement_task(i2c)).unwrap();

    // Nothing else to do here; park the main task.
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}