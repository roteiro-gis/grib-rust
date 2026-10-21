#[cfg(feature = "png")]
use grib_core::filled_vec;
use grib_core::{GridDefinition, ScaledPackingParams};

use super::super::{
    bits_needed, pack_bitmap, present_mask, quantize_present_values, simple_packing_deltas,
};
use crate::{Error, Result};

#[cfg(feature = "jpeg2000")]
use super::openjpeg::{encode_jpeg2000_codestream, MAX_LOSSLESS_BITS_PER_VALUE};

#[cfg(any(feature = "jpeg2000", feature = "png"))]
#[derive(Debug, Clone)]
pub(super) struct PreparedImagePacking {
    pub(super) params: ScaledPackingParams,
    pub(super) bitmap_payload: Option<Vec<u8>>,
    pub(super) deltas: Vec<u64>,
    pub(super) dimensions: ImageDimensions,
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ImageDimensions {
    width: u32,
    height: u32,
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn image_dimension_u32(value: usize, dimension: &str) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| Error::ValueOutOfRange(format!("image {dimension} {value} exceeds GRIB2 u32")))
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn image_dimension_usize(value: u32, dimension: &str) -> Result<usize> {
    usize::try_from(value)
        .map_err(|_| Error::ValueOutOfRange(format!("image {dimension} {value} exceeds usize")))
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
pub(super) fn prepare_image_packing(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    grid: &GridDefinition,
    decimal_scale: i16,
    packing_name: &str,
    select_bits_per_value: fn(u8) -> Result<u8>,
) -> Result<PreparedImagePacking> {
    let present = present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    if present_count == 0 {
        return Err(Error::ValueOutOfRange(format!(
            "{packing_name} requires at least one present value"
        )));
    }

    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };

    let quantized = quantize_present_values(values, &present, decimal_scale, packing_name)?;
    let (reference_value, deltas) = simple_packing_deltas(&quantized)?;
    let max_delta = deltas.iter().copied().max().unwrap_or(0);
    let minimum_bits = bits_needed(max_delta).max(1);
    let bits_per_value = select_bits_per_value(minimum_bits)?;
    validate_image_deltas_fit(&deltas, bits_per_value)?;

    Ok(PreparedImagePacking {
        params: ScaledPackingParams {
            encoded_values: present_count,
            reference_value,
            binary_scale: 0,
            decimal_scale,
            bits_per_value,
            original_field_type: 0,
        },
        bitmap_payload,
        deltas,
        dimensions: image_dimensions(grid, values.len(), present_count)?,
    })
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn image_dimensions(
    grid: &GridDefinition,
    total_values: usize,
    present_count: usize,
) -> Result<ImageDimensions> {
    if present_count == total_values {
        let (width, height) = grid.shape();
        if width == 0 || height == 0 {
            return Err(Error::UnsupportedGridTemplate(grid.template_number()));
        }
        return Ok(ImageDimensions {
            width: image_dimension_u32(width, "width")?,
            height: image_dimension_u32(height, "height")?,
        });
    }

    Ok(ImageDimensions {
        width: image_dimension_u32(present_count, "width")?,
        height: 1,
    })
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn validate_image_deltas_fit(deltas: &[u64], bits_per_value: u8) -> Result<()> {
    let max_value = if bits_per_value == u64::BITS as u8 {
        u64::MAX
    } else {
        (1u64 << bits_per_value) - 1
    };
    if deltas.iter().any(|delta| *delta > max_value) {
        return Err(Error::UnsupportedPackingWidth(bits_per_value));
    }
    Ok(())
}

#[cfg(feature = "jpeg2000")]
pub(super) fn jpeg2000_bits_per_value(minimum_bits: u8) -> Result<u8> {
    if (1..=MAX_LOSSLESS_BITS_PER_VALUE).contains(&minimum_bits) {
        Ok(minimum_bits)
    } else {
        Err(Error::UnsupportedPackingWidth(minimum_bits))
    }
}

#[cfg(feature = "png")]
pub(super) fn png_bits_per_value(minimum_bits: u8) -> Result<u8> {
    match minimum_bits {
        0 | 1 => Ok(1),
        2 => Ok(2),
        3 | 4 => Ok(4),
        5..=8 => Ok(8),
        9..=16 => Ok(16),
        17..=24 => Ok(24),
        25..=32 => Ok(32),
        bits => Err(Error::UnsupportedPackingWidth(bits)),
    }
}

#[cfg(feature = "png")]
pub(super) fn encode_png_payload(
    deltas: &[u64],
    bits_per_value: u8,
    dimensions: ImageDimensions,
) -> Result<Vec<u8>> {
    validate_image_sample_count(deltas.len(), dimensions)?;
    let (color_type, bit_depth, image_data) = png_image_data(deltas, bits_per_value, dimensions)?;

    let mut payload = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut payload, dimensions.width, dimensions.height);
        encoder.set_color(color_type);
        encoder.set_depth(bit_depth);
        let mut writer = encoder.write_header().map_err(|error| Error::Codec {
            codec: "PNG",
            operation: "write header",
            reason: error.to_string(),
        })?;
        writer
            .write_image_data(&image_data)
            .map_err(|error| Error::Codec {
                codec: "PNG",
                operation: "encode image data",
                reason: error.to_string(),
            })?;
    }
    Ok(payload)
}

#[cfg(feature = "png")]
fn png_image_data(
    deltas: &[u64],
    bits_per_value: u8,
    dimensions: ImageDimensions,
) -> Result<(png::ColorType, png::BitDepth, Vec<u8>)> {
    match bits_per_value {
        1 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::One,
            pack_png_subbyte_rows(deltas, dimensions, 1)?,
        )),
        2 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::Two,
            pack_png_subbyte_rows(deltas, dimensions, 2)?,
        )),
        4 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::Four,
            pack_png_subbyte_rows(deltas, dimensions, 4)?,
        )),
        8 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            deltas
                .iter()
                .map(|delta| u8::try_from(*delta))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?,
        )),
        16 => {
            let byte_count = deltas
                .len()
                .checked_mul(2)
                .ok_or(Error::ArithmeticOverflow {
                    operation: "computing PNG sample byte count",
                })?;
            let mut data = Vec::new();
            data.try_reserve_exact(byte_count)
                .map_err(|error| Error::allocation("bytes for PNG samples", byte_count, error))?;
            for delta in deltas {
                data.extend_from_slice(
                    &u16::try_from(*delta)
                        .map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?
                        .to_be_bytes(),
                );
            }
            Ok((png::ColorType::Grayscale, png::BitDepth::Sixteen, data))
        }
        24 => Ok((
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            pack_png_multibyte_samples(deltas, 3)?,
        )),
        32 => Ok((
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            pack_png_multibyte_samples(deltas, 4)?,
        )),
        bits => Err(Error::UnsupportedPackingWidth(bits)),
    }
}

