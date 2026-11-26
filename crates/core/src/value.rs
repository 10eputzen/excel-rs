use serde::ser::{Serialize, SerializeMap, Serializer};

const MS_PER_DAY: f64 = 86_400_000.0;
/// Excel serial number of 1970-01-01 in the 1900 date system.
const UNIX_EPOCH_SERIAL: f64 = 25_569.0;
/// A single cell value.
///
/// Dates are stored as milliseconds since the Unix epoch, interpreted as UTC
/// wall-clock time (Excel has no time zones).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Empty,
    String(String),
    Number(f64),
    Bool(bool),
    Date(f64),
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Empty => s.serialize_none(),
            Value::String(v) => s.serialize_str(v),
            Value::Number(v) if v.is_finite() => s.serialize_f64(*v),
            Value::Number(_) => s.serialize_none(),
            Value::Bool(v) => s.serialize_bool(*v),
            Value::Date(ms) => {
                let mut map = s.serialize_map(Some(1))?;
                map.serialize_entry("d", ms)?;
                map.end()
            }
        }
    }
}

/// Converts Unix milliseconds to an Excel serial date (1900 date system).
pub fn unix_ms_to_serial(ms: f64) -> f64 {
    let s = ms / MS_PER_DAY + UNIX_EPOCH_SERIAL;
    if s < 61.0 {
        s - 1.0
    } else {
        s
    }
}

/// True if the timestamp lies exactly on midnight (a date without time).
pub fn is_whole_day(ms: f64) -> bool {
    ms.rem_euclid(MS_PER_DAY) == 0.0
}

/// Parses `YYYY-MM-DD`, optionally followed by `THH:MM[:SS[.fff]]`, to Unix ms.
pub fn parse_iso_datetime(s: &str) -> Option<f64> {
    let (date, time) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let mut parts = date.splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut ms = days_from_civil(year, month, day) as f64 * MS_PER_DAY;
    if let Some(t) = time {
        let t = t.trim_end_matches('Z');
        let mut tp = t.splitn(3, ':');
        let h: f64 = tp.next()?.parse().ok()?;
        let m: f64 = tp.next().unwrap_or("0").parse().ok()?;
        let sec: f64 = tp.next().unwrap_or("0").parse().ok()?;
        if !(0.0..24.0).contains(&h) || !(0.0..60.0).contains(&m) || !(0.0..61.0).contains(&sec) {
            return None;
        }
        ms += ((h * 3600.0 + m * 60.0 + sec) * 1000.0).round();
    }
    Some(ms)
}

/// Unix milliseconds of midnight UTC on the given calendar day.
pub fn civil_to_unix_ms(y: i64, m: u32, d: u32) -> f64 {
    days_from_civil(y, m, d) as f64 * MS_PER_DAY
}

/// Days since 1970-01-01 for a proleptic Gregorian date (H. Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_serial() {
        // 2024-02-29 = serial 45351, 1900-01-01 = serial 1, 1900-03-01 = serial 61
        assert_eq!(unix_ms_to_serial(civil_to_unix_ms(2024, 2, 29)), 45351.0);
        assert_eq!(unix_ms_to_serial(civil_to_unix_ms(1900, 1, 1)), 1.0);
        assert_eq!(unix_ms_to_serial(civil_to_unix_ms(1900, 3, 1)), 61.0);
        assert_eq!(
            unix_ms_to_serial(civil_to_unix_ms(1970, 1, 1) + MS_PER_DAY / 2.0),
            25569.5
        );
    }

    #[test]
    fn iso_parsing() {
        assert_eq!(parse_iso_datetime("1970-01-02"), Some(MS_PER_DAY));
        assert_eq!(
            parse_iso_datetime("1970-01-01T12:30:00"),
            Some(45_000_000.0)
        );
        assert_eq!(parse_iso_datetime("nope"), None);
        assert_eq!(parse_iso_datetime("2024-13-01"), None);
    }

    #[test]
    fn whole_day() {
        assert!(is_whole_day(MS_PER_DAY * 3.0));
        assert!(!is_whole_day(MS_PER_DAY * 3.0 + 1.0));
        assert!(is_whole_day(-MS_PER_DAY));
    }
}
