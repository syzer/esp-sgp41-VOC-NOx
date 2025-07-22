use defmt::info;
use esp_hal::rmt::{Channel as RmtChannel};
use esp_hal::Blocking;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::channel::Receiver;


use crate::led::Led;
use crate::led::LedCommand;

#[embassy_executor::task]
pub async fn led_task(
    led_receiver: Receiver<'static, NoopRawMutex, LedCommand, 4>,
    led: &'static Mutex<NoopRawMutex, Led<RmtChannel<Blocking, 0>>>,
) {
    loop {
        // Wait for a command from the channel
        let command = led_receiver.receive().await;
        match command {
            LedCommand::Solid(r, g, b) => {
                info!("Setting LED to solid color: R={}, G={}, B={}", r, g, b);
                led.lock().await.set_color_rgb(r, g, b).ok();
            }
            LedCommand::Blink(r, g, b, period_ms) => {
                info!("Setting LED to blink: R={}, G={}, B={}, Period={}ms", r, g, b, period_ms);
                // led.lock().await.blink(r, g, b, period_ms).ok();
            }
        }
    }
}