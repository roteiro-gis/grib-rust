//! GRIB Edition 1 parsing.

use crate::data::{decode_field, DataRepresentation, SimplePackingParams};
use crate::error::{Error, Result};
use crate::grid::{GridDefinition, LatLonGrid};
use crate::metadata::{Parameter, ReferenceTime};
use crate::parameter;
use crate::sections::SectionRef;
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
            reference_time: parse_reference_time(section_bytes)?,
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
            0 | 1 | 10 => Some(self.p1 as u32),
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
            0 => parse_latlon_grid(section_bytes)?,
            other => GridDefinition::Unsupported(other as u16),
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
        let reference_value = ibm_f32(section_bytes[6..10].try_into().unwrap());
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

pub fn bitmap_payload(section_bytes: &[u8]) -> Result<Option<&[u8]>> {
    if section_bytes.len() < 6 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("expected at least 6 bytes, got {}", section_bytes.len()),
        });
    }
    let indicator = u16::from_be_bytes(section_bytes[4..6].try_into().unwrap());
    if indicator == 0 {
        Ok(Some(&section_bytes[6..]))
    } else {
        Err(Error::UnsupportedBitmapIndicator(u8::MAX))
    }
}

pub fn decode_simple_field(
    data_section: &[u8],
    representation: &DataRepresentation,
    bitmap_section: Option<&[u8]>,
    num_grid_points: usize,
) -> Result<Vec<f64>> {
    let mut wrapped = Vec::with_capacity(data_section.len() + 1);
    wrapped.extend_from_slice(&[0, 0, 0, 0, 7]);
    wrapped.extend_from_slice(data_section);
    decode_field(&wrapped, representation, bitmap_section, num_grid_points)
}

pub fn parse_message_sections(message_bytes: &[u8]) -> Result<Grib1Sections> {
    if message_bytes.len() < 8 + 28 + 11 + 4 {
        return Err(Error::InvalidMessage(format!(
            "GRIB1 message too short: {} bytes",
            message_bytes.len()
        )));
    }

    let payload_limit = message_bytes.len() - 4;
    let pds = parse_section(message_bytes, 8, 1, payload_limit)?;
    let pds_bytes = &message_bytes[pds.offset..pds.offset + pds.length];
    let product = ProductDefinition::parse(pds_bytes)?;

    let mut cursor = pds.offset + pds.length;
    let grid = if product.has_grid_definition {
        let section_ref = parse_section(message_bytes, cursor, 2, payload_limit)?;
        cursor += section_ref.length;
        Some(section_ref)
    } else {
        None
    };

    let bitmap = if product.has_bitmap {
        let section_ref = parse_section(message_bytes, cursor, 3, payload_limit)?;
        cursor += section_ref.length;
        Some(section_ref)
    } else {
        None
    };

    let data = parse_section(message_bytes, cursor, 4, payload_limit)?;
    if data.offset + data.length != payload_limit {
        return Err(Error::InvalidMessage(
            "GRIB1 message contains trailing bytes before end marker".into(),
        ));
    }

    Ok(Grib1Sections {
        product,
        pds,
        grid,
        bitmap,
        data,
    })
}

#[derive(Debug, Clone)]
pub struct Grib1Sections {
    pub product: ProductDefinition,
    pub pds: SectionRef,
    pub grid: Option<SectionRef>,
    pub bitmap: Option<SectionRef>,
    pub data: SectionRef,
}

fn parse_reference_time(section_bytes: &[u8]) -> Result<ReferenceTime> {
    let century = section_bytes[24];
    let year_of_century = section_bytes[12] as u16;
    let year = match century {
        0 => year_of_century,
        c => (c as u16 - 1) * 100 + year_of_century,
    };

    Ok(ReferenceTime {
        year,
        month: section_bytes[13],
        day: section_bytes[14],
        hour: section_bytes[15],
        minute: section_bytes[16],
        second: 0,
    })
}

