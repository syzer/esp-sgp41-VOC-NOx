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

extern crate alloc;

// ─────────────────────────────────────────────────────────────────────────────
// Simple shim that lets an `embedded-hal 1.0` I²C implementation satisfy the
// *blocking* traits from `embedded-hal 0.2` (needed by SGP41).

pub type HalI2c<'a> = I2c<'a, esp_hal::Blocking>;

pub struct I2cCompat<'a> {
    inner: &'a mut HalI2c<'a>,
}

impl<'a> I2cCompat<'a> {
    pub fn new(inner: &'a mut HalI2c<'a>) -> Self {
        Self { inner }
    }
}

impl<'a> Write for I2cCompat<'a> {
    type Error = esp_hal::i2c::master::Error;
    fn write(&mut self, addr: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        self.inner.write(addr, bytes)
    }
}

impl<'a> Read for I2cCompat<'a> {
    type Error = esp_hal::i2c::master::Error;
    fn read(&mut self, addr: u8, buf: &mut [u8]) -> Result<(), Self::Error> {
        self.inner.read(addr, buf)
    }
}

impl<'a> WriteRead for I2cCompat<'a> {
    type Error = esp_hal::i2c::master::Error;
    fn write_read(&mut self, addr: u8, bytes: &[u8], buf: &mut [u8]) -> Result<(), Self::Error> {
        self.inner.write_read(addr, bytes, buf)
    }
}
// ─────────────────────────────────────────────────────────────────────────────

// SGP41 Commands
const SGP41_ADDR: u8 = 0x59;
const CMD_EXECUTE_CONDITIONING: [u8; 2] = [0x26, 0x12];
const CMD_MEASURE_RAW_SIGNALS: [u8; 2] = [0x26, 0x19];

// CRC calculation for SGP41
fn calculate_crc(data: &[u8]) -> u8 {
    let mut crc: u8 = 0xFF;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x31;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

// Helper function to prepare temperature and humidity parameters
fn prepare_temp_hum_params(temp_celsius: f32, humidity_percent: f32) -> [u8; 6] {
    // Convert temperature and humidity to SGP41 format
    let humidity_ticks = ((humidity_percent / 100.0) * 65535.0) as u16;
    let temp_ticks = (((temp_celsius + 45.0) / 175.0) * 65535.0) as u16;

    [
        (humidity_ticks >> 8) as u8,
        (humidity_ticks & 0xFF) as u8,
        calculate_crc(&[(humidity_ticks >> 8) as u8, (humidity_ticks & 0xFF) as u8]),
        (temp_ticks >> 8) as u8,
        (temp_ticks & 0xFF) as u8,
        calculate_crc(&[(temp_ticks >> 8) as u8, (temp_ticks & 0xFF) as u8]),
    ]
}

/// Runs the mandatory 10‑second SGP41 conditioning phase.
/// Blocks until the phase is finished.
async fn sgp41_conditioning_task(i2c: &mut I2cCompat<'static>, duration_secs: u8) {
    info!(
        "Starting SGP41 conditioning phase ({} seconds)…",
        duration_secs
    );

    for i in 1..=duration_secs {
        info!("  Conditioning {}/{}", i, duration_secs);

        // 25 °C / 50 % RH dummy compensation values
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

    info!("Starting SGP41 sensor reading loop...");

    // Run the conditioning phase before entering the main measurement loop.
    sgp41_conditioning_task(&mut i2c, 10).await;

    info!("Starting normal measurements…");

    // Main measurement loop
    loop {
        // Prepare measurement command with temperature and humidity
        let params = prepare_temp_hum_params(25.0, 50.0); // 25°C, 50% RH
        let mut cmd_with_params = [0u8; 8];
        cmd_with_params[0] = CMD_MEASURE_RAW_SIGNALS[0];
        cmd_with_params[1] = CMD_MEASURE_RAW_SIGNALS[1];
        cmd_with_params[2..8].copy_from_slice(&params);

        if i2c.write(SGP41_ADDR, &cmd_with_params).is_err() {
            error!("Failed to send measurement command");
            error!("Check sensor connection and power supply");
            Timer::after(Duration::from_secs(1)).await;
            continue; // Retry after 1 second
        }

        embassy_time::Timer::after(Duration::from_millis(50)).await;
        let mut buffer = [0u8; 6]; // 2 bytes VOC + 1 CRC + 2 bytes NOx + 1 CRC
        if i2c.read(SGP41_ADDR, &mut buffer).is_err() {
            error!("Failed to read SGP41 measurement data");
            error!("Check sensor connection and power supply");
            Timer::after(Duration::from_secs(1)).await;
            continue; // Retry after 1 second
        }
        let voc_raw = ((buffer[0] as u16) << 8) | (buffer[1] as u16);
        let nox_raw = ((buffer[3] as u16) << 8) | (buffer[4] as u16);

        info!("SGP41 Raw Measurements:");
        info!("  VOC Raw: {} ticks", voc_raw);
        info!("  NOx Raw: {} ticks", nox_raw);

        // Convert to approximate concentrations
        // Note: For production use, implement the Sensirion Gas Index Algorithm
        // These are simplified approximations
        let voc_index = if voc_raw > 25000 {
            (voc_raw as i32 - 25000) / 100 // Convert to tenths for display
        } else {
            0
        };

        let nox_index = if nox_raw > 25000 {
            (nox_raw as i32 - 25000) / 100 // Convert to tenths for display
        } else {
            0
        };

        info!("  VOC Index (approx): {}.{}", voc_index / 10, voc_index % 10);
        info!("  NOx Index (approx): {}.{}",nox_index / 10,nox_index % 10);

        // Quality indicators
        if voc_index > 100 {
            // voc_index is in tenths, so 100 = 10.0
            warn!("High VOC levels detected!");
        }
        if nox_index > 100 {
            // nox_index is in tenths, so 100 = 10.0
            warn!("High NOx levels detected!");
        }

        // Wait 1 second between measurements
        Timer::after(Duration::from_secs(1)).await;
    }
}
