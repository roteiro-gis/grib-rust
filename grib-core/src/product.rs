//! GRIB2 metadata carried by Sections 1 and 4.

use crate::binary::{decode_wmo_i32, decode_wmo_i8};
use crate::error::{Error, Result};
use crate::metadata::ReferenceTime;
use crate::parameter;

/// Section 1: Identification Section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identification {
    pub center_id: u16,
    pub subcenter_id: u16,
    pub master_table_version: u8,
    pub local_table_version: u8,
    pub significance_of_reference_time: u8,
    pub reference_year: u16,
    pub reference_month: u8,
    pub reference_day: u8,
    pub reference_hour: u8,
    pub reference_minute: u8,
    pub reference_second: u8,
    pub production_status: u8,
    pub processed_data_type: u8,
}

impl Identification {
    pub fn parse(section_bytes: &[u8]) -> Result<Self> {
        if section_bytes.len() < 21 {
            return Err(Error::InvalidSection {
                section: 1,
                reason: format!("expected at least 21 bytes, got {}", section_bytes.len()),
            });
        }
        if section_bytes[4] != 1 {
            return Err(Error::InvalidSection {
                section: section_bytes[4],
                reason: "not an identification section".into(),
            });
        }

        let reference_time = ReferenceTime {
            year: u16::from_be_bytes(section_bytes[12..14].try_into().unwrap()),
            month: section_bytes[14],
            day: section_bytes[15],
            hour: section_bytes[16],
            minute: section_bytes[17],
            second: section_bytes[18],
        };
        reference_time.validate_in_section(1)?;

        Ok(Self {
            center_id: u16::from_be_bytes(section_bytes[5..7].try_into().unwrap()),
            subcenter_id: u16::from_be_bytes(section_bytes[7..9].try_into().unwrap()),
            master_table_version: section_bytes[9],
            local_table_version: section_bytes[10],
            significance_of_reference_time: section_bytes[11],
            reference_year: reference_time.year,
            reference_month: reference_time.month,
            reference_day: reference_time.day,
            reference_hour: reference_time.hour,
            reference_minute: reference_time.minute,
            reference_second: reference_time.second,
            production_status: section_bytes[19],
            processed_data_type: section_bytes[20],
        })
    }
}

/// A fixed surface from Product Definition templates.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedSurface {
    pub surface_type: u8,
    /// The numeric level, or `None` when either WMO numeric component is
    /// encoded with its missing-value sentinel.
    pub value: Option<ScaledValue>,
}

/// The scale/value pair used to encode a fixed-surface numeric level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaledValue {
    pub scale_factor: i16,
    pub scaled_value: i32,
}

impl FixedSurface {
    pub const fn with_value(surface_type: u8, scale_factor: i16, scaled_value: i32) -> Self {
        Self {
            surface_type,
            value: Some(ScaledValue {
                scale_factor,
                scaled_value,
            }),
        }
    }

    pub const fn without_value(surface_type: u8) -> Self {
        Self {
            surface_type,
            value: None,
        }
    }

    pub fn scaled_value_f64(&self) -> Option<f64> {
        let value = self.value?;
        let factor = 10.0_f64.powi(-i32::from(value.scale_factor));
        Some(f64::from(value.scaled_value) * factor)
    }
}

/// Section 4: Product Definition Section.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductDefinition {
    pub parameter_category: u8,
    pub parameter_number: u8,
    pub template: ProductDefinitionTemplate,
}

/// Typed GRIB2 Product Definition templates.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ProductDefinitionTemplate {
    AnalysisOrForecast(AnalysisOrForecastTemplate),
    IndividualEnsembleForecast(IndividualEnsembleForecastTemplate),
    DerivedForecast(DerivedForecastTemplate),
    ProbabilityForecast(ProbabilityForecastTemplate),
    PercentileForecast(PercentileForecastTemplate),
    StatisticalProcess(StatisticalProcessTemplate),
    EnsembleStatisticalProcess(EnsembleStatisticalProcessTemplate),
    /// A well-framed Section 4 whose template is not interpreted by this
    /// version of the library. `raw` contains the template-specific bytes
    /// following the common parameter category and number.
    Unsupported {
        number: u16,
        raw: Vec<u8>,
    },
}

/// Product Definition Template 4.0: analysis or forecast at a horizontal level.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisOrForecastTemplate {
    pub type_of_generating_process: u8,
    pub background_generating_process_identifier: u8,
    pub generating_process_identifier: u8,
    pub hours_after_data_cutoff: Option<u16>,
    pub minutes_after_data_cutoff: Option<u8>,
    pub forecast_time_unit: u8,
    pub forecast_time: i32,
    pub first_surface: Option<FixedSurface>,
    pub second_surface: Option<FixedSurface>,
}

