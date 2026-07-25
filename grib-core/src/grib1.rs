//! GRIB Edition 1 shared metadata and section models.

use crate::binary::{decode_ibm_f32, decode_wmo_i16, decode_wmo_i24};
use crate::bit::read_bit;
use crate::data::{
    DataRepresentation, Grib1SecondOrderLayout, Grib1SecondOrderPackingParams, ScaledPackingParams,
};
use crate::error::{Error, Result};
use crate::grid::{GridDefinition, LatLonGrid};
use crate::metadata::{Parameter, ReferenceTime};
use crate::parameter;

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
            decimal_scale: decode_wmo_i16(&section_bytes[26..28]).unwrap(),
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
            0 => parse_latlon_grid(section_bytes)?,
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

        let binary_scale = decode_wmo_i16(&section_bytes[4..6]).unwrap();
        let reference_value = decode_ibm_f32(section_bytes[6..10].try_into().unwrap());
        let bits_per_value = section_bytes[10];
        if bits_per_value > 64 {
            return Err(Error::UnsupportedPackingWidth(bits_per_value));
        }
        let packing = ScaledPackingParams {
            encoded_values,
            reference_value,
            binary_scale,
            decimal_scale,
            bits_per_value,
            original_field_type: u8::from(flags & 0b0010 != 0),
        };
        let representation = if flags & 0b0100 != 0 {
            parse_second_order_packing(section_bytes, packing, data_flag & 0x0f)?
        } else {
            if flags & 0b0001 != 0 {
                return Err(Error::UnsupportedDataTemplate(1007));
            }
            DataRepresentation::SimplePacking(packing)
        };

        Ok((
            Self {
                flags,
                unused_bits: data_flag & 0x0f,
                binary_scale,
                reference_value,
                bits_per_value,
            },
            representation,
        ))
    }
}

