// Compatibility implementation for time formatting.
// - iso_from_unix_ms matches JavaScript Date#toISOString (three millisecond digits and a Z suffix).
// - local H:MM:SS matches the watch log's ja-JP toLocaleTimeString output.
/// Euclidean division that rounds negative values to the correct calendar day, like JavaScript Date.
fn div_floor(value: i64, divisor: i64) -> i64 {
    let quotient = value / divisor;
    if value % divisor < 0 {
        quotient - 1
    } else {
        quotient
    }
}

/// Converts days since 1970-01-01 to (year, month, day) using Howard Hinnant's civil_from_days.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = div_floor(z, 146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Converts Unix milliseconds to the same format as Date#toISOString.
#[cfg(test)]
fn iso_from_unix_ms(unix_ms: i64) -> String {
    let days = div_floor(unix_ms, 86_400_000);
    let ms_of_day = unix_ms - days * 86_400_000;
    let (year, month, day) = civil_from_days(days);
    let hours = ms_of_day / 3_600_000;
    let minutes = ms_of_day % 3_600_000 / 60_000;
    let seconds = ms_of_day % 60_000 / 1000;
    let millis = ms_of_day % 1000;
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_unix_milliseconds_like_to_iso_string() {
        assert_eq!(iso_from_unix_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            iso_from_unix_ms(1_785_542_400_000),
            "2026-08-01T00:00:00.000Z"
        );
        assert_eq!(
            iso_from_unix_ms(1_751_328_000_123),
            "2025-07-01T00:00:00.123Z"
        );
    }

    #[test]
    fn rounds_pre_epoch_values_to_the_correct_calendar_day() {
        assert_eq!(iso_from_unix_ms(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn handles_february_29_in_a_leap_year() {
        assert_eq!(
            iso_from_unix_ms(1_709_210_096_789),
            "2024-02-29T12:34:56.789Z"
        );
    }
}
