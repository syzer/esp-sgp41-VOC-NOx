#![no_std]

// ─────────────────────────────────────────────────────────────────────────────
// Simple shim that lets an `embedded-hal 1.0` I²C implementation satisfy the
// *blocking* traits from `embedded-hal 0.2` (needed by SGP41).

use embedded_hal_02::blocking::i2c::{Read, Write, WriteRead};
use esp_hal::i2c::master::I2c;

pub type HalI2c<'a> = I2c<'a, esp_hal::Blocking>;

pub struct I2cCompat<'a> {
    pub inner: &'a mut HalI2c<'a>,
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
