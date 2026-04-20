//! GRIB Edition 1 shared metadata and section models.

use crate::binary::decode_ibm_f32;
use crate::data::{DataRepresentation, SimplePackingParams};
use crate::error::{Error, Result};
use crate::grid::{GridDefinition, LatLonGrid};
use crate::metadata::{Parameter, ReferenceTime};
use crate::parameter;
use crate::util::{grib_i16, grib_i24};

/// GRIB1 product definition metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDefinition {
    pub table_version: u8,
    pub center_id: u8,
    pub generating_process_id: u8,
    pub grid_id: u8,
    pub has_grid_definition: bool,
    pub has_bitmap: bool,
    pub parameter_number: u8,
    pub level_type: u8,
    pub level_value: u16,
    pub reference_time: ReferenceTime,
    pub forecast_time_unit: u8,
    pub p1: u8,
    pub p2: u8,
    pub time_range_indicator: u8,
    pub average_count: u16,
    pub missing_count: u8,
    pub century: u8,
    pub subcenter_id: u8,
    pub decimal_scale: i16,
}

impl ProductDefinition {
    pub fn parse(section_bytes: &[u8]) -> Result<Self> {
        if section_bytes.len() < 28 {
            return Err(Error::InvalidSection {
                section: 1,
                reason: format!("expected at least 28 bytes, got {}", section_bytes.len()),
            });
        }

        Ok(Self {
            table_version: section_bytes[3],
            center_id: section_bytes[4],
            generating_process_id: section_bytes[5],
            grid_id: section_bytes[6],
            has_grid_definition: section_bytes[7] & 0b1000_0000 != 0,
            has_bitmap: section_bytes[7] & 0b0100_0000 != 0,
            parameter_number: section_bytes[8],
            level_type: section_bytes[9],
            level_value: u16::from_be_bytes(section_bytes[10..12].try_into().unwrap()),
            reference_time: parse_reference_time(section_bytes),
            forecast_time_unit: section_bytes[17],
            p1: section_bytes[18],
            p2: section_bytes[19],
            time_range_indicator: section_bytes[20],
            average_count: u16::from_be_bytes(section_bytes[21..23].try_into().unwrap()),
            missing_count: section_bytes[23],
            century: section_bytes[24],
            subcenter_id: section_bytes[25],
            decimal_scale: grib_i16(&section_bytes[26..28]).unwrap(),
        })
    }

    pub fn parameter(&self) -> Parameter {
        let short_name = parameter::grib1_parameter_name(self.table_version, self.parameter_number);
        let description =
            parameter::grib1_parameter_description(self.table_version, self.parameter_number);
        Parameter::new_grib1(
            self.table_version,
            self.parameter_number,
            short_name,
            description,
        )
    }