/// Product Definition Template 4.1: individual ensemble forecast at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct IndividualEnsembleForecastTemplate {
    pub base: AnalysisOrForecastTemplate,
    pub type_of_ensemble_forecast: u8,
    pub perturbation_number: u8,
    pub number_of_forecasts_in_ensemble: u8,
}

/// Product Definition Template 4.2: forecast derived from all ensemble members.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedForecastTemplate {
    pub base: AnalysisOrForecastTemplate,
    pub derived_forecast_type: u8,
    pub number_of_forecasts_in_ensemble: u8,
}

/// A signed decimal threshold used by probability product templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbabilityLimit {
    pub scale_factor: i16,
    pub scaled_value: i32,
}

impl ProbabilityLimit {
    pub fn value_f64(self) -> f64 {
        f64::from(self.scaled_value) * 10.0_f64.powi(-i32::from(self.scale_factor))
    }
}

/// The event whose forecast probability is encoded by templates 4.5 and 4.9.
///
/// Known WMO event types carry exactly the limit values used by their
/// definition. [`ProbabilityType::Other`] preserves reserved, locally defined,
/// and noncanonical input without imposing semantics on its limits. The writer
/// accepts `Other` only for codes without a standard typed variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProbabilityType {
    BelowLowerLimit(ProbabilityLimit),
    AboveUpperLimit(ProbabilityLimit),
    BetweenLimits {
        lower: ProbabilityLimit,
        upper: ProbabilityLimit,
    },
    AboveLowerLimit(ProbabilityLimit),
    BelowUpperLimit(ProbabilityLimit),
    EqualLowerLimit(ProbabilityLimit),
    AboveNormal,
    NearNormal,
    BelowNormal,
    CategoricalBoolean,
    Quantile,
    Missing,
    Other {
        code: u8,
        lower: Option<ProbabilityLimit>,
        upper: Option<ProbabilityLimit>,
    },
}

impl ProbabilityType {
    pub const fn code(self) -> u8 {
        match self {
            Self::BelowLowerLimit(_) => 0,
            Self::AboveUpperLimit(_) => 1,
            Self::BetweenLimits { .. } => 2,
            Self::AboveLowerLimit(_) => 3,
            Self::BelowUpperLimit(_) => 4,
            Self::EqualLowerLimit(_) => 5,
            Self::AboveNormal => 6,
            Self::NearNormal => 7,
            Self::BelowNormal => 8,
            Self::CategoricalBoolean => 9,
            Self::Quantile => 10,
            Self::Missing => 255,
            Self::Other { code, .. } => code,
        }
    }

    pub const fn lower_limit(self) -> Option<ProbabilityLimit> {
        match self {
            Self::BelowLowerLimit(limit)
            | Self::AboveLowerLimit(limit)
            | Self::EqualLowerLimit(limit) => Some(limit),
            Self::BetweenLimits { lower, .. } => Some(lower),
            Self::Other { lower, .. } => lower,
            _ => None,
        }
    }

    pub const fn upper_limit(self) -> Option<ProbabilityLimit> {
        match self {
            Self::AboveUpperLimit(limit) | Self::BelowUpperLimit(limit) => Some(limit),
            Self::BetweenLimits { upper, .. } => Some(upper),
            Self::Other { upper, .. } => upper,
            _ => None,
        }
    }

    fn from_code_and_limits(
        code: u8,
        lower: Option<ProbabilityLimit>,
        upper: Option<ProbabilityLimit>,
    ) -> Self {
        match (code, lower, upper) {
            (0, Some(limit), None) => Self::BelowLowerLimit(limit),
            (1, None, Some(limit)) => Self::AboveUpperLimit(limit),
            (2, Some(lower), Some(upper)) => Self::BetweenLimits { lower, upper },
            (3, Some(limit), None) => Self::AboveLowerLimit(limit),
            (4, None, Some(limit)) => Self::BelowUpperLimit(limit),
            (5, Some(limit), None) => Self::EqualLowerLimit(limit),
            (6, None, None) => Self::AboveNormal,
            (7, None, None) => Self::NearNormal,
            (8, None, None) => Self::BelowNormal,
            (9, None, None) => Self::CategoricalBoolean,
            (10, None, None) => Self::Quantile,
            (255, None, None) => Self::Missing,
            (code, lower, upper) => Self::Other { code, lower, upper },
        }
    }
}

/// Product Definition Template 4.5: probability forecast at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbabilityForecastTemplate {
    pub base: AnalysisOrForecastTemplate,
    pub forecast_probability_number: u8,
    pub total_number_of_forecast_probabilities: u8,
    pub probability: ProbabilityType,
}

