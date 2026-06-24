//! GRIB2 metadata carried by Sections 1 and 4.

use crate::error::{Error, Result};
use crate::metadata::ReferenceTime;
use crate::parameter;
use crate::util::{grib_i32, grib_i8};

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
    pub scale_factor: i16,
    pub scaled_value: i32,
}

impl FixedSurface {
    pub fn scaled_value_f64(&self) -> f64 {
        let factor = 10.0_f64.powi(-(self.scale_factor as i32));
        self.scaled_value as f64 * factor
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
pub enum ProductDefinitionTemplate {
    AnalysisOrForecast(AnalysisOrForecastTemplate),
    IndividualEnsembleForecast(IndividualEnsembleForecastTemplate),
    StatisticalProcess(StatisticalProcessTemplate),
    EnsembleStatisticalProcess(EnsembleStatisticalProcessTemplate),
}

/// Product Definition Template 4.0: analysis or forecast at a horizontal level.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisOrForecastTemplate {
    pub generating_process: u8,
    pub forecast_time_unit: u8,
    pub forecast_time: u32,
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

    pub fn generating_process(&self) -> Option<u8> {
        Some(self.template.base().generating_process)
    }

    pub fn forecast_time_unit(&self) -> Option<u8> {
        Some(self.template.base().forecast_time_unit)
    }

    pub fn forecast_time(&self) -> Option<u32> {
        Some(self.template.base().forecast_time)
    }

    pub fn first_surface(&self) -> Option<&FixedSurface> {
        self.template.base().first_surface.as_ref()
    }

    pub fn second_surface(&self) -> Option<&FixedSurface> {
        self.template.base().second_surface.as_ref()
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
            8 => Ok(Self::StatisticalProcess(StatisticalProcessTemplate::parse(
                section_bytes,
            )?)),
            11 => Ok(Self::EnsembleStatisticalProcess(
                EnsembleStatisticalProcessTemplate::parse(section_bytes)?,
            )),
            other => Err(Error::UnsupportedProductTemplate(other)),
        }
    }

    pub const fn number(&self) -> u16 {
        match self {
            Self::AnalysisOrForecast(_) => 0,
            Self::IndividualEnsembleForecast(_) => 1,
            Self::StatisticalProcess(_) => 8,
            Self::EnsembleStatisticalProcess(_) => 11,
        }
    }

    fn base(&self) -> &AnalysisOrForecastTemplate {
        match self {
            Self::AnalysisOrForecast(template) => template,
            Self::IndividualEnsembleForecast(template) => &template.base,
            Self::StatisticalProcess(template) => &template.base,
            Self::EnsembleStatisticalProcess(template) => &template.ensemble.base,
        }
    }

    fn end_of_overall_time_interval(&self) -> Option<ReferenceTime> {
        match self {
            Self::StatisticalProcess(template) => Some(template.end_of_overall_time_interval),
            Self::EnsembleStatisticalProcess(template) => {
                Some(template.end_of_overall_time_interval)
            }
            Self::AnalysisOrForecast(_) | Self::IndividualEnsembleForecast(_) => None,
        }
    }
}

impl AnalysisOrForecastTemplate {
    const MINIMUM_LENGTH: usize = 34;

    fn parse(section_bytes: &[u8]) -> Result<Self> {
        require_len(section_bytes, Self::MINIMUM_LENGTH, "template 4.0")?;

        Ok(Self {
            generating_process: section_bytes[11],
            forecast_time_unit: section_bytes[17],
            forecast_time: u32::from_be_bytes(section_bytes[18..22].try_into().unwrap()),
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

fn parse_surface(section_bytes: &[u8]) -> Option<FixedSurface> {
    let surface_type = section_bytes[0];
    if surface_type == 255 {
        return None;
    }

    Some(FixedSurface {
        surface_type,
        scale_factor: grib_i8(section_bytes[1]),
        scaled_value: grib_i32(&section_bytes[2..6])?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AnalysisOrForecastTemplate, Identification, ProductDefinition, ProductDefinitionTemplate,
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
        assert_eq!(product.first_surface().unwrap().scaled_value_f64(), 850.0);
        assert_eq!(
            product.template,
            ProductDefinitionTemplate::AnalysisOrForecast(AnalysisOrForecastTemplate {
                generating_process: 2,
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
    fn rejects_unsupported_product_definition_templates() {
        let mut section = vec![0u8; 34];
        section[..4].copy_from_slice(&(34u32).to_be_bytes());
        section[4] = 4;
        section[7..9].copy_from_slice(&99u16.to_be_bytes());
        section[9] = 2;
        section[10] = 3;

        let err = ProductDefinition::parse(&section).unwrap_err();
        assert!(matches!(err, Error::UnsupportedProductTemplate(99)));
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