fn parse_second_order_packing(
    section: &[u8],
    packing: ScaledPackingParams,
    unused_bits: u8,
) -> Result<DataRepresentation> {
    if section.len() < 22 {
        return Err(invalid_second_order(format!(
            "requires at least 22 bytes, got {}",
            section.len()
        )));
    }

    let n1 = usize::from(u16::from_be_bytes(section[11..13].try_into().unwrap()));
    let extended_flags = section[13];
    let n2 = usize::from(u16::from_be_bytes(section[14..16].try_into().unwrap()));
    let coded_group_count = usize::from(u16::from_be_bytes(section[16..18].try_into().unwrap()));
    let encoded_values = usize::from(u16::from_be_bytes(section[18..20].try_into().unwrap()));
    if section[20] != 0 {
        return Err(invalid_second_order(format!(
            "reserved octet 21 must be zero, got {}",
            section[20]
        )));
    }
    let group_count = coded_group_count;

    if encoded_values != packing.encoded_values {
        return Err(Error::DataLengthMismatch {
            expected: packing.encoded_values,
            actual: encoded_values,
        });
    }
    if extended_flags & 0x80 != 0 {
        return Err(invalid_second_order("reserved extended flag bit 1 is set"));
    }
    if extended_flags & 0x40 != 0 {
        return Err(invalid_second_order(
            "matrix-valued grid points are not supported",
        ));
    }
    if extended_flags & 0x0f != 0 {
        return Err(invalid_second_order(format!(
            "non-WMO extended ordering or spatial differencing flags are set: 0x{:02x}",
            extended_flags & 0x0f
        )));
    }

    let has_secondary_bitmap = extended_flags & 0x20 != 0;
    let has_variable_widths = extended_flags & 0x10 != 0;
    let layout = match (has_secondary_bitmap, has_variable_widths) {
        (false, true) => Grib1SecondOrderLayout::RowByRow,
        (true, false) => Grib1SecondOrderLayout::ConstantWidth,
        (true, true) => Grib1SecondOrderLayout::VariableWidth,
        (false, false) => {
            return Err(invalid_second_order(
                "constant-width groups require a secondary bitmap",
            ));
        }
    };

    if encoded_values > 0 && group_count == 0 {
        return Err(invalid_second_order(
            "non-empty fields require at least one group",
        ));
    }

    let width_count = match layout {
        Grib1SecondOrderLayout::ConstantWidth => 1,
        Grib1SecondOrderLayout::RowByRow | Grib1SecondOrderLayout::VariableWidth => group_count,
    };
    let descriptors_end = 21_usize
        .checked_add(width_count)
        .ok_or(Error::ArithmeticOverflow {
            operation: "computing GRIB1 second-order width descriptor boundary",
        })?;
    let secondary_bitmap_bytes = if has_secondary_bitmap {
        encoded_values.div_ceil(8)
    } else {
        0
    };
    let expected_first_order_index =
        descriptors_end
            .checked_add(secondary_bitmap_bytes)
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing GRIB1 first-order value offset",
            })?;
    let first_order_index = n1
        .checked_sub(1)
        .ok_or_else(|| invalid_second_order("N1 must be a one-based octet position"))?;
    if first_order_index != expected_first_order_index {
        return Err(invalid_second_order(format!(
            "N1 points to octet {n1}, expected {}",
            expected_first_order_index + 1
        )));
    }
    if expected_first_order_index > section.len() {
        return Err(invalid_second_order(format!(
            "width descriptors and secondary bitmap require {expected_first_order_index} bytes, got {}",
            section.len()
        )));
    }

    let width_bytes = &section[21..descriptors_end];
    if width_bytes.iter().any(|width| *width > 64) {
        let width = width_bytes
            .iter()
            .copied()
            .find(|width| *width > 64)
            .expect("out-of-range width exists");
        return Err(Error::UnsupportedPackingWidth(width));
    }
    let mut group_widths = Vec::new();
    group_widths
        .try_reserve_exact(width_count)
        .map_err(|error| Error::allocation("GRIB1 group widths", width_count, error))?;
    group_widths.extend_from_slice(width_bytes);

    let group_lengths = if has_secondary_bitmap {
        parse_secondary_group_lengths(
            &section[descriptors_end..expected_first_order_index],
            encoded_values,
            group_count,
        )?
    } else {
        Vec::new()
    };

    let first_order_bits = group_count
        .checked_mul(usize::from(packing.bits_per_value))
        .ok_or(Error::BitOffsetOverflow)?;
    let expected_second_order_index = expected_first_order_index
        .checked_add(first_order_bits.div_ceil(8))
        .ok_or(Error::ArithmeticOverflow {
            operation: "computing GRIB1 second-order value offset",
        })?;
    let second_order_index = n2
        .checked_sub(1)
        .ok_or_else(|| invalid_second_order("N2 must be a one-based octet position"))?;
    if second_order_index != expected_second_order_index {
        return Err(invalid_second_order(format!(
            "N2 points to octet {n2}, expected {}",
            expected_second_order_index + 1
        )));
    }
    if second_order_index > section.len() {
        return Err(invalid_second_order(format!(
            "first-order values require {second_order_index} bytes, got {}",
            section.len()
        )));
    }

    let first_order_values_offset = first_order_index - 11;
    let second_order_values_offset = second_order_index - 11;
    let payload_bits = section[11..]
        .len()
        .checked_mul(8)
        .ok_or(Error::BitOffsetOverflow)?;
    let usable_bits = payload_bits
        .checked_sub(usize::from(unused_bits))
        .ok_or_else(|| invalid_second_order("unused-bit count exceeds the BDS payload"))?;
    let second_order_start = second_order_values_offset
        .checked_mul(8)
        .ok_or(Error::BitOffsetOverflow)?;

    if layout != Grib1SecondOrderLayout::RowByRow {
        let second_order_bits =
            group_lengths
                .iter()
                .enumerate()
                .try_fold(0usize, |total, (group_index, length)| {
                    let width = match layout {
                        Grib1SecondOrderLayout::ConstantWidth => group_widths[0],
                        Grib1SecondOrderLayout::VariableWidth => group_widths[group_index],
                        Grib1SecondOrderLayout::RowByRow => unreachable!(),
                    };
                    length
                        .checked_mul(usize::from(width))
                        .and_then(|bits| total.checked_add(bits))
                        .ok_or(Error::BitOffsetOverflow)
                })?;
        let expected_end = second_order_start
            .checked_add(second_order_bits)
            .ok_or(Error::BitOffsetOverflow)?;
        if expected_end != usable_bits {
            return Err(invalid_second_order(format!(
                "packed values end at bit {expected_end}, but the usable BDS payload ends at bit {usable_bits}"
            )));
        }
    } else if second_order_start > usable_bits {
        return Err(invalid_second_order(
            "N2 points beyond the usable BDS payload",
        ));
    }

    Ok(DataRepresentation::Grib1SecondOrderPacking(
        Grib1SecondOrderPackingParams {
            packing,
            layout,
            group_count,
            group_widths,
            group_lengths,
            first_order_values_offset,
            second_order_values_offset,
            unused_bits,
        },
    ))
}

