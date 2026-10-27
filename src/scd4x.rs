//! Minimal driver for the Sensirion SCD40 CO2/temperature/humidity sensor.
//!
//! Covers only what periodic logging needs. Command codes, timings and
//! conversion formulas come from the Sensirion SCD4x datasheet.

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

pub const ADDRESS: u8 = 0x62;

const START_LOW_POWER_PERIODIC_MEASUREMENT: u16 = 0x21ac;
const STOP_PERIODIC_MEASUREMENT: u16 = 0x3f86;
const GET_DATA_READY_STATUS: u16 = 0xe4b8;
const READ_MEASUREMENT: u16 = 0xec05;

const STOP_PERIODIC_MEASUREMENT_MS: u32 = 500;
const COMMAND_EXECUTION_MS: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub co2_ppm: u16,
    pub temperature_c: f32,
    pub humidity_pct: f32,
}

#[derive(Debug)]
pub enum Error<E> {
    I2c(E),
    Crc,
}

impl<E: core::fmt::Debug> core::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::I2c(e) => write!(f, "I2C error: {e:?}"),
            Error::Crc => write!(f, "CRC mismatch in sensor response"),
        }
    }
}

impl<E: core::fmt::Debug> std::error::Error for Error<E> {}

pub struct Scd40<I, D> {
    i2c: I,
    delay: D,
}

impl<I: I2c, D: DelayNs> Scd40<I, D> {
    pub fn new(i2c: I, delay: D) -> Self {
        Self { i2c, delay }
    }

    /// Stops any measurement already running, then starts low-power periodic
    /// measurement (one sample every 30 s).
    ///
    /// The sensor ignores most commands while measuring, and it keeps
    /// measuring across a restart of this process, so stop always comes first.
    pub fn start(&mut self) -> Result<(), Error<I::Error>> {
        self.stop()?;
        self.command(START_LOW_POWER_PERIODIC_MEASUREMENT)
    }

    pub fn stop(&mut self) -> Result<(), Error<I::Error>> {
        self.command(STOP_PERIODIC_MEASUREMENT)?;
        self.delay.delay_ms(STOP_PERIODIC_MEASUREMENT_MS);
        Ok(())
    }

    pub fn data_ready(&mut self) -> Result<bool, Error<I::Error>> {
        let [status] = self.read_words::<1>(GET_DATA_READY_STATUS)?;
        Ok(status & 0x07ff != 0)
    }

    /// Reads the latest sample. Only valid once `data_ready` returns true;
    /// reading also clears the ready flag.
    pub fn read_measurement(&mut self) -> Result<Measurement, Error<I::Error>> {
        let [co2, t, rh] = self.read_words::<3>(READ_MEASUREMENT)?;
        Ok(Measurement {
            co2_ppm: co2,
            temperature_c: -45.0 + 175.0 * f32::from(t) / 65535.0,
            humidity_pct: 100.0 * f32::from(rh) / 65535.0,
        })
    }

    fn command(&mut self, command: u16) -> Result<(), Error<I::Error>> {
        self.i2c
            .write(ADDRESS, &command.to_be_bytes())
            .map_err(Error::I2c)
    }

    fn read_words<const N: usize>(&mut self, command: u16) -> Result<[u16; N], Error<I::Error>> {
        self.command(command)?;
        self.delay.delay_ms(COMMAND_EXECUTION_MS);

        // Each word arrives as two big-endian bytes followed by their CRC.
        let mut buf = [0u8; 9];
        let buf = &mut buf[..N * 3];
        self.i2c.read(ADDRESS, buf).map_err(Error::I2c)?;

        let mut words = [0u16; N];
        for (word, chunk) in words.iter_mut().zip(buf.chunks_exact(3)) {
            if crc8(&chunk[..2]) != chunk[2] {
                return Err(Error::Crc);
            }
            *word = u16::from_be_bytes([chunk[0], chunk[1]]);
        }
        Ok(words)
    }
}

/// CRC-8 as specified by Sensirion: polynomial 0x31, initial value 0xff.
fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0xffu8;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal_mock::eh1::delay::NoopDelay;
    use embedded_hal_mock::eh1::i2c::{Mock, Transaction};

    #[test]
    fn crc_matches_datasheet_example() {
        assert_eq!(crc8(&[0xbe, 0xef]), 0x92);
    }

    #[test]
    fn start_stops_before_starting_low_power_mode() {
        let mut i2c = Mock::new(&[
            Transaction::write(ADDRESS, vec![0x3f, 0x86]),
            Transaction::write(ADDRESS, vec![0x21, 0xac]),
        ]);
        Scd40::new(i2c.clone(), NoopDelay).start().unwrap();
        i2c.done();
    }

    #[test]
    fn data_ready_checks_low_eleven_bits() {
        let mut i2c = Mock::new(&[
            Transaction::write(ADDRESS, vec![0xe4, 0xb8]),
            Transaction::read(ADDRESS, with_crc(&[0x8000])),
            Transaction::write(ADDRESS, vec![0xe4, 0xb8]),
            Transaction::read(ADDRESS, with_crc(&[0x0006])),
        ]);
        let mut sensor = Scd40::new(i2c.clone(), NoopDelay);
        assert!(!sensor.data_ready().unwrap());
        assert!(sensor.data_ready().unwrap());
        i2c.done();
    }

    #[test]
    fn read_measurement_converts_raw_values() {
        // Raw values from the datasheet's read_measurement example.
        let mut i2c = Mock::new(&[
            Transaction::write(ADDRESS, vec![0xec, 0x05]),
            Transaction::read(ADDRESS, with_crc(&[0x01f4, 0x6667, 0x5eb9])),
        ]);
        let m = Scd40::new(i2c.clone(), NoopDelay)
            .read_measurement()
            .unwrap();
        i2c.done();

        assert_eq!(m.co2_ppm, 500);
        assert!((m.temperature_c - 25.0).abs() < 0.01, "{}", m.temperature_c);
        assert!((m.humidity_pct - 37.0).abs() < 0.01, "{}", m.humidity_pct);
    }

    #[test]
    fn read_measurement_rejects_bad_crc() {
        let mut response = with_crc(&[0x01f4, 0x6667, 0x5eb9]);
        response[5] ^= 0xff;
        let mut i2c = Mock::new(&[
            Transaction::write(ADDRESS, vec![0xec, 0x05]),
            Transaction::read(ADDRESS, response),
        ]);
        let result = Scd40::new(i2c.clone(), NoopDelay).read_measurement();
        i2c.done();

        assert!(matches!(result, Err(Error::Crc)));
    }

    fn with_crc(words: &[u16]) -> Vec<u8> {
        words
            .iter()
            .flat_map(|w| {
                let [hi, lo] = w.to_be_bytes();
                [hi, lo, crc8(&[hi, lo])]
            })
            .collect()
    }
}
