use core::sync::atomic::Ordering;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use defmt::{error, info, warn};
use embedded_hal_02::blocking::i2c::{Read, Write};
use embassy_sync::channel::Sender;
use crate::led::LedCommand;

use crate::hal::I2cCompat;
use crate::prepare_temp_hum_params;
use crate::tasks::conditioning::{CMD_MEASURE_RAW_SIGNALS, CONDITION_DONE, SGP41_ADDR};

#[embassy_executor::task]
pub async fn sgp41_measurement_task(
    bus: &'static Mutex<NoopRawMutex, I2cCompat<'static>>,
    _led_sender: Sender<'static, NoopRawMutex, LedCommand, 4>,
) {
    // Wait until conditioning has handed over the bus.
    while !CONDITION_DONE.load(Ordering::Acquire) {
        Timer::after(Duration::from_millis(100)).await;
    }

    info!("Starting normal measurements…");

    // --- VOC/NOx index calibration constants ---
    const VOC_OFFSET: f32 = 25000.0;
    const VOC_SCALE: f32 = 50.0;   // tune so that raw≈30449 → index≈104
    const NOX_OFFSET: f32 = 25000.0;
    const NOX_SCALE: f32 = 50.0;

    loop {
        // Prepare measurement command with temperature (25 °C) and humidity (50 % RH).
        let params = prepare_temp_hum_params(25.0, 50.0);
        let mut cmd_with_params = [0u8; 8];
        cmd_with_params[0] = CMD_MEASURE_RAW_SIGNALS[0];
        cmd_with_params[1] = CMD_MEASURE_RAW_SIGNALS[1];
        cmd_with_params[2..8].copy_from_slice(&params);

        // ── write ─────────────────────────────────────────────────────────────
        if bus.lock().await.write(SGP41_ADDR, &cmd_with_params).is_err() {
            error!("Failed to send measurement command");
            Timer::after(Duration::from_secs(1)).await;
            continue;
        }

        // wait 50 ms before reading
        Timer::after(Duration::from_millis(50)).await;

        // ── read ──────────────────────────────────────────────────────────────
        let mut buffer = [0u8; 6];
        if bus.lock().await.read(SGP41_ADDR, &mut buffer).is_err() {
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

        // Determine base color from VOC level
        let mut color = if voc_index > 180.0 {
            warn!("High VOC levels detected!");
            [30, 0, 0]
        } else if voc_index > 120.0 {
            [30, 30, 0]
        } else {
            [0, 30, 0]
        };

        // Override if NOx is high
        if nox_index > 30.0 {
            warn!("High NOx levels detected!");
            color = [0, 30, 0];
        }

        // Send blink command
        _led_sender.send(LedCommand::Blink(color[0], color[1], color[2], None)).await;
        Timer::after(Duration::from_secs(2)).await;
    }
}