/// Product Definition Template 4.6: percentile forecast at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct PercentileForecastTemplate {
    pub base: AnalysisOrForecastTemplate,
    pub percentile_value: u8,
}

/// Product Definition Template 4.8: statistically processed field over a time interval.
#[derive(Debug, Clone, PartialEq)]
pub struct StatisticalProcessTemplate {
    pub base: AnalysisOrForecastTemplate,
    pub end_of_overall_time_interval: ReferenceTime,
    pub number_of_missing_in_statistical_process: u32,
    pub time_ranges: Vec<StatisticalTimeRange>,
}

/// Product Definition Template 4.11: individual ensemble forecast over a time interval.
#[derive(Debug, Clone, PartialEq)]
pub struct EnsembleStatisticalProcessTemplate {
    pub ensemble: IndividualEnsembleForecastTemplate,
    pub end_of_overall_time_interval: ReferenceTime,
    pub number_of_missing_in_statistical_process: u32,
    pub time_ranges: Vec<StatisticalTimeRange>,
}

/// Statistical processing descriptor from GRIB2 Product Definition templates
/// with one or more time range specifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticalTimeRange {
    pub type_of_statistical_processing: u8,
    pub type_of_time_increment: u8,
    pub time_range_unit: u8,
    pub time_range_length: u32,
    pub time_increment_unit: u8,
    pub time_increment: u32,
}

impl ProductDefinition {
    pub fn parse(section_bytes: &[u8]) -> Result<Self> {
        if section_bytes.len() < 11 {
            return Err(Error::InvalidSection {
                section: 4,
                reason: format!("expected at least 11 bytes, got {}", section_bytes.len()),
            });
        }
        if section_bytes[4] != 4 {
            return Err(Error::InvalidSection {
                section: section_bytes[4],
                reason: "not a product definition section".into(),
            });
        }

        let template = u16::from_be_bytes(section_bytes[7..9].try_into().unwrap());
        let parameter_category = section_bytes[9];
        let parameter_number = section_bytes[10];

        Ok(Self {
            parameter_category,
            parameter_number,
            template: ProductDefinitionTemplate::parse(template, section_bytes)?,
        })
    }

    pub fn parameter_name(&self, discipline: u8) -> &'static str {
        parameter::parameter_name(discipline, self.parameter_category, self.parameter_number)
    }

    pub fn parameter_description(&self, discipline: u8) -> &'static str {
        parameter::parameter_description(discipline, self.parameter_category, self.parameter_number)
    }

    pub fn template_number(&self) -> u16 {
        self.template.number()
    }

    pub fn type_of_generating_process(&self) -> Option<u8> {
        self.template
            .base()
            .map(|base| base.type_of_generating_process)
    }

    pub fn generating_process_identifier(&self) -> Option<u8> {
        self.template
            .base()
            .map(|base| base.generating_process_identifier)
    }

    pub fn forecast_time_unit(&self) -> Option<u8> {
        self.template.base().map(|base| base.forecast_time_unit)
    }

    pub fn forecast_time(&self) -> Option<i32> {
        self.template.base().map(|base| base.forecast_time)
    }

    pub fn first_surface(&self) -> Option<&FixedSurface> {
        self.template
            .base()
            .and_then(|base| base.first_surface.as_ref())
    }

    pub fn second_surface(&self) -> Option<&FixedSurface> {
        self.template
            .base()
            .and_then(|base| base.second_surface.as_ref())
    }

    pub fn end_of_overall_time_interval(&self) -> Option<ReferenceTime> {
        self.template.end_of_overall_time_interval()
    }
}

impl ProductDefinitionTemplate {
    pub fn parse(template: u16, section_bytes: &[u8]) -> Result<Self> {
        match template {
            0 => Ok(Self::AnalysisOrForecast(AnalysisOrForecastTemplate::parse(
                section_bytes,
            )?)),
            1 => Ok(Self::IndividualEnsembleForecast(
                IndividualEnsembleForecastTemplate::parse(section_bytes)?,
            )),
            2 => Ok(Self::DerivedForecast(DerivedForecastTemplate::parse(
                section_bytes,
            )?)),
            5 => Ok(Self::ProbabilityForecast(
                ProbabilityForecastTemplate::parse(section_bytes)?,
            )),
            6 => Ok(Self::PercentileForecast(PercentileForecastTemplate::parse(
                section_bytes,
            )?)),
            8 => Ok(Self::StatisticalProcess(StatisticalProcessTemplate::parse(
                section_bytes,
            )?)),
            11 => Ok(Self::EnsembleStatisticalProcess(
                EnsembleStatisticalProcessTemplate::parse(section_bytes)?,
            )),
            number => {
                let raw_len = section_bytes.len() - 11;
                let mut raw = Vec::new();
                raw.try_reserve(raw_len).map_err(|error| {
                    Error::allocation("unsupported product-template bytes", raw_len, error)
                })?;
                raw.extend_from_slice(&section_bytes[11..]);
                Ok(Self::Unsupported { number, raw })
            }
        }
    }