fn parse_secondary_group_lengths(
    bitmap: &[u8],
    encoded_values: usize,
    group_count: usize,
) -> Result<Vec<usize>> {
    if encoded_values == 0 {
        if group_count == 0 {
            return Ok(Vec::new());
        }
        return Err(invalid_second_order(format!(
            "empty secondary bitmap cannot describe {group_count} groups"
        )));
    }
    if !read_bit(bitmap, 0)? {
        return Err(invalid_second_order(
            "secondary bitmap must start with a group boundary",
        ));
    }

    let mut lengths = Vec::new();
    lengths
        .try_reserve_exact(group_count)
        .map_err(|error| Error::allocation("GRIB1 group lengths", group_count, error))?;
    let mut group_start = 0usize;
    for bit_index in 1..encoded_values {
        if read_bit(bitmap, bit_index)? {
            lengths.push(bit_index - group_start);
            group_start = bit_index;
        }
    }
    lengths.push(encoded_values - group_start);
    if lengths.len() != group_count {
        return Err(invalid_second_order(format!(
            "secondary bitmap defines {} groups, but P1 declares {group_count}",
            lengths.len()
        )));
    }
    Ok(lengths)
}

fn invalid_second_order(reason: impl Into<String>) -> Error {
    Error::InvalidSection {
        section: 4,
        reason: format!("invalid GRIB1 second-order packing: {}", reason.into()),
    }
}

fn parse_reference_time(section_bytes: &[u8]) -> Result<ReferenceTime> {
    let century = section_bytes[24];
    let year_of_century = u16::from(section_bytes[12]);
    let year = match century {
        0 => year_of_century,
        c => (u16::from(c) - 1) * 100 + year_of_century,
    };

    let reference_time = ReferenceTime {
        year,
        month: section_bytes[13],
        day: section_bytes[14],
        hour: section_bytes[15],
        minute: section_bytes[16],
        second: 0,
    };
    reference_time.validate_in_section(1)?;
    Ok(reference_time)
}

