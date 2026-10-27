//! Wall-clock alignment of log slots.

use chrono::{DateTime, Duration, DurationRound, Utc};

/// The first slot boundary strictly after `now`, where boundaries are
/// multiples of `interval` since the Unix epoch (so a 15-minute interval
/// lands on :00, :15, :30 and :45).
pub fn next_slot(now: DateTime<Utc>, interval: Duration) -> DateTime<Utc> {
    let floor = now
        .duration_trunc(interval)
        .expect("interval fits in a timestamp");
    floor + interval
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 27, h, m, s).unwrap()
    }

    #[test]
    fn rounds_up_to_the_next_quarter_hour() {
        assert_eq!(
            next_slot(at(14, 7, 31), Duration::minutes(15)),
            at(14, 15, 0)
        );
        assert_eq!(
            next_slot(at(14, 59, 59), Duration::minutes(15)),
            at(15, 0, 0)
        );
    }

    #[test]
    fn exactly_on_a_boundary_moves_to_the_following_one() {
        assert_eq!(
            next_slot(at(14, 15, 0), Duration::minutes(15)),
            at(14, 30, 0)
        );
    }
}