    pub const fn number(&self) -> u16 {
        match self {
            Self::AnalysisOrForecast(_) => 0,
            Self::IndividualEnsembleForecast(_) => 1,
            Self::DerivedForecast(_) => 2,
            Self::ProbabilityForecast(_) => 5,
            Self::PercentileForecast(_) => 6,
            Self::StatisticalProcess(_) => 8,
            Self::EnsembleStatisticalProcess(_) => 11,
            Self::Unsupported { number, .. } => *number,
        }
    }

    fn base(&self) -> Option<&AnalysisOrForecastTemplate> {
        Some(match self {
            Self::AnalysisOrForecast(template) => template,
            Self::IndividualEnsembleForecast(template) => &template.base,
            Self::DerivedForecast(template) => &template.base,
            Self::ProbabilityForecast(template) => &template.base,
            Self::PercentileForecast(template) => &template.base,
            Self::StatisticalProcess(template) => &template.base,
            Self::EnsembleStatisticalProcess(template) => &template.ensemble.base,
            Self::Unsupported { .. } => return None,
        })
    }

    fn end_of_overall_time_interval(&self) -> Option<ReferenceTime> {
        match self {
            Self::StatisticalProcess(template) => Some(template.end_of_overall_time_interval),
            Self::EnsembleStatisticalProcess(template) => {
                Some(template.end_of_overall_time_interval)
            }
            Self::AnalysisOrForecast(_)
            | Self::IndividualEnsembleForecast(_)
            | Self::DerivedForecast(_)
            | Self::ProbabilityForecast(_)
            | Self::PercentileForecast(_) => None,
            Self::Unsupported { .. } => None,
        }
    }
}

impl AnalysisOrForecastTemplate {
    const MINIMUM_LENGTH: usize = 34;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::MINIMUM_LENGTH, "template 4.0")?;

        let minutes_after_data_cutoff = (section_bytes[16] != 0xff).then_some(section_bytes[16]);
        if minutes_after_data_cutoff.is_some_and(|minutes| minutes > 59) {
            return Err(Error::InvalidSection {
                section: 4,
                reason: "minutes after data cutoff must be at most 59".into(),
            });
        }

        Ok(Self {
            type_of_generating_process: section_bytes[11],
            background_generating_process_identifier: section_bytes[12],
            generating_process_identifier: section_bytes[13],
            hours_after_data_cutoff: (section_bytes[14..16] != [0xff; 2])
                .then(|| u16::from_be_bytes(section_bytes[14..16].try_into().unwrap())),
            minutes_after_data_cutoff,
            forecast_time_unit: section_bytes[17],
            forecast_time: decode_wmo_i32(&section_bytes[18..22]).unwrap(),
            first_surface: parse_surface(&section_bytes[22..28]),
            second_surface: parse_surface(&section_bytes[28..34]),
        })
    }
}

impl IndividualEnsembleForecastTemplate {
    const MINIMUM_LENGTH: usize = 37;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::MINIMUM_LENGTH, "template 4.1")?;

        Ok(Self {
            base: AnalysisOrForecastTemplate::parse(section_bytes)?,
            type_of_ensemble_forecast: section_bytes[34],
            perturbation_number: section_bytes[35],
            number_of_forecasts_in_ensemble: section_bytes[36],
        })
    }
}

impl DerivedForecastTemplate {
    const MINIMUM_LENGTH: usize = 36;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::MINIMUM_LENGTH, "template 4.2")?;

        Ok(Self {
            base: AnalysisOrForecastTemplate::parse(section_bytes)?,
            derived_forecast_type: section_bytes[34],
            number_of_forecasts_in_ensemble: section_bytes[35],
        })
    }
}

impl ProbabilityForecastTemplate {
    const MINIMUM_LENGTH: usize = 47;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::MINIMUM_LENGTH, "template 4.5")?;

        let lower = parse_probability_limit(&section_bytes[37..42]);
        let upper = parse_probability_limit(&section_bytes[42..47]);
        Ok(Self {
            base: AnalysisOrForecastTemplate::parse(section_bytes)?,
            forecast_probability_number: section_bytes[34],
            total_number_of_forecast_probabilities: section_bytes[35],
            probability: ProbabilityType::from_code_and_limits(section_bytes[36], lower, upper),
        })
    }
}

