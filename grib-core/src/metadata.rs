//! Edition-independent field metadata.

/// Semantic forecast-time units shared across GRIB editions.
///
/// Raw unit codes are edition-specific. In particular, GRIB1 code `13` means
/// quarter-hour while GRIB2 code `13` means second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForecastTimeUnit {
    Minute,
    Hour,
    Day,
    Month,
    Year,
    Decade,
    Normal,
    Century,
    ThreeHours,
    SixHours,
    TwelveHours,
    QuarterHour,
    HalfHour,
    Second,
}

impl ForecastTimeUnit {
    pub fn from_grib1_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => Self::Minute,
            1 => Self::Hour,
            2 => Self::Day,
            3 => Self::Month,
            4 => Self::Year,
            5 => Self::Decade,
            6 => Self::Normal,
            7 => Self::Century,
            10 => Self::ThreeHours,
            11 => Self::SixHours,
            12 => Self::TwelveHours,
            13 => Self::QuarterHour,
            14 => Self::HalfHour,
            254 => Self::Second,
            _ => return None,
        })
    }

    pub fn from_grib2_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => Self::Minute,
            1 => Self::Hour,
            2 => Self::Day,
            3 => Self::Month,
            4 => Self::Year,
            5 => Self::Decade,
            6 => Self::Normal,
            7 => Self::Century,
            10 => Self::ThreeHours,
            11 => Self::SixHours,
            12 => Self::TwelveHours,
            13 => Self::Second,
            _ => return None,
        })
    }

    pub fn from_edition_and_code(edition: u8, code: u8) -> Option<Self> {
        match edition {
            1 => Self::from_grib1_code(code),
            2 => Self::from_grib2_code(code),
            _ => None,
        }
    }

    fn seconds_per_unit(self) -> Option<i64> {
        Some(match self {
            Self::Minute => 60,
            Self::Hour => 60 * 60,
            Self::Day => 24 * 60 * 60,
            Self::ThreeHours => 3 * 60 * 60,
            Self::SixHours => 6 * 60 * 60,
            Self::TwelveHours => 12 * 60 * 60,
            Self::QuarterHour => 15 * 60,
            Self::HalfHour => 30 * 60,
            Self::Second => 1,
            Self::Month | Self::Year | Self::Decade | Self::Normal | Self::Century => {
                return None;
            }
        })
    }
}

/// Common reference time representation for GRIB fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl ReferenceTime {
    /// Add a GRIB forecast lead using a semantic forecast-time unit.
    ///
    /// Returns `None` for calendar-dependent units or invalid timestamps.
    pub fn checked_add_forecast_time_unit(
        &self,
        unit: ForecastTimeUnit,
        value: u32,
    ) -> Option<Self> {
        let seconds_per_unit = unit.seconds_per_unit()?;
        let base = self.seconds_since_epoch()?;
        let delta = i64::from(value).checked_mul(seconds_per_unit)?;
        Self::from_seconds_since_epoch(base.checked_add(delta)?)
    }

    /// Add a GRIB forecast lead using raw GRIB edition and unit-code values.
    ///
    /// Returns `None` for unsupported edition/code pairs, calendar-dependent
    /// units, or invalid timestamps.
    pub fn checked_add_forecast_time_by_edition(
        &self,
        edition: u8,
        unit: u8,
        value: u32,
    ) -> Option<Self> {
        let unit = ForecastTimeUnit::from_edition_and_code(edition, unit)?;
        self.checked_add_forecast_time_unit(unit, value)
    }

    /// Add a GRIB forecast lead using raw GRIB2 Code Table 4.4 units.
    ///
    /// Returns `None` for unsupported unit codes, calendar-dependent units, or
    /// invalid timestamps.
    pub fn checked_add_forecast_time(&self, unit: u8, value: u32) -> Option<Self> {
        let unit = ForecastTimeUnit::from_grib2_code(unit)?;
        self.checked_add_forecast_time_unit(unit, value)
    }

    fn seconds_since_epoch(&self) -> Option<i64> {
        if !(1..=12).contains(&self.month)
            || self.day == 0
            || self.day > days_in_month(self.year, self.month)
            || self.hour > 23
            || self.minute > 59
            || self.second > 59
        {
            return None;
        }

        let days = days_from_civil(self.year, self.month, self.day)?;
        let seconds =
            i64::from(self.hour) * 60 * 60 + i64::from(self.minute) * 60 + i64::from(self.second);
        days.checked_mul(24 * 60 * 60)?.checked_add(seconds)
    }

    fn from_seconds_since_epoch(seconds: i64) -> Option<Self> {
        let days = seconds.div_euclid(24 * 60 * 60);
        let seconds_of_day = seconds.rem_euclid(24 * 60 * 60);
        let (year, month, day) = civil_from_days(days)?;

        Some(Self {
            year,
            month,
            day,
            hour: (seconds_of_day / (60 * 60)) as u8,
            minute: ((seconds_of_day % (60 * 60)) / 60) as u8,
            second: (seconds_of_day % 60) as u8,
        })
    }
}