fn parse_latlon_grid(section_bytes: &[u8]) -> Result<GridDefinition> {
    let ni = u16::from_be_bytes(section_bytes[6..8].try_into().unwrap()) as u32;
    let nj = u16::from_be_bytes(section_bytes[8..10].try_into().unwrap()) as u32;
    let lat_first = grib_i24(&section_bytes[10..13]).unwrap() * 1_000;
    let lon_first = grib_i24(&section_bytes[13..16]).unwrap() * 1_000;
    let lat_last = grib_i24(&section_bytes[17..20]).unwrap() * 1_000;
    let lon_last = grib_i24(&section_bytes[20..23]).unwrap() * 1_000;
    let di = u16::from_be_bytes(section_bytes[23..25].try_into().unwrap()) as u32 * 1_000;
    let dj = u16::from_be_bytes(section_bytes[25..27].try_into().unwrap()) as u32 * 1_000;
    let scanning_mode = section_bytes[27];

    Ok(GridDefinition::LatLon(LatLonGrid {
        ni,
        nj,
        lat_first,
        lon_first,
        lat_last,
        lon_last,
        di,
        dj,
        scanning_mode,
    }))
}

fn read_u24(bytes: &[u8]) -> u32 {
    ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32)
}

fn parse_section(
    message_bytes: &[u8],
    offset: usize,
    number: u8,
    payload_limit: usize,
) -> Result<SectionRef> {
    let length_bytes = message_bytes
        .get(offset..offset + 3)
        .ok_or(Error::Truncated { offset: offset as u64 })?;
    let length = read_u24(length_bytes) as usize;
    if length < 3 {
        return Err(Error::InvalidSection {
            section: number,
            reason: format!("section length {length} is smaller than the 3-byte header"),
        });
    }

    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::InvalidMessage("GRIB1 section length overflow".into()))?;
    if end > payload_limit {
        return Err(Error::Truncated { offset: offset as u64 });
    }

    Ok(section(number, offset, length))
}

fn section(number: u8, offset: usize, length: usize) -> SectionRef {
    SectionRef {
        number,
        offset,
        length,
    }
}

fn ibm_f32(bytes: [u8; 4]) -> f32 {
    if bytes == [0, 0, 0, 0] {
        return 0.0;
    }

    let sign = if bytes[0] & 0x80 == 0 { 1.0 } else { -1.0 };
    let exponent = ((bytes[0] & 0x7f) as i32) - 64;
    let mantissa = ((bytes[1] as u32) << 16) | ((bytes[2] as u32) << 8) | (bytes[3] as u32);
    let value = sign * (mantissa as f64) / 16_777_216.0 * 16f64.powi(exponent);
    value as f32
}

#[cfg(test)]
mod tests {
    use super::{ibm_f32, parse_message_sections};

    #[test]
    fn decodes_zero_ibm_float() {
        assert_eq!(ibm_f32([0, 0, 0, 0]), 0.0);
    }

    #[test]
    fn parses_minimal_section_layout() {
        let mut message = Vec::new();
        message.extend_from_slice(b"GRIB");
        message.extend_from_slice(&[0, 0, 64, 1]);
        let mut pds = vec![0u8; 28];
        pds[..3].copy_from_slice(&[0, 0, 28]);
        pds[7] = 0b1000_0000;
        pds[24] = 21;
        message.extend_from_slice(&pds);
        let mut gds = vec![0u8; 32];
        gds[..3].copy_from_slice(&[0, 0, 32]);
        message.extend_from_slice(&gds);
        let mut bds = vec![0u8; 12];
        bds[..3].copy_from_slice(&[0, 0, 12]);
        message.extend_from_slice(&bds);
        message.extend_from_slice(b"7777");

        let sections = parse_message_sections(&message).unwrap();
        assert!(sections.grid.is_some());
        assert!(sections.bitmap.is_none());
        assert_eq!(sections.data.length, 12);
    }

    #[test]
    fn rejects_section_length_beyond_message_boundary() {
        let mut message = Vec::new();
        message.extend_from_slice(b"GRIB");
        message.extend_from_slice(&[0, 0, 64, 1]);
        let mut pds = vec![0u8; 28];
        pds[..3].copy_from_slice(&[0, 0, 28]);
        pds[7] = 0b1000_0000;
        pds[24] = 21;
        message.extend_from_slice(&pds);
        let mut gds = vec![0u8; 32];
        gds[..3].copy_from_slice(&[0, 0, 250]);
        message.extend_from_slice(&gds);
        let mut bds = vec![0u8; 12];
        bds[..3].copy_from_slice(&[0, 0, 12]);
        message.extend_from_slice(&bds);
        message.extend_from_slice(b"7777");

        assert!(parse_message_sections(&message).is_err());
    }
}