impl PercentileForecastTemplate {
    const MINIMUM_LENGTH: usize = 35;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::MINIMUM_LENGTH, "template 4.6")?;

        let percentile_value = section_bytes[34];
        if percentile_value > 100 {
            return Err(Error::InvalidSection {
                section: 4,
                reason: format!("template 4.6 percentile {percentile_value} exceeds 100"),
            });
        }
        Ok(Self {
            base: AnalysisOrForecastTemplate::parse(section_bytes)?,
            percentile_value,
        })
    }
}

impl StatisticalProcessTemplate {
    const TIME_RANGE_OFFSET: usize = 46;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::TIME_RANGE_OFFSET, "template 4.8")?;
        let time_range_count = section_bytes[41] as usize;
        let min_len = required_time_range_template_len(Self::TIME_RANGE_OFFSET, time_range_count)?;
        require_len(section_bytes, min_len, "template 4.8")?;

        Ok(Self {
            base: AnalysisOrForecastTemplate::parse(section_bytes)?,
            end_of_overall_time_interval: parse_reference_time(&section_bytes[34..41], 4)?,
            number_of_missing_in_statistical_process: u32::from_be_bytes(
                section_bytes[42..46].try_into().unwrap(),
            ),
            time_ranges: parse_statistical_time_ranges(
                &section_bytes[Self::TIME_RANGE_OFFSET..min_len],
                time_range_count,
            ),
        })
    }
}

impl EnsembleStatisticalProcessTemplate {
    const TIME_RANGE_OFFSET: usize = 49;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::TIME_RANGE_OFFSET, "template 4.11")?;
        let time_range_count = section_bytes[44] as usize;
        let min_len = required_time_range_template_len(Self::TIME_RANGE_OFFSET, time_range_count)?;
        require_len(section_bytes, min_len, "template 4.11")?;

        Ok(Self {
            ensemble: IndividualEnsembleForecastTemplate::parse(section_bytes)?,
            end_of_overall_time_interval: parse_reference_time(&section_bytes[37..44], 4)?,
            number_of_missing_in_statistical_process: u32::from_be_bytes(
                section_bytes[45..49].try_into().unwrap(),
            ),
            time_ranges: parse_statistical_time_ranges(
                &section_bytes[Self::TIME_RANGE_OFFSET..min_len],
                time_range_count,
            ),
        })
    }
}

fn require_len(section_bytes: &[u8], min_len: usize, context: &str) -> Result<()> {
    if section_bytes.len() < min_len {
        return Err(Error::InvalidSection {
            section: 4,
            reason: format!(
                "{context} requires at least {min_len} bytes, got {}",
                section_bytes.len()
            ),
        });
    }
    Ok(())
}

fn required_time_range_template_len(
    time_range_offset: usize,
    time_range_count: usize,
) -> Result<usize> {
    time_range_count
        .checked_mul(12)
        .and_then(|len| time_range_offset.checked_add(len))
        .ok_or_else(|| Error::InvalidSection {
            section: 4,
            reason: "statistical time range length overflow".into(),
        })
}

fn parse_reference_time(bytes: &[u8], section: u8) -> Result<ReferenceTime> {
    let reference_time = ReferenceTime {
        year: u16::from_be_bytes(bytes[0..2].try_into().unwrap()),
        month: bytes[2],
        day: bytes[3],
        hour: bytes[4],
        minute: bytes[5],
        second: bytes[6],
    };
    reference_time.validate_in_section(section)?;
    Ok(reference_time)
}

fn parse_statistical_time_ranges(
    bytes: &[u8],
    time_range_count: usize,
) -> Vec<StatisticalTimeRange> {
    bytes
        .chunks_exact(12)
        .take(time_range_count)
        .map(|range| StatisticalTimeRange {
            type_of_statistical_processing: range[0],
            type_of_time_increment: range[1],
            time_range_unit: range[2],
            time_range_length: u32::from_be_bytes(range[3..7].try_into().unwrap()),
            time_increment_unit: range[7],
            time_increment: u32::from_be_bytes(range[8..12].try_into().unwrap()),
        })
        .collect()
}

fn parse_probability_limit(bytes: &[u8]) -> Option<ProbabilityLimit> {
    if bytes[0] == 0xff || bytes[1..5] == [0xff; 4] {
        return None;
    }

    Some(ProbabilityLimit {
        scale_factor: decode_wmo_i8(bytes[0]),
        scaled_value: decode_wmo_i32(&bytes[1..5])?,
    })
}

