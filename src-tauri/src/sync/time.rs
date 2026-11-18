//! Minimal ISO-8601 conversion for the Supabase wire format.
//!
//! `chrono`/`time` would be a large dependency for two conversions, so the
//! civil-from-days algorithm (Howard Hinnant) is used directly. Both directions
//! are unit-tested against known instants.

/// Current Unix time in seconds.
#[must_use]
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Current Unix time in milliseconds.
#[must_use]
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn to_iso8601(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rest / 3_600;
    let minute = (rest % 3_600) / 60;
    let second = rest % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Parses a Supabase timestamp (`2026-11-18T08:00:00.123456+00:00`).
///
/// Sub-second precision is dropped: the protocol works in whole seconds.
#[must_use]
pub fn from_iso8601(value: &str) -> Option<i64> {
    let (date, time) = value.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    let time = time.trim_end_matches('Z');
    let time = time.split(['+', 'Z']).next()?;
    let time = time.split('-').next()?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let seconds: i64 = time_parts.next()?.split('.').next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + seconds)
}

/// Percent-encodes a query value (timestamps carry `:` and `+`).
#[must_use]
pub fn encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::{encode_query_value, from_iso8601, to_iso8601};

    #[test]
    fn epoch_round_trips() {
        for seconds in [0_i64, 1_000_000_000, 1_800_000_000, 1_795_000_000] {
            let text = to_iso8601(seconds);
            assert_eq!(from_iso8601(&text), Some(seconds), "{text}");
        }
        assert_eq!(to_iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(to_iso8601(1_800_000_000), "2027-01-15T08:00:00Z");
    }

    #[test]
    fn supabase_timestamps_are_accepted() {
        assert_eq!(
            from_iso8601("2026-11-18T08:00:00.123456+00:00"),
            Some(1_794_988_800)
        );
        assert_eq!(from_iso8601("2026-11-18T08:00:00Z"), Some(1_794_988_800));
        assert_eq!(from_iso8601("2026-13-18T08:00:00Z"), None);
        assert_eq!(from_iso8601("garbage"), None);
    }

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(
            encode_query_value("2026-11-18T08:00:00+00:00"),
            "2026-11-18T08%3A00%3A00%2B00%3A00"
        );
    }
}
