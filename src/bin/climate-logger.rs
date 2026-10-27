//! Logs one SCD40 reading to a CSV file every 15 minutes, on the quarter hour.
//!
//! Configuration comes from the environment:
//! - `CLIMATE_I2C_DEVICE` (default `/dev/i2c-1`)
//! - `CLIMATE_LOG_PATH` (default `/var/lib/climate-monitor/readings.csv`)

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    logger::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("climate-logger talks to the sensor through Linux I2C and only runs on Linux");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod logger {
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration as StdDuration;

    use anyhow::{Context, Result};
    use chrono::{DateTime, Duration, Utc};
    use climate_monitor::csv_log::CsvLog;
    use climate_monitor::scd4x::{Measurement, Scd40};
    use climate_monitor::schedule::next_slot;
    use embedded_hal::delay::DelayNs;
    use embedded_hal::i2c::I2c;
    use linux_embedded_hal::{Delay, I2cdev};

    const INTERVAL: Duration = Duration::minutes(15);

    /// Low-power mode samples every 30 s; allow some slack before giving up.
    const DATA_READY_TIMEOUT: StdDuration = StdDuration::from_secs(40);

    /// The longest single sleep. Sleeping in short steps and re-reading the
    /// wall clock keeps slots on time when NTP steps the clock mid-sleep,
    /// which happens on a Pi that boots without a real-time clock.
    const MAX_SLEEP: StdDuration = StdDuration::from_secs(30);

    pub fn run() -> Result<()> {
        let device = env_or("CLIMATE_I2C_DEVICE", "/dev/i2c-1");
        let log_path = PathBuf::from(env_or(
            "CLIMATE_LOG_PATH",
            "/var/lib/climate-monitor/readings.csv",
        ));

        let mut log = CsvLog::open(&log_path)
            .with_context(|| format!("opening log {}", log_path.display()))?;
        let i2c = I2cdev::new(&device).with_context(|| format!("opening {device}"))?;
        let mut sensor = Scd40::new(i2c, Delay);
        sensor
            .start()
            .context("starting SCD40 periodic measurement")?;
        eprintln!(
            "logging from {device} to {} every {} min",
            log_path.display(),
            INTERVAL.num_minutes()
        );

        loop {
            let slot = next_slot(Utc::now(), INTERVAL);
            sleep_until(slot);

            match wait_and_read(&mut sensor) {
                // A log that cannot be written is fatal: exit and let systemd
                // restart the service rather than silently dropping readings.
                Ok(m) => log.append(Utc::now(), &m).context("appending to log")?,
                Err(e) => {
                    eprintln!("no reading for {slot}: {e:#}");
                    if let Err(e) = sensor.start() {
                        eprintln!("restarting sensor failed: {e}");
                    }
                }
            }
        }
    }

    fn wait_and_read<I, D>(sensor: &mut Scd40<I, D>) -> Result<Measurement>
    where
        I: I2c,
        I::Error: Send + Sync + 'static,
        D: DelayNs,
    {
        let deadline = std::time::Instant::now() + DATA_READY_TIMEOUT;
        while !sensor.data_ready()? {
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "sensor had no data after {DATA_READY_TIMEOUT:?}"
            );
            sleep(StdDuration::from_secs(1));
        }
        Ok(sensor.read_measurement()?)
    }

    fn sleep_until(target: DateTime<Utc>) {
        while let Ok(remaining) = (target - Utc::now()).to_std() {
            if remaining.is_zero() {
                break;
            }
            sleep(remaining.min(MAX_SLEEP));
        }
    }

    fn env_or(key: &str, default: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| default.to_owned())
    }
}