fn parse_surface(section_bytes: &[u8]) -> Option<FixedSurface> {
    let surface_type = section_bytes[0];
    if surface_type == 255 {
        return None;
    }

    let value = if section_bytes[1] == 0xff || section_bytes[2..6] == [0xff; 4] {
        None
    } else {
        Some(ScaledValue {
            scale_factor: decode_wmo_i8(section_bytes[1]),
            scaled_value: decode_wmo_i32(&section_bytes[2..6])?,
        })
    };

    Some(FixedSurface {
        surface_type,
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AnalysisOrForecastTemplate, Identification, ProbabilityType, ProductDefinition,
        ProductDefinitionTemplate,
    };
    use crate::error::Error;
    use crate::metadata::ReferenceTime;

    #[test]
    fn parses_identification_section() {
        let section = valid_identification_section();

        let id = Identification::parse(&section).unwrap();
        assert_eq!(id.center_id, 7);
        assert_eq!(id.reference_year, 2026);
        assert_eq!(id.reference_hour, 12);
    }

    #[test]
    fn rejects_invalid_identification_reference_time() {
        let mut section = valid_identification_section();
        section[14] = 2;
        section[15] = 29;
        let err = Identification::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 1, .. }));
        assert!(err.to_string().contains("invalid reference timestamp"));

        let mut section = valid_identification_section();
        section[18] = 60;
        let err = Identification::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 1, .. }));
    }

    #[test]
    fn parses_product_definition_template_zero_fields() {
        let section = product_section_template_zero();

        let product = ProductDefinition::parse(&section).unwrap();
        assert_eq!(product.parameter_category, 2);
        assert_eq!(product.parameter_number, 3);
        assert_eq!(product.template_number(), 0);
        assert_eq!(product.forecast_time(), Some(6));
        assert_eq!(
            product.first_surface().unwrap().scaled_value_f64(),
            Some(850.0)
        );
        assert_eq!(
            product.template,
            ProductDefinitionTemplate::AnalysisOrForecast(AnalysisOrForecastTemplate {
                type_of_generating_process: 2,
                background_generating_process_identifier: 0,
                generating_process_identifier: 0,
                hours_after_data_cutoff: Some(0),
                minutes_after_data_cutoff: Some(0),
                forecast_time_unit: 1,
                forecast_time: 6,
                first_surface: product.first_surface().cloned(),
                second_surface: None,
            })
        );
    }

    #[test]
    fn parses_individual_ensemble_forecast_template() {
        let mut section = product_section_template_zero();
        section.resize(37, 0);
        section[..4].copy_from_slice(&(37u32).to_be_bytes());
        section[7..9].copy_from_slice(&1u16.to_be_bytes());
        section[34] = 1;
        section[35] = 2;
        section[36] = 20;

        let product = ProductDefinition::parse(&section).unwrap();
        assert_eq!(product.template_number(), 1);
        assert_eq!(product.forecast_time(), Some(6));
        match product.template {
            ProductDefinitionTemplate::IndividualEnsembleForecast(template) => {
                assert_eq!(template.type_of_ensemble_forecast, 1);
                assert_eq!(template.perturbation_number, 2);
                assert_eq!(template.number_of_forecasts_in_ensemble, 20);
                assert_eq!(template.base.forecast_time, 6);
            }
            other => panic!("expected template 4.1, got {other:?}"),
        }
    }

    #[test]
    fn parses_derived_forecast_template() {
        let mut section = product_section_template_zero();
        section.resize(36, 0);
        set_product_template(&mut section, 2);
        section[34] = 4;
        section[35] = 50;

        let product = ProductDefinition::parse(&section).unwrap();
        let ProductDefinitionTemplate::DerivedForecast(template) = product.template else {
            panic!("expected template 4.2");
        };
        assert_eq!(template.derived_forecast_type, 4);
        assert_eq!(template.number_of_forecasts_in_ensemble, 50);
        assert_eq!(template.base.forecast_time, 6);
    }

    #[test]
    fn parses_probability_forecast_with_typed_thresholds() {
        let mut section = product_section_template_zero();
        section.resize(47, 0xff);
        set_product_template(&mut section, 5);
        section[34] = 2;
        section[35] = 10;
        section[36] = 2;
        section[37] = 1;
        section[38..42].copy_from_slice(&crate::binary::encode_wmo_i32(-125).unwrap());
        section[42] = 1;
        section[43..47].copy_from_slice(&crate::binary::encode_wmo_i32(250).unwrap());

        let product = ProductDefinition::parse(&section).unwrap();
        let ProductDefinitionTemplate::ProbabilityForecast(template) = product.template else {
            panic!("expected template 4.5");
        };
        assert_eq!(template.forecast_probability_number, 2);
        assert_eq!(template.total_number_of_forecast_probabilities, 10);
        let ProbabilityType::BetweenLimits { lower, upper } = template.probability else {
            panic!("expected a between-limits probability");
        };
        assert_eq!(lower.value_f64(), -12.5);
        assert_eq!(upper.value_f64(), 25.0);
    }

    #[test]
    fn preserves_noncanonical_probability_limit_combinations() {
        let mut section = product_section_template_zero();
        section.resize(47, 0xff);
        set_product_template(&mut section, 5);
        section[36] = 0;

        let product = ProductDefinition::parse(&section).unwrap();
        let ProductDefinitionTemplate::ProbabilityForecast(template) = product.template else {
            panic!("expected template 4.5");
        };
        assert_eq!(
            template.probability,
            ProbabilityType::Other {
                code: 0,
                lower: None,
                upper: None,
            }
        );

        section[36] = 7;
        let product = ProductDefinition::parse(&section).unwrap();
        let ProductDefinitionTemplate::ProbabilityForecast(template) = product.template else {
            panic!("expected template 4.5");
        };
        assert_eq!(template.probability, ProbabilityType::NearNormal);
    }

    #[test]
    fn parses_percentile_forecast_and_rejects_values_above_one_hundred() {
        let mut section = product_section_template_zero();
        section.resize(35, 0);
        set_product_template(&mut section, 6);
        section[34] = 90;

        let product = ProductDefinition::parse(&section).unwrap();
        let ProductDefinitionTemplate::PercentileForecast(template) = product.template else {
            panic!("expected template 4.6");
        };
        assert_eq!(template.percentile_value, 90);

        section[34] = 101;
        assert!(matches!(
            ProductDefinition::parse(&section),
            Err(Error::InvalidSection { section: 4, .. })
        ));
    }

    #[test]
    fn parses_signed_forecast_time_and_process_metadata() {
        let mut section = product_section_template_zero();
        section[12] = 7;
        section[13] = 42;
        section[14..16].copy_from_slice(&12u16.to_be_bytes());
        section[16] = 30;
        section[18..22].copy_from_slice(&crate::binary::encode_wmo_i32(-6).unwrap());

        let product = ProductDefinition::parse(&section).unwrap();
        assert_eq!(product.type_of_generating_process(), Some(2));
        assert_eq!(product.generating_process_identifier(), Some(42));
        assert_eq!(product.forecast_time(), Some(-6));
        let ProductDefinitionTemplate::AnalysisOrForecast(template) = product.template else {
            panic!("expected template 4.0");
        };
        assert_eq!(template.background_generating_process_identifier, 7);
        assert_eq!(template.hours_after_data_cutoff, Some(12));
        assert_eq!(template.minutes_after_data_cutoff, Some(30));
    }

    #[test]
    fn parses_missing_cutoff_metadata_and_rejects_invalid_minutes() {
        let mut section = product_section_template_zero();
        section[14..16].copy_from_slice(&u16::MAX.to_be_bytes());
        section[16] = u8::MAX;
        let product = ProductDefinition::parse(&section).unwrap();
        let ProductDefinitionTemplate::AnalysisOrForecast(template) = product.template else {
            panic!("expected template 4.0");
        };
        assert_eq!(template.hours_after_data_cutoff, None);
        assert_eq!(template.minutes_after_data_cutoff, None);

        section[16] = 60;
        assert!(matches!(
            ProductDefinition::parse(&section),
            Err(Error::InvalidSection { section: 4, .. })
        ));
    }

    #[test]
    fn parses_statistical_process_template() {
        let section = product_section_template_eight();

        let product = ProductDefinition::parse(&section).unwrap();
        assert_eq!(product.template_number(), 8);
        assert_eq!(product.forecast_time(), Some(6));
        assert_eq!(
            product.end_of_overall_time_interval(),
            Some(ReferenceTime {
                year: 2026,
                month: 3,
                day: 20,
                hour: 18,
                minute: 0,
                second: 0,
            })
        );
        match product.template {
            ProductDefinitionTemplate::StatisticalProcess(template) => {
                assert_eq!(template.time_ranges.len(), 1);
                assert_eq!(template.time_ranges[0].type_of_statistical_processing, 1);
                assert_eq!(template.time_ranges[0].time_range_length, 6);
            }
            other => panic!("expected template 4.8, got {other:?}"),
        }
    }

    #[test]
    fn parses_ensemble_statistical_process_template() {
        let mut section = product_section_template_eight();
        section.resize(61, 0);
        section[..4].copy_from_slice(&(61u32).to_be_bytes());
        section[7..9].copy_from_slice(&11u16.to_be_bytes());
        section.copy_within(34..58, 37);
        section[34] = 1;
        section[35] = 3;
        section[36] = 20;

        let product = ProductDefinition::parse(&section).unwrap();
        assert_eq!(product.template_number(), 11);
        assert_eq!(
            product.end_of_overall_time_interval(),
            Some(ReferenceTime {
                year: 2026,
                month: 3,
                day: 20,
                hour: 18,
                minute: 0,
                second: 0,
            })
        );
        match product.template {
            ProductDefinitionTemplate::EnsembleStatisticalProcess(template) => {
                assert_eq!(template.ensemble.perturbation_number, 3);
                assert_eq!(template.time_ranges.len(), 1);
            }
            other => panic!("expected template 4.11, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_statistical_process_end_time() {
        let mut section = product_section_template_eight();
        section[36] = 2;
        section[37] = 29;

        let err = ProductDefinition::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 4, .. }));
        assert!(err.to_string().contains("invalid reference timestamp"));
    }

    #[test]
    fn preserves_unsupported_product_definition_templates() {
        let mut section = vec![0u8; 34];
        section[..4].copy_from_slice(&(34u32).to_be_bytes());
        section[4] = 4;
        section[7..9].copy_from_slice(&99u16.to_be_bytes());
        section[9] = 2;
        section[10] = 3;

        section[11..].copy_from_slice(&[0x5a; 23]);

        let product = ProductDefinition::parse(&section).unwrap();
        assert_eq!(product.template_number(), 99);
        assert_eq!(product.forecast_time(), None);
        assert_eq!(product.first_surface(), None);
        assert!(matches!(
            product.template,
            ProductDefinitionTemplate::Unsupported { number: 99, ref raw }
                if raw == &[0x5a; 23]
        ));
    }

    #[test]
    fn preserves_surface_type_when_numeric_level_is_missing() {
        let mut section = product_section_template_zero();
        section[23] = 0xff;
        section[24..28].copy_from_slice(&[0xff; 4]);

        let product = ProductDefinition::parse(&section).unwrap();
        let surface = product.first_surface().unwrap();
        assert_eq!(surface.surface_type, 103);
        assert_eq!(surface.value, None);
        assert_eq!(surface.scaled_value_f64(), None);
    }

    #[test]
    fn rejects_truncated_template_zero_sections() {
        let mut section = vec![0u8; 33];
        section[..4].copy_from_slice(&(33u32).to_be_bytes());
        section[4] = 4;
        section[7..9].copy_from_slice(&0u16.to_be_bytes());
        section[9] = 2;
        section[10] = 3;

        let err = ProductDefinition::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 4, .. }));
    }

    #[test]
    fn rejects_truncated_instantaneous_product_templates() {
        for (template, length) in [(2, 35), (5, 46), (6, 34)] {
            let mut section = product_section_template_zero();
            section.resize(length, 0);
            set_product_template(&mut section, template);
            assert!(matches!(
                ProductDefinition::parse(&section),
                Err(Error::InvalidSection { section: 4, .. })
            ));
        }
    }

    fn product_section_template_zero() -> Vec<u8> {
        let mut section = vec![0u8; 34];
        section[..4].copy_from_slice(&(34u32).to_be_bytes());
        section[4] = 4;
        section[7..9].copy_from_slice(&0u16.to_be_bytes());
        section[9] = 2;
        section[10] = 3;
        section[11] = 2;
        section[17] = 1;
        section[18..22].copy_from_slice(&6u32.to_be_bytes());
        section[22] = 103;
        section[23] = 0;
        section[24..28].copy_from_slice(&850u32.to_be_bytes());
        section[28] = 255;
        section
    }

    fn product_section_template_eight() -> Vec<u8> {
        let mut section = product_section_template_zero();
        section.resize(58, 0);
        section[..4].copy_from_slice(&(58u32).to_be_bytes());
        section[7..9].copy_from_slice(&8u16.to_be_bytes());
        section[34..36].copy_from_slice(&2026u16.to_be_bytes());
        section[36] = 3;
        section[37] = 20;
        section[38] = 18;
        section[39] = 0;
        section[40] = 0;
        section[41] = 1;
        section[46] = 1;
        section[47] = 2;
        section[48] = 1;
        section[49..53].copy_from_slice(&6u32.to_be_bytes());
        section[53] = 255;
        section
    }

    fn set_product_template(section: &mut [u8], template: u16) {
        let length = u32::try_from(section.len()).unwrap();
        section[..4].copy_from_slice(&length.to_be_bytes());
        section[7..9].copy_from_slice(&template.to_be_bytes());
    }

    fn valid_identification_section() -> Vec<u8> {
        let mut section = vec![0u8; 21];
        section[..4].copy_from_slice(&(21u32).to_be_bytes());
        section[4] = 1;
        section[5..7].copy_from_slice(&7u16.to_be_bytes());
        section[7..9].copy_from_slice(&14u16.to_be_bytes());
        section[9] = 35;
        section[10] = 1;
        section[11] = 1;
        section[12..14].copy_from_slice(&2026u16.to_be_bytes());
        section[14] = 3;
        section[15] = 20;
        section[16] = 12;
        section[17] = 30;
        section[18] = 45;
        section[19] = 0;
        section[20] = 1;
        section
    }
}
