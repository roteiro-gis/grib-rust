//! Data Representation Section (Section 5) and Data Section (Section 7) decoding.

use crate::error::{Error, Result};
use crate::util::grib_i16;

/// Data representation template number and parameters.
#[derive(Debug, Clone, PartialEq)]
pub enum DataRepresentation {
    /// Template 5.0: Simple packing.
    SimplePacking(SimplePackingParams),
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
            _ => Ok(Self::Unsupported(template)),
        }
    }

    pub fn encoded_values(&self) -> Option<usize> {
        match self {
            Self::SimplePacking(params) => Some(params.encoded_values),
            Self::Unsupported(_) => None,
        }
    }
}

/// Decode Section 7 payload into field values, applying Section 6 bitmap when present.
pub fn decode_field(
    data_section: &[u8],
    representation: &DataRepresentation,
    bitmap_section: Option<&[u8]>,
    num_grid_points: usize,
) -> Result<Vec<f64>> {
    if data_section.len() < 5 || data_section[4] != 7 {
        return Err(Error::InvalidSection {
            section: data_section.get(4).copied().unwrap_or(7),
            reason: "not a data section".into(),
        });
    }

    let payload = &data_section[5..];
    match representation {
        DataRepresentation::SimplePacking(params) => {
            let encoded_values = match bitmap_section {
                Some(_) => params.encoded_values,
                None => num_grid_points,
            };
            let unpacked = unpack_simple(payload, params, encoded_values)?;
            match bitmap_section {
                Some(bitmap) => apply_bitmap(bitmap, unpacked, num_grid_points),
                None => {
                    if unpacked.len() != num_grid_points {
                        return Err(Error::DataLengthMismatch {
                            expected: num_grid_points,
                            actual: unpacked.len(),
                        });
                    }
                    Ok(unpacked)
                }
            }
        }
        DataRepresentation::Unsupported(template) => Err(Error::UnsupportedDataTemplate(*template)),
    }
}

