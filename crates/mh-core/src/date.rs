use std::fmt;

use serde::{Deserialize, Serialize};

/// A calendar date without time or timezone. Deadlines in editorial mail are
/// almost always day-granular; the timezone assumption is stored separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CalendarDate {
    year: i32,
    month: u8,
    day: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateError {
    MonthOutOfRange,
    DayOutOfRange,
}

impl fmt::Display for DateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DateError::MonthOutOfRange => f.write_str("month out of range"),
            DateError::DayOutOfRange => f.write_str("day out of range for month"),
        }
    }
}

impl std::error::Error for DateError {}

impl CalendarDate {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, DateError> {
        if !(1..=12).contains(&month) {
            return Err(DateError::MonthOutOfRange);
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(DateError::DayOutOfRange);
        }
        Ok(Self { year, month, day })
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    pub fn day(self) -> u8 {
        self.day
    }

    /// The UTC calendar date of a Unix timestamp.
    pub fn from_unix_seconds(seconds: i64) -> Self {
        Self::from_days_since_epoch(seconds.div_euclid(86_400))
    }

    pub fn add_days(self, days: i64) -> Self {
        Self::from_days_since_epoch(self.days_since_epoch() + days)
    }

    // Civil-date conversions after Howard Hinnant's `days_from_civil` / `civil_from_days`.
    pub fn days_since_epoch(self) -> i64 {
        let month = i64::from(self.month);
        let year = i64::from(self.year) - i64::from(month <= 2);
        let era = (if year >= 0 { year } else { year - 399 }) / 400;
        let year_of_era = year - era * 400;
        let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + i64::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    fn from_days_since_epoch(days: i64) -> Self {
        let z = days + 719_468;
        let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
        let day_of_era = z - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let mp = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Self {
            year: year as i32,
            month: month as u8,
            day: day as u8,
        }
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_month_lengths() {
        assert!(CalendarDate::new(2028, 2, 29).is_ok());
        assert_eq!(
            CalendarDate::new(2026, 2, 29),
            Err(DateError::DayOutOfRange)
        );
        assert_eq!(
            CalendarDate::new(2026, 4, 31),
            Err(DateError::DayOutOfRange)
        );
        assert_eq!(
            CalendarDate::new(2026, 13, 1),
            Err(DateError::MonthOutOfRange)
        );
    }

    #[test]
    fn converts_unix_time_and_adds_days() {
        assert_eq!(
            CalendarDate::from_unix_seconds(0),
            CalendarDate::new(1970, 1, 1).unwrap()
        );
        assert_eq!(
            CalendarDate::from_unix_seconds(-1),
            CalendarDate::new(1969, 12, 31).unwrap()
        );
        let start = CalendarDate::new(2026, 10, 29).unwrap();
        assert_eq!(start.add_days(14), CalendarDate::new(2026, 11, 12).unwrap());
        assert_eq!(
            CalendarDate::new(2028, 2, 28).unwrap().add_days(1),
            CalendarDate::new(2028, 2, 29).unwrap()
        );
        assert_eq!(start.add_days(-365).add_days(365), start);
    }

    #[test]
    fn displays_as_iso() {
        assert_eq!(
            CalendarDate::new(2026, 11, 2).unwrap().to_string(),
            "2026-11-02"
        );
    }
}