/// Edition-independent parameter identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parameter {
    pub discipline: Option<u8>,
    pub category: Option<u8>,
    pub table_version: Option<u8>,
    pub number: u8,
    pub short_name: &'static str,
    pub description: &'static str,
}

impl Parameter {
    pub fn new_grib1(
        table_version: u8,
        number: u8,
        short_name: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            discipline: None,
            category: None,
            table_version: Some(table_version),
            number,
            short_name,
            description,
        }
    }

    pub fn new_grib2(
        discipline: u8,
        category: u8,
        number: u8,
        short_name: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            discipline: Some(discipline),
            category: Some(category),
            table_version: None,
            number,
            short_name,
            description,
        }
    }
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_from_civil(year: u16, month: u8, day: u8) -> Option<i64> {
    let month = i64::from(month);
    let day = i64::from(day);
    if !(1..=12).contains(&(month as u8)) {
        return None;
    }

    let year = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

fn civil_from_days(days_since_epoch: i64) -> Option<(u16, u8, u8)> {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    if !(0..=i64::from(u16::MAX)).contains(&year) {
        return None;
    }

    Some((year as u16, month as u8, day as u8))
}

#[cfg(test)]
mod tests {
    use super::{ForecastTimeUnit, ReferenceTime};

    #[test]
    fn adds_forecast_hours_across_day_boundary() {
        let valid = ReferenceTime {
            year: 2026,
            month: 3,
            day: 20,
            hour: 18,
            minute: 0,
            second: 0,
        }
        .checked_add_forecast_time(11, 2)
        .unwrap();

        assert_eq!(
            valid,
            ReferenceTime {
                year: 2026,
                month: 3,
                day: 21,
                hour: 6,
                minute: 0,
                second: 0,
            }
        );
    }

    #[test]
    fn adds_forecast_days_across_leap_day() {
        let valid = ReferenceTime {
            year: 2024,
            month: 2,
            day: 28,
            hour: 12,
            minute: 30,
            second: 0,
        }
        .checked_add_forecast_time(2, 2)
        .unwrap();

        assert_eq!(
            valid,
            ReferenceTime {
                year: 2024,
                month: 3,
                day: 1,
                hour: 12,
                minute: 30,
                second: 0,
            }
        );
    }

    #[test]
    fn rejects_unsupported_forecast_units() {
        assert!(ReferenceTime {
            year: 2026,
            month: 3,
            day: 20,
            hour: 12,
            minute: 0,
            second: 0,
        }
        .checked_add_forecast_time(3, 1)
        .is_none());
    }

    #[test]
    fn decodes_edition_specific_forecast_units() {
        assert_eq!(
            ForecastTimeUnit::from_grib1_code(13),
            Some(ForecastTimeUnit::QuarterHour)
        );
        assert_eq!(
            ForecastTimeUnit::from_grib2_code(13),
            Some(ForecastTimeUnit::Second)
        );
        assert_eq!(
            ForecastTimeUnit::from_grib1_code(254),
            Some(ForecastTimeUnit::Second)
        );
    }

    #[test]
    fn adds_grib1_quarter_hours_by_edition() {
        let valid = ReferenceTime {
            year: 2026,
            month: 3,
            day: 20,
            hour: 12,
            minute: 0,
            second: 0,
        }
        .checked_add_forecast_time_by_edition(1, 13, 2)
        .unwrap();

        assert_eq!(
            valid,
            ReferenceTime {
                year: 2026,
                month: 3,
                day: 20,
                hour: 12,
                minute: 30,
                second: 0,
            }
        );
    }

    #[test]
    fn adds_semantic_second_units() {
        let valid = ReferenceTime {
            year: 2026,
            month: 3,
            day: 20,
            hour: 12,
            minute: 0,
            second: 0,
        }
        .checked_add_forecast_time_unit(ForecastTimeUnit::Second, 30)
        .unwrap();

        assert_eq!(
            valid,
            ReferenceTime {
                year: 2026,
                month: 3,
                day: 20,
                hour: 12,
                minute: 0,
                second: 30,
            }
        );
    }
}
