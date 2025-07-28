use crate::hal::I2cCompat;
use crate::led::LedCommand;
use crate::prepare_temp_hum_params;
use core::sync::atomic::{AtomicBool, Ordering};
use defmt::{info, warn};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Sender;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_hal_02::blocking::i2c::{Read, Write};
use gas_index_algorithm::GasIndexAlgorithm;
use core::cell::RefCell;

pub static CONDITION_DONE: AtomicBool = AtomicBool::new(false);
pub const SGP41_ADDR: u8 = 0x59;


// SGP41 Commands
pub const CMD_EXECUTE_CONDITIONING: [u8; 2] = [0x26, 0x12];

pub const CMD_MEASURE_RAW_SIGNALS: [u8; 2] = [0x26, 0x19];


#[embassy_executor::task]
pub async fn sgp41_conditioning_task(
    bus: &'static Mutex<NoopRawMutex, I2cCompat<'static>>,
    duration_secs: u8,
    led_sender: Sender<'static, NoopRawMutex, LedCommand, 4>,
    voc_algo: &'static RefCell<GasIndexAlgorithm>,
) {
    info!("Starting SGP41 conditioning phase ({} s)…", duration_secs);

    // led.lock().await.set_color_rgb(30, 0, 0).ok();
    let _ = led_sender.send(LedCommand::Solid(30, 0, 0)).await;

    for i in 1..=duration_secs {
        info!("  Conditioning {}/{}", i, duration_secs);
        // 25 °C / 50 %RH dummy compensation values
        let params = prepare_temp_hum_params(25.0, 50.0);
        let mut cmd = [0u8; 8];
        cmd[0..2].copy_from_slice(&CMD_EXECUTE_CONDITIONING);
        cmd[2..8].copy_from_slice(&params);

        if bus.lock().await.write(SGP41_ADDR, &cmd).is_err() {
            warn!("    Failed to send measure command");
        }

        // led.lock().await.set_color_rgb(30, 0, 30).ok();
        let _ = led_sender.send(LedCommand::Solid(30, 0, 30)).await;

        // wait 50 ms before reading
        Timer::after(Duration::from_millis(50)).await;

        // ── read ──────────────────────────────────────────────────────────────
        let mut buf = [0u8; 3];
        if bus.lock().await.read(SGP41_ADDR, &mut buf).is_ok() {
            let voc_raw = u16::from_be_bytes([buf[0], buf[1]]);
            info!("    VOC raw: {}", voc_raw);
            let voc_index = voc_algo.borrow_mut().process(voc_raw as i32);
            info!("    VOC index: {}", voc_index);
        }

        // wait 1 s between conditioning cycles
        Timer::after(Duration::from_secs(1)).await;
    }

    let _ = led_sender.send(LedCommand::Solid(0, 30, 0)).await;

    // Signal completion.
    CONDITION_DONE.store(true, Ordering::Release);
    info!("Conditioning complete!");
}
