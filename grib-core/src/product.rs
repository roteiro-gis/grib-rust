//! GRIB2 metadata carried by Sections 1 and 4.

use crate::error::{Error, Result};
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

        Ok(Self {
            center_id: u16::from_be_bytes(section_bytes[5..7].try_into().unwrap()),
            subcenter_id: u16::from_be_bytes(section_bytes[7..9].try_into().unwrap()),
            master_table_version: section_bytes[9],
            local_table_version: section_bytes[10],
            significance_of_reference_time: section_bytes[11],
            reference_year: u16::from_be_bytes(section_bytes[12..14].try_into().unwrap()),
            reference_month: section_bytes[14],
            reference_day: section_bytes[15],
            reference_hour: section_bytes[16],
            reference_minute: section_bytes[17],
            reference_second: section_bytes[18],
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
        match &self.template {
            ProductDefinitionTemplate::AnalysisOrForecast(template) => {
                Some(template.generating_process)
            }
        }
    }

    pub fn forecast_time_unit(&self) -> Option<u8> {
        match &self.template {
            ProductDefinitionTemplate::AnalysisOrForecast(template) => {
                Some(template.forecast_time_unit)
            }
        }
    }

    pub fn forecast_time(&self) -> Option<u32> {
        match &self.template {
            ProductDefinitionTemplate::AnalysisOrForecast(template) => Some(template.forecast_time),
        }
    }

    pub fn first_surface(&self) -> Option<&FixedSurface> {
        match &self.template {
            ProductDefinitionTemplate::AnalysisOrForecast(template) => {
                template.first_surface.as_ref()
            }
        }
    }

    pub fn second_surface(&self) -> Option<&FixedSurface> {
        match &self.template {
            ProductDefinitionTemplate::AnalysisOrForecast(template) => {
                template.second_surface.as_ref()
            }
        }
    }
}

impl ProductDefinitionTemplate {
    pub fn parse(template: u16, section_bytes: &[u8]) -> Result<Self> {
        match template {
            0 => Ok(Self::AnalysisOrForecast(AnalysisOrForecastTemplate::parse(
                section_bytes,
            )?)),
            other => Err(Error::UnsupportedProductTemplate(other)),
        }
    }

    pub const fn number(&self) -> u16 {
        match self {
            Self::AnalysisOrForecast(_) => 0,
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

    #[test]
    fn parses_identification_section() {
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

        let id = Identification::parse(&section).unwrap();
        assert_eq!(id.center_id, 7);
        assert_eq!(id.reference_year, 2026);
        assert_eq!(id.reference_hour, 12);
    }

    #[test]
    fn parses_product_definition_template_zero_fields() {
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
    fn rejects_unsupported_product_definition_templates() {
        let mut section = vec![0u8; 34];
        section[..4].copy_from_slice(&(34u32).to_be_bytes());
        section[4] = 4;
        section[7..9].copy_from_slice(&8u16.to_be_bytes());
        section[9] = 2;
        section[10] = 3;

        let err = ProductDefinition::parse(&section).unwrap_err();
        assert!(matches!(err, Error::UnsupportedProductTemplate(8)));
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
}