#[cfg(feature = "png")]
fn pack_png_subbyte_rows(
    deltas: &[u64],
    dimensions: ImageDimensions,
    bits_per_value: u8,
) -> Result<Vec<u8>> {
    let width = image_dimension_usize(dimensions.width, "width")?;
    let height = image_dimension_usize(dimensions.height, "height")?;
    let bits = usize::from(bits_per_value);
    let row_bits = width.checked_mul(bits).ok_or(Error::ArithmeticOverflow {
        operation: "computing PNG row width",
    })?;
    let row_bytes = row_bits.div_ceil(8);
    let byte_count = row_bytes
        .checked_mul(height)
        .ok_or(Error::ArithmeticOverflow {
            operation: "computing PNG data length",
        })?;
    let mut data = filled_vec(byte_count, 0, "PNG image")?;

    for (index, delta) in deltas.iter().copied().enumerate() {
        let row = index / width;
        let column = index % width;
        let bit_offset = column.checked_mul(bits).ok_or(Error::ArithmeticOverflow {
            operation: "computing a PNG bit offset",
        })?;
        let byte_index = row
            .checked_mul(row_bytes)
            .and_then(|row_offset| row_offset.checked_add(bit_offset / 8))
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing a PNG byte offset",
            })?;
        let shift = 8 - bits - (bit_offset % 8);
        data[byte_index] |= (delta as u8) << shift;
    }

    Ok(data)
}

#[cfg(feature = "png")]
fn pack_png_multibyte_samples(deltas: &[u64], bytes_per_sample: usize) -> Result<Vec<u8>> {
    let byte_count =
        deltas
            .len()
            .checked_mul(bytes_per_sample)
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing PNG data length",
            })?;
    let mut data = Vec::new();
    data.try_reserve_exact(byte_count)
        .map_err(|error| Error::allocation("bytes for PNG samples", byte_count, error))?;
    for delta in deltas {
        let bytes = u32::try_from(*delta)
            .map_err(|_| Error::UnsupportedPackingWidth((bytes_per_sample * 8) as u8))?
            .to_be_bytes();
        data.extend_from_slice(&bytes[4 - bytes_per_sample..]);
    }
    Ok(data)
}

#[cfg(feature = "jpeg2000")]
pub(super) fn encode_jpeg2000_payload(
    deltas: &[u64],
    bits_per_value: u8,
    dimensions: ImageDimensions,
) -> Result<Vec<u8>> {
    validate_image_sample_count(deltas.len(), dimensions)?;
    encode_jpeg2000_codestream(deltas, bits_per_value, dimensions.width, dimensions.height)
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn validate_image_sample_count(sample_count: usize, dimensions: ImageDimensions) -> Result<()> {
    let width = image_dimension_usize(dimensions.width, "width")?;
    let height = image_dimension_usize(dimensions.height, "height")?;
    let expected = width.checked_mul(height).ok_or(Error::ArithmeticOverflow {
        operation: "computing image sample count",
    })?;
    if sample_count != expected {
        return Err(Error::DataLengthMismatch {
            expected,
            actual: sample_count,
        });
    }
    Ok(())
}
