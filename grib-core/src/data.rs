//! Data Representation Section (Section 5) shared model.

use crate::error::{Error, Result};
use crate::util::grib_i16;

/// Data representation template number and parameters.
#[derive(Debug, Clone, PartialEq)]
pub enum DataRepresentation {
    /// Template 5.0: Simple packing.
    SimplePacking(SimplePackingParams),
    /// Template 5.2/5.3: Complex packing with optional spatial differencing.
    ComplexPacking(ComplexPackingParams),
    /// Unsupported template.
    Unsupported(u16),
}

/// Parameters for simple packing (Template 5.0).
#[derive(Debug, Clone, PartialEq)]
pub struct SimplePackingParams {
    pub encoded_values: usize,
    pub reference_value: f32,
    pub binary_scale: i16,
    pub decimal_scale: i16,
    pub bits_per_value: u8,
    pub original_field_type: u8,
}

/// Parameters for complex packing (Templates 5.2 and 5.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexPackingParams {
    pub encoded_values: usize,
    pub reference_value: f32,
    pub binary_scale: i16,
    pub decimal_scale: i16,
    pub group_reference_bits: u8,
    pub original_field_type: u8,
    pub group_splitting_method: u8,
    pub missing_value_management: u8,
    pub primary_missing_substitute: u32,
    pub secondary_missing_substitute: u32,
    pub num_groups: usize,
    pub group_width_reference: u8,
    pub group_width_bits: u8,
    pub group_length_reference: u32,
    pub group_length_increment: u8,
    pub true_length_last_group: u32,
    pub scaled_group_length_bits: u8,
    pub spatial_differencing: Option<SpatialDifferencingParams>,
}

/// Parameters specific to template 5.3 spatial differencing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialDifferencingParams {
    pub order: u8,
    pub descriptor_octets: u8,
}

impl DataRepresentation {
    pub fn parse(section_bytes: &[u8]) -> Result<Self> {
        if section_bytes.len() < 11 {
            return Err(Error::InvalidSection {
                section: 5,
                reason: format!("expected at least 11 bytes, got {}", section_bytes.len()),
            });
        }
        if section_bytes[4] != 5 {
            return Err(Error::InvalidSection {
                section: section_bytes[4],
                reason: "not a data representation section".into(),
            });
        }

        let template = u16::from_be_bytes(section_bytes[9..11].try_into().unwrap());
        match template {
            0 => parse_simple_packing(section_bytes),
            2 => parse_complex_packing(section_bytes, false),
            3 => parse_complex_packing(section_bytes, true),
            _ => Ok(Self::Unsupported(template)),
        }
    }

    pub fn encoded_values(&self) -> Option<usize> {
        match self {
            Self::SimplePacking(params) => Some(params.encoded_values),
            Self::ComplexPacking(params) => Some(params.encoded_values),
            Self::Unsupported(_) => None,
        }
    }
}

fn parse_simple_packing(data: &[u8]) -> Result<DataRepresentation> {
    if data.len() < 21 {
        return Err(Error::InvalidSection {
            section: 5,
            reason: format!("template 5.0 requires 21 bytes, got {}", data.len()),
        });
    }

    let encoded_values = u32::from_be_bytes(data[5..9].try_into().unwrap()) as usize;
    let reference_value = f32::from_be_bytes(data[11..15].try_into().unwrap());
    let binary_scale = grib_i16(&data[15..17]).unwrap();
    let decimal_scale = grib_i16(&data[17..19]).unwrap();
    let bits_per_value = data[19];
    let original_field_type = data[20];

    Ok(DataRepresentation::SimplePacking(SimplePackingParams {
        encoded_values,
        reference_value,
        binary_scale,
        decimal_scale,
        bits_per_value,
        original_field_type,
    }))
}

fn parse_complex_packing(
    data: &[u8],
    with_spatial_differencing: bool,
) -> Result<DataRepresentation> {
    let required = if with_spatial_differencing { 49 } else { 47 };
    if data.len() < required {
        return Err(Error::InvalidSection {
            section: 5,
            reason: format!(
                "template 5.{} requires {required} bytes, got {}",
                if with_spatial_differencing { 3 } else { 2 },
                data.len()
            ),
        });
    }

    let group_splitting_method = data[21];
    if group_splitting_method != 1 {
        return Err(Error::UnsupportedGroupSplittingMethod(
            group_splitting_method,
        ));
    }

    let missing_value_management = data[22];
    if missing_value_management > 2 {
        return Err(Error::UnsupportedMissingValueManagement(
            missing_value_management,
        ));
    }

    let spatial_differencing = if with_spatial_differencing {
        let order = data[47];
        if !matches!(order, 1 | 2) {
            return Err(Error::UnsupportedSpatialDifferencingOrder(order));
        }
        Some(SpatialDifferencingParams {
            order,
            descriptor_octets: data[48],
        })
    } else {
        None
    };

    Ok(DataRepresentation::ComplexPacking(ComplexPackingParams {
        encoded_values: u32::from_be_bytes(data[5..9].try_into().unwrap()) as usize,
        reference_value: f32::from_be_bytes(data[11..15].try_into().unwrap()),
        binary_scale: grib_i16(&data[15..17]).unwrap(),
        decimal_scale: grib_i16(&data[17..19]).unwrap(),
        group_reference_bits: data[19],
        original_field_type: data[20],
        group_splitting_method,
        missing_value_management,
        primary_missing_substitute: u32::from_be_bytes(data[23..27].try_into().unwrap()),
        secondary_missing_substitute: u32::from_be_bytes(data[27..31].try_into().unwrap()),
        num_groups: u32::from_be_bytes(data[31..35].try_into().unwrap()) as usize,
        group_width_reference: data[35],
        group_width_bits: data[36],
        group_length_reference: u32::from_be_bytes(data[37..41].try_into().unwrap()),
        group_length_increment: data[41],
        true_length_last_group: u32::from_be_bytes(data[42..46].try_into().unwrap()),
        scaled_group_length_bits: data[46],
        spatial_differencing,
    }))
}

#[cfg(test)]
mod tests {
    use super::{DataRepresentation, SimplePackingParams};

    #[test]
    fn parses_simple_packing_template() {
        let mut section = vec![0u8; 21];
        section[..4].copy_from_slice(&(21u32).to_be_bytes());
        section[4] = 5;
        section[5..9].copy_from_slice(&3u32.to_be_bytes());
        section[9..11].copy_from_slice(&0u16.to_be_bytes());
        section[11..15].copy_from_slice(&10.0f32.to_be_bytes());
        section[15..17].copy_from_slice(&2i16.to_be_bytes());
        section[17..19].copy_from_slice(&1i16.to_be_bytes());
        section[19] = 8;
        section[20] = 0;

        assert_eq!(
            DataRepresentation::parse(&section).unwrap(),
            DataRepresentation::SimplePacking(SimplePackingParams {
                encoded_values: 3,
                reference_value: 10.0,
                binary_scale: 2,
                decimal_scale: 1,
                bits_per_value: 8,
                original_field_type: 0,
            })
        );
    }
}
