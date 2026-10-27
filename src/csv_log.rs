//! Append-only CSV log of measurements.
//!
//! Each row is written and synced to disk on its own, so a power cut loses at
//! most the row being written, never earlier ones.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::scd4x::Measurement;

pub const HEADER: &str = "timestamp,co2_ppm,temperature_c,humidity_pct";

pub struct CsvLog {
    file: File,
}

impl CsvLog {
    /// Opens the log for appending, creating it with a header row if it is
    /// new or empty.
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        if file.metadata()?.len() == 0 {
            writeln!(file, "{HEADER}")?;
            file.sync_data()?;
        }
        Ok(Self { file })
    }

    pub fn append(&mut self, at: DateTime<Utc>, m: &Measurement) -> io::Result<()> {
        // One write call per row keeps rows whole in the file.
        self.file.write_all(format_row(at, m).as_bytes())?;
        self.file.sync_data()
    }
}

fn format_row(at: DateTime<Utc>, m: &Measurement) -> String {
    format!(
        "{},{},{:.2},{:.2}\n",
        at.to_rfc3339_opts(SecondsFormat::Secs, true),
        m.co2_ppm,
        m.temperature_c,
        m.humidity_pct,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> Measurement {
        Measurement {
            co2_ppm: 812,
            temperature_c: 22.456,
            humidity_pct: 41.0,
        }
    }

    #[test]
    fn row_uses_utc_rfc3339_and_two_decimals() {
        let at = Utc.with_ymd_and_hms(2026, 10, 27, 14, 15, 3).unwrap();
        assert_eq!(
            format_row(at, &sample()),
            "2026-10-27T14:15:03Z,812,22.46,41.00\n"
        );
    }

    #[test]
    fn header_is_written_once() {
        let dir = std::env::temp_dir().join(format!("climate-monitor-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("readings.csv");
        let _ = std::fs::remove_file(&path);
        let at = Utc.with_ymd_and_hms(2026, 10, 27, 14, 15, 0).unwrap();

        CsvLog::open(&path).unwrap().append(at, &sample()).unwrap();
        CsvLog::open(&path).unwrap().append(at, &sample()).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], HEADER);
        assert_eq!(lines.iter().filter(|l| **l == HEADER).count(), 1);
    }
}
