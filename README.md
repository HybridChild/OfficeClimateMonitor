# Office Climate Monitor

A Raspberry Pi 3 Model B+ reads CO2, temperature and relative humidity from an Adafruit SCD40 breakout over I2C and appends one reading to a CSV file every 15 minutes, on the quarter hour (UTC).

## Hardware

Connect the SCD40 breakout (STEMMA QT / Qwiic, or the header pins) to the Pi's GPIO header:

| SCD40 | Pi header pin |
| ----- | ------------- |
| VIN   | 1 (3.3 V)     |
| GND   | 6 (GND)       |
| SDA   | 3 (GPIO 2)    |
| SCL   | 5 (GPIO 3)    |

The sensor answers at I2C address `0x62`.

## Log format

`/var/lib/climate-monitor/readings.csv`, one header row followed by one row per reading:

```csv
timestamp,co2_ppm,temperature_c,humidity_pct
2026-10-27T14:15:03Z,812,22.46,41.00
```

Timestamps are UTC in RFC 3339 and record when the reading was taken, a few seconds after the slot boundary. The sensor runs in low-power periodic mode, sampling every 30 s, so each row is the most recent sample at that moment. A slot whose reading fails is logged to the journal and has no row; the CSV never contains placeholder values.

## Design

The binary runs as a long-lived service rather than a job started every 15 minutes, because the SCD40 has no single-shot mode: it must measure continuously, and its automatic self-calibration relies on that continuous operation.

The crate is a library plus binaries. `scd4x` is a small driver for the subset of SCD40 commands this needs, generic over `embedded-hal` I2C so it is tested against a mock bus; `csv_log` and `schedule` are plain logic. Only the `climate-logger` binary touches Linux I2C, so `cargo test` runs on any development machine.

## Building

Build on the Pi itself. Install the C toolchain and [rustup](https://rustup.rs), then clone the repository and build:

```sh
sudo apt install -y build-essential curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"

git clone <repo-url> OfficeClimateMonitor && cd OfficeClimateMonitor
cargo build --release
```

rustup picks the Pi's native target: `aarch64-unknown-linux-gnu` on Raspberry Pi OS 64-bit, `armv7-unknown-linux-gnueabihf` on 32-bit. Use rustup rather than Debian's `rustc` package, which lags behind the toolchain the dependencies expect. If the build runs out of memory on the Pi's 1 GB, build with `cargo build --release -j1`.

## Installing on the Pi

```sh
# Enable I2C and check the sensor is visible at 0x62
sudo raspi-config nonint do_i2c 0
sudo apt install i2c-tools && i2cdetect -y 1

# Service account with access to /dev/i2c-*
sudo useradd --system --no-create-home --shell /usr/sbin/nologin --groups i2c climate

# Wait for NTP before starting, so the first rows have correct timestamps
sudo systemctl enable systemd-time-wait-sync

sudo install -m 755 target/release/climate-logger /usr/local/bin/
sudo install -m 644 deploy/climate-logger.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now climate-logger
journalctl -u climate-logger -f
```

The journal shows only the startup line and failed readings; successful readings go to the CSV alone. To watch them arrive:

```sh
tail -f /var/lib/climate-monitor/readings.csv
```

`CLIMATE_I2C_DEVICE` (default `/dev/i2c-1`) and `CLIMATE_LOG_PATH` (default `/var/lib/climate-monitor/readings.csv`) override the defaults; set them with `Environment=` in a drop-in via `sudo systemctl edit climate-logger`.

## Development

```sh
rustup target add aarch64-unknown-linux-gnu
cargo test
cargo clippy --all-targets --target aarch64-unknown-linux-gnu
```

The second command type-checks the Linux-only binary from a non-Linux machine.