    pub fn forecast_time(&self) -> Option<u32> {
        match self.time_range_indicator {
            0 | 1 => Some(u32::from(self.p1)),
            10 => Some(u32::from(u16::from_be_bytes([self.p1, self.p2]))),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GridDescription {
    pub nv: u8,
    pub pv_or_pl: u8,
    pub data_representation_type: u8,
    pub grid: GridDefinition,
}

impl GridDescription {
    pub fn parse(section_bytes: &[u8]) -> Result<Self> {
        if section_bytes.len() < 32 {
            return Err(Error::InvalidSection {
                section: 2,
                reason: format!("expected at least 32 bytes, got {}", section_bytes.len()),
            });
        }

        let data_representation_type = section_bytes[5];
        let grid = match data_representation_type {
            0 => parse_latlon_grid(section_bytes),
            other => GridDefinition::Unsupported(u16::from(other)),
        };

        Ok(Self {
            nv: section_bytes[3],
            pv_or_pl: section_bytes[4],
            data_representation_type,
            grid,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryDataSection {
    pub flags: u8,
    pub unused_bits: u8,
    pub binary_scale: i16,
    pub reference_value: f32,
    pub bits_per_value: u8,
}

impl BinaryDataSection {
    pub fn parse(
        section_bytes: &[u8],
        decimal_scale: i16,
        encoded_values: usize,
    ) -> Result<(Self, DataRepresentation)> {
        if section_bytes.len() < 11 {
            return Err(Error::InvalidSection {
                section: 4,
                reason: format!("expected at least 11 bytes, got {}", section_bytes.len()),
            });
        }

        let data_flag = section_bytes[3];
        let flags = data_flag >> 4;
        if flags & 0b1000 != 0 {
            return Err(Error::UnsupportedDataTemplate(1004));
        }
        if flags & 0b0100 != 0 {
            return Err(Error::UnsupportedDataTemplate(1005));
        }
        if flags & 0b0010 != 0 {
            return Err(Error::UnsupportedDataTemplate(1006));
        }
        if flags & 0b0001 != 0 {
            return Err(Error::UnsupportedDataTemplate(1007));
        }

        let binary_scale = grib_i16(&section_bytes[4..6]).unwrap();
        let reference_value = decode_ibm_f32(section_bytes[6..10].try_into().unwrap());
        let bits_per_value = section_bytes[10];
        let simple = SimplePackingParams {
            encoded_values,
            reference_value,
            binary_scale,
            decimal_scale,
            bits_per_value,
            original_field_type: if flags & 0b0010_0000 != 0 { 1 } else { 0 },
        };

        Ok((
            Self {
                flags,
                unused_bits: data_flag & 0x0f,
                binary_scale,
                reference_value,
                bits_per_value,
            },
            DataRepresentation::SimplePacking(simple),
        ))
    }
}

fn parse_reference_time(section_bytes: &[u8]) -> ReferenceTime {
    let century = section_bytes[24];
    let year_of_century = u16::from(section_bytes[12]);
    let year = match century {
        0 => year_of_century,
        c => (u16::from(c) - 1) * 100 + year_of_century,
    };

    ReferenceTime {
        year,
        month: section_bytes[13],
        day: section_bytes[14],
        hour: section_bytes[15],
        minute: section_bytes[16],
        second: 0,
    }
}

fn parse_latlon_grid(section_bytes: &[u8]) -> GridDefinition {
    let ni = u32::from(u16::from_be_bytes(section_bytes[6..8].try_into().unwrap()));
    let nj = u32::from(u16::from_be_bytes(section_bytes[8..10].try_into().unwrap()));
    let lat_first = grib_i24(&section_bytes[10..13]).unwrap() * 1_000;
    let lon_first = grib_i24(&section_bytes[13..16]).unwrap() * 1_000;
    let lat_last = grib_i24(&section_bytes[17..20]).unwrap() * 1_000;
    let lon_last = grib_i24(&section_bytes[20..23]).unwrap() * 1_000;
    let di = u32::from(u16::from_be_bytes(
        section_bytes[23..25].try_into().unwrap(),
    )) * 1_000;
    let dj = u32::from(u16::from_be_bytes(
        section_bytes[25..27].try_into().unwrap(),
    )) * 1_000;
    let scanning_mode = section_bytes[27];

    GridDefinition::LatLon(LatLonGrid {
        ni,
        nj,
        lat_first,
        lon_first,
        lat_last,
        lon_last,
        di,
        dj,
        scanning_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::ProductDefinition;
    use crate::metadata::ReferenceTime;

    #[test]
    fn decodes_indicator_ten_forecast_time_as_u16() {
        let product = ProductDefinition {
            table_version: 2,
            center_id: 7,
            generating_process_id: 255,
            grid_id: 0,
            has_grid_definition: true,
            has_bitmap: false,
            parameter_number: 11,
            level_type: 100,
            level_value: 850,
            reference_time: ReferenceTime {
                year: 2026,
                month: 3,
                day: 20,
                hour: 12,
                minute: 0,
                second: 0,
            },
            forecast_time_unit: 1,
            p1: 0x01,
            p2: 0x2c,
            time_range_indicator: 10,
            average_count: 0,
            missing_count: 0,
            century: 21,
            subcenter_id: 0,
            decimal_scale: 0,
        };

        assert_eq!(product.forecast_time(), Some(300));
    }
}
