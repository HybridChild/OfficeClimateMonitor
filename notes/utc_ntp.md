# UTC, NTP and timestamps

This guide explains why `readings.csv` shows `14:15` when the office clock shows `15:15` or `16:15`, and how to turn its timestamps into local time.

## The short version

- **UTC** is one worldwide time scale with no daylight saving.
- **NTP** keeps computers' clocks in step, and it only ever deals in UTC.
- **Time zones and daylight saving** are display rules applied on top of UTC.
- The climate logger stores UTC, so conversion to local time happens when the data is read.

## UTC

Coordinated Universal Time is the reference that civil time around the world is defined against. It never jumps forward or back with the seasons, so every moment has exactly one UTC label and the labels always run in order.

Local time is UTC plus an offset. The offset depends on the place and the date, because daylight saving changes it during the year. Denmark, for example:

| Period | Zone name | Offset | `14:15` UTC is shown as |
|---|---|---|---|
| Late October to late March | CET | `+01:00` | `15:15` |
| Late March to late October | CEST | `+02:00` | `16:15` |

An offset and a time zone are different things. `+01:00` is a fixed number. `Europe/Copenhagen` is a set of rules from the [IANA time zone database](https://www.iana.org/time-zones) that gives the right offset for any date. Software should be told the zone name, because a fixed offset is wrong for part of every year.

## NTP

The Network Time Protocol lets a computer ask a server what time it is. The client measures the round trip, assumes the network delay is about equal in each direction, and corrects its clock by the difference.

An NTP packet carries only a count of seconds in UTC. It has no field for a time zone or a daylight saving rule, so no NTP server can supply them, whatever settings it has. That is why the fix for this project never involves the Windows PC.

Servers are arranged in layers called strata. A stratum 1 server is attached directly to a precise source such as GPS or an atomic clock, a stratum 2 server syncs from stratum 1, and so on. The strata measure distance from the source, and a client uses them together with the delay and dispersion the server reports to judge how far to trust it.

In this project:

```text
Windows PC (Windows Time service, NTP server on UDP 123)
    │  UTC
    ▼
Raspberry Pi (systemd-timesyncd sets the system clock)
    │  UTC
    ▼
climate-logger (reads the clock, writes UTC to the CSV)
```

The Pi 3 has no battery-backed real-time clock, so it needs NTP after every boot to know the correct time. The [README](../README.md) covers the setup and how to troubleshoot it.

Windows shows local time on screen and works out UTC from its clock and its time zone setting. If someone corrected the clock by hand while the PC had the wrong zone selected, the display can look right while the UTC it serves is off by whole hours, and the Pi inherits the error. If the CSV is off by a number of hours that daylight saving doesn't explain, check the zone setting on the PC.

## Timestamps

The log uses [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339), a strict profile of ISO 8601:

```text
2026-10-27T14:15:03Z
└── date ──┘└ time ┘└ Z means UTC, an offset of zero
```

The same moment can be written in several ways, and all of these are equal:

```text
2026-10-27T14:15:03Z         UTC
2026-10-27T15:15:03+01:00    Denmark in winter
2026-10-27T10:15:03-04:00    New York on that date
1793110503                   Unix time, seconds since 1970-01-01 00:00 UTC
```

A timestamp identifies a moment only if it carries `Z` or an offset. `2026-10-27 15:15:03` on its own is a *naive* timestamp: a reader has to guess which zone it belongs to.

### Why local time is a poor storage format

Local time breaks twice a year. In Denmark in 2026:

- **Spring, 29 March:** clocks jump from `02:00` to `03:00`, so `02:30` never happens that day.
- **Autumn, 25 October:** clocks go back from `03:00` to `02:00`, so `02:30` happens twice.

The autumn case shows the problem. These two readings are an hour apart, yet without an offset both read `02:30`:

```text
2026-10-25T00:30:03Z  →  2026-10-25 02:30:03 (+02:00)
2026-10-25T01:30:03Z  →  2026-10-25 02:30:03 (+01:00)
```

UTC has neither a gap nor a repeat, so a day of readings always has 96 slots and durations can be calculated by subtracting one timestamp from another. Storing UTC and converting for display keeps those properties, and the same data can be shown in any zone later.

Setting the Pi's time zone changes what `date` and the journal display. The CSV stays in UTC, because the logger asks for UTC explicitly.

## Converting the log

The script below reads the CSV and writes a copy with the timestamp column converted. It only needs the Python standard library, version 3.9 or newer. On Windows, also run `pip install tzdata`, because Windows doesn't provide the IANA zone rules that Python needs.

Copy the log off the Pi first:

```sh
scp <user>@<pi-address>:/var/lib/climate-monitor/readings.csv .
```

Save this as `convert_timestamps.py`:

```python
#!/usr/bin/env python3
"""Convert the timestamp column of a climate-logger CSV."""

import argparse
import csv
import sys
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

FORMATS = {
    "rfc3339": lambda t: t.isoformat(timespec="seconds"),
    "excel": lambda t: t.strftime("%Y-%m-%d %H:%M:%S"),
    "unix": lambda t: str(int(t.timestamp())),
}


def parse_utc(text):
    # datetime.fromisoformat accepts a trailing Z only from Python 3.11.
    return datetime.fromisoformat(text.replace("Z", "+00:00")).astimezone(timezone.utc)


def convert(rows, zone, render):
    header = next(rows)
    column = header.index("timestamp")
    yield header
    for row in rows:
        row[column] = render(parse_utc(row[column]).astimezone(zone))
        yield row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="CSV written by climate-logger")
    parser.add_argument("-o", "--output", help="output file (default: stdout)")
    parser.add_argument("--tz", default="UTC", help="IANA zone, e.g. Europe/Copenhagen")
    parser.add_argument("--format", choices=FORMATS, default="rfc3339")
    args = parser.parse_args()

    with open(args.input, newline="") as source:
        rows = convert(csv.reader(source), ZoneInfo(args.tz), FORMATS[args.format])
        if args.output:
            with open(args.output, "w", newline="") as target:
                csv.writer(target, lineterminator="\n").writerows(rows)
        else:
            csv.writer(sys.stdout, lineterminator="\n").writerows(rows)


if __name__ == "__main__":
    main()
```

Choose the format by what will read the output:

| `--format` | Example | Use it for |
|---|---|---|
| `rfc3339` | `2026-10-27T15:15:03+01:00` | Reading the local time while keeping it unambiguous |
| `excel` | `2026-10-27 15:15:03` | Excel, which recognises this as a date and time |
| `unix` | `1793110503` | Programs and calculations; the value is the same in every zone |

```sh
# Local time with its offset, printed to the terminal
python3 convert_timestamps.py readings.csv --tz Europe/Copenhagen

# Local time for Excel
python3 convert_timestamps.py readings.csv --tz Europe/Copenhagen --format excel -o readings_local.csv

# Unix time
python3 convert_timestamps.py readings.csv --format unix -o readings_unix.csv
```

The `excel` format drops the offset, so the repeated hour in autumn can't be told apart by its timestamps. The rows stay in the order they were logged, so keep that order rather than sorting by time across that hour.

Use the copy, and always write to a new file. Writing back over the live log on the Pi replaces the file the logger holds open, which loses every later reading, as the [log format](../README.md#log-format) section explains.