/// Parse bitmap presence from Section 6.
pub fn bitmap_payload(section_bytes: &[u8]) -> Result<Option<&[u8]>> {
    if section_bytes.len() < 6 {
        return Err(Error::InvalidSection {
            section: 6,
            reason: format!("expected at least 6 bytes, got {}", section_bytes.len()),
        });
    }
    if section_bytes[4] != 6 {
        return Err(Error::InvalidSection {
            section: section_bytes[4],
            reason: "not a bitmap section".into(),
        });
    }

    match section_bytes[5] {
        255 => Ok(None),
        0 => Ok(Some(&section_bytes[6..])),
        indicator => Err(Error::UnsupportedBitmapIndicator(indicator)),
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

/// Unpack simple-packed values.
pub fn unpack_simple(
    data_bytes: &[u8],
    params: &SimplePackingParams,
    num_values: usize,
) -> Result<Vec<f64>> {
    let bits = params.bits_per_value as usize;
    if bits == 0 {
        return Ok(vec![params.reference_value as f64; num_values]);
    }
    if bits > u64::BITS as usize {
        return Err(Error::UnsupportedPackingWidth(params.bits_per_value));
    }

    let required_bits = bits
        .checked_mul(num_values)
        .ok_or_else(|| Error::Other("bit count overflow during unpacking".into()))?;
    let required_bytes = required_bits.div_ceil(8);
    if data_bytes.len() < required_bytes {
        return Err(Error::Truncated {
            offset: data_bytes.len() as u64,
        });
    }

    let binary_factor = 2.0_f64.powi(params.binary_scale as i32);
    let decimal_factor = 10.0_f64.powi(-(params.decimal_scale as i32));
    let reference = params.reference_value as f64;
    let mut reader = BitReader::new(data_bytes);
    let mut values = Vec::with_capacity(num_values);

    for _ in 0..num_values {
        let packed = reader.read(bits)?;
        values.push(reference + (packed as f64) * binary_factor * decimal_factor);
    }

    Ok(values)
}

fn apply_bitmap(
    bitmap_payload: &[u8],
    packed_values: Vec<f64>,
    num_grid_points: usize,
) -> Result<Vec<f64>> {
    let mut decoded = Vec::with_capacity(num_grid_points);
    let mut packed_iter = packed_values.into_iter();
    let mut present_points = 0usize;

    for bit_index in 0..num_grid_points {
        if bitmap_bit(bitmap_payload, bit_index)? {
            present_points += 1;
            decoded.push(packed_iter.next().ok_or(Error::MissingBitmap)?);
        } else {
            decoded.push(f64::NAN);
        }
    }

    let extra_values = packed_iter.count();
    if extra_values > 0 {
        return Err(Error::DataLengthMismatch {
            expected: present_points,
            actual: present_points + extra_values,
        });
    }

    Ok(decoded)
}

fn bitmap_bit(bitmap_payload: &[u8], index: usize) -> Result<bool> {
    let byte_index = index / 8;
    let bit_index = index % 8;
    let byte = bitmap_payload
        .get(byte_index)
        .copied()
        .ok_or(Error::MissingBitmap)?;
    Ok(((byte >> (7 - bit_index)) & 1) != 0)
}

struct BitReader<'a> {
    data: &'a [u8],
    bit_offset: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_offset: 0,
        }
    }

    fn read(&mut self, bit_count: usize) -> Result<u64> {
        let mut remaining = bit_count;
        let mut value = 0u64;

        while remaining > 0 {
            let byte_index = self.bit_offset / 8;
            let bit_index = self.bit_offset % 8;
            let byte = *self.data.get(byte_index).ok_or(Error::Truncated {
                offset: byte_index as u64,
            })?;
            let available = 8 - bit_index;
            let take = remaining.min(available);
            let mask = ((1u16 << take) - 1) as u8;
            let shift = available - take;
            let bits = (byte >> shift) & mask;

            value = (value << take) | bits as u64;
            self.bit_offset += take;
            remaining -= take;
        }

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bitmap_payload, decode_field, unpack_simple, DataRepresentation, SimplePackingParams,
    };
    use crate::error::Error;

    #[test]
    fn unpack_simple_constant() {
        let params = SimplePackingParams {
            encoded_values: 5,
            reference_value: 42.0,
            binary_scale: 0,
            decimal_scale: 0,
            bits_per_value: 0,
            original_field_type: 0,
        };
        let values = unpack_simple(&[], &params, 5).unwrap();
        assert_eq!(values, vec![42.0; 5]);
    }

    #[test]
    fn unpack_simple_basic() {
        let params = SimplePackingParams {
            encoded_values: 5,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            bits_per_value: 8,
            original_field_type: 0,
        };
        let values = unpack_simple(&[0, 1, 2, 3, 4], &params, 5).unwrap();
        assert_eq!(values, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn decodes_bitmap_masked_field() {
        let data_section = [0, 0, 0, 8, 7, 10, 20, 30];
        let bitmap_section = [0, 0, 0, 7, 6, 0, 0b1011_0000];
        let representation = DataRepresentation::SimplePacking(SimplePackingParams {
            encoded_values: 3,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            bits_per_value: 8,
            original_field_type: 0,
        });

        let bitmap = bitmap_payload(&bitmap_section).unwrap();
        let decoded = decode_field(&data_section, &representation, bitmap, 4).unwrap();
        assert_eq!(decoded[0], 10.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 20.0);
        assert_eq!(decoded[3], 30.0);
    }

    #[test]
    fn rejects_simple_packing_wider_than_u64() {
        let params = SimplePackingParams {
            encoded_values: 1,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            bits_per_value: 65,
            original_field_type: 0,
        };
        let err = unpack_simple(&[0; 9], &params, 1).unwrap_err();
        assert!(matches!(err, Error::UnsupportedPackingWidth(65)));
    }
}
