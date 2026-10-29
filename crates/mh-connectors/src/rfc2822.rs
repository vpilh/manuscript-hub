use mh_core::CalendarDate;

/// Parses an RFC 2822 `Date:` header (`Thu, 29 Oct 2026 12:00:00 +0100`) or an
/// IMAP `INTERNALDATE` (`29-Oct-2026 12:00:00 +0100`) into Unix seconds.
pub fn parse_date_time(value: &str) -> Option<i64> {
    let mut words: Vec<&str> = value.split_whitespace().collect();
    if words.first()?.ends_with(',') {
        words.remove(0);
    }
    // Expand the IMAP `29-Oct-2026` form into three words.
    if words.first()?.contains('-') {
        let mut expanded: Vec<&str> = words.remove(0).split('-').collect();
        expanded.extend(words);
        words = expanded;
    }
    let [day, month, year, clock, rest @ ..] = words.as_slice() else {
        return None;
    };

    let day: u8 = day.parse().ok()?;
    let month = month_number(month)?;
    let year: i32 = year.parse().ok()?;
    let mut clock = clock.split(':');
    let hours: i64 = clock.next()?.parse().ok()?;
    let minutes: i64 = clock.next()?.parse().ok()?;
    let seconds: i64 = clock.next().map_or(Some(0), |s| s.parse().ok())?;
    let offset = rest
        .first()
        .map_or(Some(0), |zone| zone_offset_seconds(zone))?;

    let days = CalendarDate::new(year, month, day).ok()?.days_since_epoch();
    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds - offset)
}

fn month_number(name: &str) -> Option<u8> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = name.to_ascii_lowercase();
    let index = MONTHS.iter().position(|m| *m == lower)?;
    Some(index as u8 + 1)
}

/// `+0130` → 5400. Obsolete zone names such as `GMT` or `UT` are treated as UTC.
fn zone_offset_seconds(zone: &str) -> Option<i64> {
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return Some(0),
    };
    let digits = &zone[1..];
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = digits[2..].parse().ok()?;
    Some(sign * (hours * 3_600 + minutes * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_internaldate_forms() {
        // 2026-10-29T12:00:00Z
        let utc = 1_793_275_200;
        assert_eq!(
            parse_date_time("Thu, 29 Oct 2026 12:00:00 +0000"),
            Some(utc)
        );
        assert_eq!(parse_date_time("29 Oct 2026 13:00:00 +0100"), Some(utc));
        assert_eq!(parse_date_time("29-Oct-2026 07:00:00 -0500"), Some(utc));
        assert_eq!(parse_date_time("Thu, 29 Oct 2026 12:00 GMT"), Some(utc));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_date_time("yesterday"), None);
        assert_eq!(parse_date_time("31 Feb 2026 12:00:00 +0000"), None);
    }
}
