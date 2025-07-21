#![no_std]

pub mod hal;
pub mod tasks;

// CRC calculation for SGP41
pub fn calculate_crc(data: &[u8]) -> u8 {
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
pub fn prepare_temp_hum_params(temp_celsius: f32, humidity_percent: f32) -> [u8; 6] {
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