fn parse_latlon_grid(section_bytes: &[u8]) -> Result<GridDefinition> {
    let ni = u32::from(u16::from_be_bytes(section_bytes[6..8].try_into().unwrap()));
    let nj = u32::from(u16::from_be_bytes(section_bytes[8..10].try_into().unwrap()));
    let lat_first = parse_coordinate(&section_bytes[10..13], 90_000_000, "first latitude")?;
    let lon_first = parse_coordinate(&section_bytes[13..16], 360_000_000, "first longitude")?;
    let lat_last = parse_coordinate(&section_bytes[17..20], 90_000_000, "last latitude")?;
    let lon_last = parse_coordinate(&section_bytes[20..23], 360_000_000, "last longitude")?;
    let di = u32::from(u16::from_be_bytes(
        section_bytes[23..25].try_into().unwrap(),
    )) * 1_000;
    let dj = u32::from(u16::from_be_bytes(
        section_bytes[25..27].try_into().unwrap(),
    )) * 1_000;
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

fn parse_coordinate(bytes: &[u8], absolute_limit: i64, name: &str) -> Result<i32> {
    let millidegrees = decode_wmo_i24(bytes).ok_or_else(|| Error::InvalidSection {
        section: 2,
        reason: format!("truncated {name}"),
    })?;
    let microdegrees = i64::from(millidegrees) * 1_000;
    if microdegrees.abs() > absolute_limit {
        return Err(Error::InvalidSection {
            section: 2,
            reason: format!(
                "{name} {microdegrees} microdegrees is outside the valid range -{absolute_limit}..={absolute_limit}"
            ),
        });
    }

    i32::try_from(microdegrees).map_err(|_| Error::InvalidSection {
        section: 2,
        reason: format!("{name} does not fit the internal microdegree representation"),
    })
}

#[cfg(test)]
mod tests {
    use super::{BinaryDataSection, GridDescription, ProductDefinition};
    use crate::data::{DataRepresentation, Grib1SecondOrderLayout};
    use crate::error::Error;
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

    #[test]
    fn rejects_invalid_product_definition_reference_time() {
        let mut section = valid_product_definition_section();
        section[13] = 2;
        section[14] = 29;
        let err = ProductDefinition::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 1, .. }));
        assert!(err.to_string().contains("invalid reference timestamp"));

        let mut section = valid_product_definition_section();
        section[15] = 24;
        let err = ProductDefinition::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 1, .. }));

        let mut section = valid_product_definition_section();
        section[16] = 60;
        let err = ProductDefinition::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 1, .. }));
    }

    #[test]
    fn rejects_grib1_coordinates_outside_geographic_bounds_without_overflowing() {
        let mut section = valid_grid_description_section();
        section[10..13].copy_from_slice(&[0x7f, 0xff, 0xff]);

        let err = GridDescription::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 2, .. }));
        assert!(err.to_string().contains("first latitude"));

        let mut section = valid_grid_description_section();
        section[13..16].copy_from_slice(&[0xff, 0xff, 0xff]);

        let err = GridDescription::parse(&section).unwrap_err();
        assert!(matches!(err, Error::InvalidSection { section: 2, .. }));
        assert!(err.to_string().contains("first longitude"));
    }

    #[test]
    fn parses_row_by_row_second_order_packing() {
        let section = second_order_section(Grib1SecondOrderLayout::RowByRow);
        let (_, representation) = BinaryDataSection::parse(&section, 0, 6).unwrap();
        let DataRepresentation::Grib1SecondOrderPacking(params) = representation else {
            panic!("expected GRIB1 second-order packing");
        };

        assert_eq!(params.layout(), Grib1SecondOrderLayout::RowByRow);
        assert_eq!(params.group_count(), 2);
        assert_eq!(params.group_widths(), &[2, 2]);
        assert!(params.group_lengths().is_empty());
        assert_eq!(params.first_order_values_offset(), 12);
        assert_eq!(params.second_order_values_offset(), 14);
    }

    #[test]
    fn parses_secondary_bitmap_second_order_layouts() {
        for layout in [
            Grib1SecondOrderLayout::ConstantWidth,
            Grib1SecondOrderLayout::VariableWidth,
        ] {
            let section = second_order_section(layout);
            let (_, representation) = BinaryDataSection::parse(&section, 0, 6).unwrap();
            let DataRepresentation::Grib1SecondOrderPacking(params) = representation else {
                panic!("expected GRIB1 second-order packing");
            };

            assert_eq!(params.layout(), layout);
            assert_eq!(params.group_count(), 2);
            assert_eq!(params.group_lengths(), &[3, 3]);
            if layout == Grib1SecondOrderLayout::ConstantWidth {
                assert_eq!(params.group_widths(), &[2]);
            } else {
                assert_eq!(params.group_widths(), &[2, 2]);
            }
        }
    }

    #[test]
    fn rejects_second_order_secondary_bitmap_without_initial_boundary() {
        let mut section = second_order_section(Grib1SecondOrderLayout::VariableWidth);
        section[23] = 0b0001_0000;
        let error = BinaryDataSection::parse(&section, 0, 6).unwrap_err();
        assert!(matches!(error, Error::InvalidSection { section: 4, .. }));
        assert!(error
            .to_string()
            .contains("must start with a group boundary"));
    }

    #[test]
    fn rejects_nonstandard_second_order_extensions_and_inconsistent_offsets() {
        let mut section = second_order_section(Grib1SecondOrderLayout::RowByRow);
        section[20] = 1;
        let error = BinaryDataSection::parse(&section, 0, 6).unwrap_err();
        assert!(error.to_string().contains("reserved octet 21"));

        let mut section = second_order_section(Grib1SecondOrderLayout::RowByRow);
        section[13] |= 0x04;
        let error = BinaryDataSection::parse(&section, 0, 6).unwrap_err();
        assert!(error.to_string().contains("non-WMO extended ordering"));

        let mut section = second_order_section(Grib1SecondOrderLayout::VariableWidth);
        section[14..16].copy_from_slice(&28u16.to_be_bytes());
        let error = BinaryDataSection::parse(&section, 0, 6).unwrap_err();
        assert!(error.to_string().contains("N2 points to octet 28"));
    }

    #[test]
    fn preserves_simple_integer_field_type() {
        let mut section = vec![0u8; 12];
        section[..3].copy_from_slice(&[0, 0, 12]);
        section[3] = 0x20;
        section[10] = 8;
        let (_, representation) = BinaryDataSection::parse(&section, 0, 1).unwrap();
        let DataRepresentation::SimplePacking(params) = representation else {
            panic!("expected simple packing");
        };
        assert_eq!(params.original_field_type, 1);
    }

    fn valid_product_definition_section() -> Vec<u8> {
        let mut section = vec![0u8; 28];
        section[..3].copy_from_slice(&[0, 0, 28]);
        section[3] = 2;
        section[4] = 7;
        section[5] = 255;
        section[6] = 0;
        section[7] = 0b1000_0000;
        section[8] = 11;
        section[9] = 100;
        section[10..12].copy_from_slice(&850u16.to_be_bytes());
        section[12] = 26;
        section[13] = 3;
        section[14] = 20;
        section[15] = 12;
        section[16] = 0;
        section[17] = 1;
        section[18] = 0;
        section[19] = 0;
        section[20] = 0;
        section[24] = 21;
        section[25] = 0;
        section
    }

    fn valid_grid_description_section() -> Vec<u8> {
        let mut section = vec![0u8; 32];
        section[..3].copy_from_slice(&[0, 0, 32]);
        section[5] = 0;
        section[6..8].copy_from_slice(&2u16.to_be_bytes());
        section[8..10].copy_from_slice(&2u16.to_be_bytes());
        section[10..13].copy_from_slice(&[0, 0, 0]);
        section[13..16].copy_from_slice(&[0, 0, 0]);
        section[17..20].copy_from_slice(&[0, 3, 0xe8]);
        section[20..23].copy_from_slice(&[0, 3, 0xe8]);
        section[23..25].copy_from_slice(&1_000u16.to_be_bytes());
        section[25..27].copy_from_slice(&1_000u16.to_be_bytes());
        section
    }

    fn second_order_section(layout: Grib1SecondOrderLayout) -> Vec<u8> {
        let (length, extended_flags, n1, n2, widths, bitmap_offset) = match layout {
            Grib1SecondOrderLayout::RowByRow => (27usize, 0x10, 24u16, 26u16, &[2, 2][..], None),
            Grib1SecondOrderLayout::ConstantWidth => (27, 0x20, 24, 26, &[2][..], Some(22usize)),
            Grib1SecondOrderLayout::VariableWidth => (28, 0x30, 25, 27, &[2, 2][..], Some(23usize)),
        };
        let mut section = vec![0u8; length];
        section[..3].copy_from_slice(&[
            ((length >> 16) & 0xff) as u8,
            ((length >> 8) & 0xff) as u8,
            (length & 0xff) as u8,
        ]);
        section[3] = 0x44;
        section[10] = 5;
        section[11..13].copy_from_slice(&n1.to_be_bytes());
        section[13] = extended_flags;
        section[14..16].copy_from_slice(&n2.to_be_bytes());
        section[16..18].copy_from_slice(&2u16.to_be_bytes());
        section[18..20].copy_from_slice(&6u16.to_be_bytes());
        section[21..21 + widths.len()].copy_from_slice(widths);
        if let Some(offset) = bitmap_offset {
            section[offset] = 0b1001_0000;
        }
        let first_order_index = usize::from(n1) - 1;
        section[first_order_index..first_order_index + 2].copy_from_slice(&[0x55, 0x00]);
        let second_order_index = usize::from(n2) - 1;
        section[second_order_index..second_order_index + 2].copy_from_slice(&[0x18, 0xb0]);
        section
    }
}
