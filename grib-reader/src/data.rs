//! Data Section (Section 7) decoding.

use crate::error::{Error, Result};
use grib_core::bit::{read_bit, BitReader};
pub use grib_core::data::{
    ComplexPackingParams, DataRepresentation, ImagePackingParams, Jpeg2000PackingParams,
    PngPackingParams, SimplePackingParams, SpatialDifferencingParams,
};

/// Numeric target type for decoded field values.
pub trait DecodeSample: Copy + Sized {
    fn from_f64(value: f64) -> Self;
    fn nan() -> Self;
}

impl DecodeSample for f32 {
    fn from_f64(value: f64) -> Self {
        value as f32
    }

    fn nan() -> Self {
        f32::NAN
    }
}

impl DecodeSample for f64 {
    fn from_f64(value: f64) -> Self {
        value
    }

    fn nan() -> Self {
        f64::NAN
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

    decode_payload(
        &data_section[5..],
        representation,
        bitmap_section,
        num_grid_points,
    )
}

/// Decode Section 7 payload into a caller-provided buffer, applying Section 6
/// bitmap when present.
pub fn decode_field_into<T: DecodeSample>(
    data_section: &[u8],
    representation: &DataRepresentation,
    bitmap_section: Option<&[u8]>,
    num_grid_points: usize,
    out: &mut [T],
) -> Result<()> {
    if data_section.len() < 5 || data_section[4] != 7 {
        return Err(Error::InvalidSection {
            section: data_section.get(4).copied().unwrap_or(7),
            reason: "not a data section".into(),
        });
    }

    decode_payload_into(
        &data_section[5..],
        representation,
        bitmap_section,
        num_grid_points,
        out,
    )
}

pub(crate) fn decode_payload(
    payload: &[u8],
    representation: &DataRepresentation,
    bitmap_section: Option<&[u8]>,
    num_grid_points: usize,
) -> Result<Vec<f64>> {
    let mut values = vec![0.0; num_grid_points];
    decode_payload_into(
        payload,
        representation,
        bitmap_section,
        num_grid_points,
        &mut values,
    )?;
    Ok(values)
}

pub(crate) fn decode_payload_into<T: DecodeSample>(
    payload: &[u8],
    representation: &DataRepresentation,
    bitmap_section: Option<&[u8]>,
    num_grid_points: usize,
    out: &mut [T],
) -> Result<()> {
    if out.len() != num_grid_points {
        return Err(Error::DataLengthMismatch {
            expected: num_grid_points,
            actual: out.len(),
        });
    }

    let expected_values = match representation {
        DataRepresentation::SimplePacking(params) => params.encoded_values,
        DataRepresentation::ComplexPacking(params) => params.encoded_values,
        DataRepresentation::Jpeg2000Packing(params) => params.packing.encoded_values,
        DataRepresentation::PngPacking(params) => params.packing.encoded_values,
        DataRepresentation::Unsupported(template) => {
            return Err(Error::UnsupportedDataTemplate(*template));
        }
    };
    match bitmap_section {
        Some(bitmap_payload) => {
            let present_values = count_bitmap_present_points(bitmap_payload, num_grid_points)?;
            if expected_values != present_values {
                return Err(Error::DataLengthMismatch {
                    expected: present_values,
                    actual: expected_values,
                });
            }
        }
        None if expected_values != num_grid_points => {
            return Err(Error::DataLengthMismatch {
                expected: num_grid_points,
                actual: expected_values,
            });
        }
        None => {}
    }

    let mut output = OutputCursor::new(out, bitmap_section);
    match representation {
        DataRepresentation::SimplePacking(params) => {
            unpack_simple_into(payload, params, expected_values, &mut output)?
        }
        DataRepresentation::ComplexPacking(params) => {
            unpack_complex_into(payload, params, &mut output)?
        }
        DataRepresentation::Jpeg2000Packing(params) => {
            unpack_jpeg2000_into(payload, params, expected_values, &mut output)?
        }
        DataRepresentation::PngPacking(params) => {
            unpack_png_into(payload, params, expected_values, &mut output)?
        }
        DataRepresentation::Unsupported(_) => unreachable!(),
    }
    output.finish()
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

pub(crate) fn count_bitmap_present_points(
    bitmap_payload: &[u8],
    num_grid_points: usize,
) -> Result<usize> {
    let full_bytes = num_grid_points / 8;
    let remaining_bits = num_grid_points % 8;
    let required_bytes = full_bytes + usize::from(remaining_bits > 0);
    if bitmap_payload.len() < required_bytes {
        return Err(Error::DataLengthMismatch {
            expected: required_bytes,
            actual: bitmap_payload.len(),
        });
    }

    let mut present = bitmap_payload[..full_bytes]
        .iter()
        .map(|byte| byte.count_ones() as usize)
        .sum();
    if remaining_bits > 0 {
        let mask = u8::MAX << (8 - remaining_bits);
        present += (bitmap_payload[full_bytes] & mask).count_ones() as usize;
    }

    Ok(present)
}

/// Unpack simple-packed values.
pub fn unpack_simple(
    data_bytes: &[u8],
    params: &SimplePackingParams,
    num_values: usize,
) -> Result<Vec<f64>> {
    let mut values = vec![0.0; num_values];
    let mut output = OutputCursor::new(&mut values, None);
    unpack_simple_into(data_bytes, params, num_values, &mut output)?;
    output.finish()?;
    Ok(values)
}

fn unpack_simple_into<T: DecodeSample>(
    data_bytes: &[u8],
    params: &SimplePackingParams,
    num_values: usize,
    output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    let bits = params.bits_per_value as usize;
    let binary_factor = 2.0_f64.powi(params.binary_scale as i32);
    let decimal_factor = 10.0_f64.powi(-(params.decimal_scale as i32));
    let reference = params.reference_value as f64;
    if bits == 0 {
        let constant = T::from_f64(scale_decoded_value(
            reference,
            0.0,
            binary_factor,
            decimal_factor,
        ));
        for _ in 0..num_values {
            output.push_present(constant)?;
        }
        return Ok(());
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

    let mut reader = BitReader::new(data_bytes);

    for _ in 0..num_values {
        let packed = reader.read(bits)?;
        output.push_present(T::from_f64(scale_decoded_value(
            reference,
            packed as f64,
            binary_factor,
            decimal_factor,
        )))?;
    }

    Ok(())
}

#[cfg(feature = "jpeg2000")]
fn unpack_jpeg2000_into<T: DecodeSample>(
    data_bytes: &[u8],
    params: &Jpeg2000PackingParams,
    num_values: usize,
    output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    validate_jpeg2000_bits(params.packing.bits_per_value)?;

    let image = jpeg2k::Image::from_bytes(data_bytes)
        .map_err(|err| Error::Other(format!("JPEG 2000 decode failed: {err}")))?;
    if image.num_components() != 1 {
        return Err(Error::Other(format!(
            "JPEG 2000 GRIB packing requires one component, got {}",
            image.num_components()
        )));
    }

    let component = image
        .components()
        .first()
        .ok_or_else(|| Error::Other("JPEG 2000 image has no components".into()))?;
    if component.is_signed() {
        return Err(Error::Other(
            "JPEG 2000 GRIB packing requires unsigned component data".into(),
        ));
    }
    if component.precision() > u32::from(params.packing.bits_per_value) {
        return Err(Error::Other(format!(
            "JPEG 2000 component precision {} exceeds GRIB bits-per-value {}",
            component.precision(),
            params.packing.bits_per_value
        )));
    }

    let sample_count = image_sample_count(component.width(), component.height())?;
    if sample_count != num_values {
        return Err(Error::DataLengthMismatch {
            expected: num_values,
            actual: sample_count,
        });
    }

    for &sample in component.data() {
        let raw = jpeg2000_raw_value(sample, params.packing.bits_per_value)?;
        push_image_value(output, &params.packing, raw)?;
    }

    Ok(())
}

#[cfg(not(feature = "jpeg2000"))]
fn unpack_jpeg2000_into<T: DecodeSample>(
    _data_bytes: &[u8],
    _params: &Jpeg2000PackingParams,
    _num_values: usize,
    _output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    Err(Error::UnsupportedDataTemplate(40))
}

#[cfg(feature = "jpeg2000")]
fn validate_jpeg2000_bits(bits_per_value: u8) -> Result<()> {
    if !(1..=32).contains(&bits_per_value) {
        return Err(Error::UnsupportedPackingWidth(bits_per_value));
    }
    Ok(())
}

#[cfg(feature = "jpeg2000")]
fn jpeg2000_raw_value(sample: i32, bits_per_value: u8) -> Result<u64> {
    let raw = if bits_per_value == 32 {
        u64::from(u32::from_ne_bytes(sample.to_ne_bytes()))
    } else {
        u64::try_from(sample).map_err(|_| {
            Error::Other("JPEG 2000 unsigned component yielded a negative sample".into())
        })?
    };
    validate_raw_value_fits(raw, bits_per_value)?;
    Ok(raw)
}

#[cfg(feature = "png")]
fn unpack_png_into<T: DecodeSample>(
    data_bytes: &[u8],
    params: &PngPackingParams,
    num_values: usize,
    output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    validate_png_bits(params.packing.bits_per_value)?;

    let decoder = png::Decoder::new(std::io::Cursor::new(data_bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|err| Error::Other(format!("PNG decode failed: {err}")))?;
    let buffer_size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::Other("PNG output buffer size overflow".into()))?;
    let mut buffer = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|err| Error::Other(format!("PNG decode failed: {err}")))?;
    let data = &buffer[..info.buffer_size()];

    let sample_count = image_sample_count(info.width, info.height)?;
    if sample_count != num_values {
        return Err(Error::DataLengthMismatch {
            expected: num_values,
            actual: sample_count,
        });
    }

    match (
        info.color_type,
        info.bit_depth,
        params.packing.bits_per_value,
    ) {
        (png::ColorType::Grayscale, png::BitDepth::One, 1)
        | (png::ColorType::Grayscale, png::BitDepth::Two, 2)
        | (png::ColorType::Grayscale, png::BitDepth::Four, 4) => unpack_png_subbyte_grayscale(
            data,
            info.width,
            info.height,
            params.packing.bits_per_value,
            &params.packing,
            output,
        ),
        (png::ColorType::Grayscale, png::BitDepth::Eight, 8) => {
            unpack_png_bytes(data, 1, num_values, &params.packing, output)
        }
        (png::ColorType::Grayscale, png::BitDepth::Sixteen, 16) => {
            unpack_png_u16(data, num_values, &params.packing, output)
        }
        (png::ColorType::Rgb, png::BitDepth::Eight, 24) => {
            unpack_png_bytes(data, 3, num_values, &params.packing, output)
        }
        (png::ColorType::Rgba, png::BitDepth::Eight, 32) => {
            unpack_png_bytes(data, 4, num_values, &params.packing, output)
        }
        (color_type, bit_depth, bits_per_value) => Err(Error::Other(format!(
            "PNG image layout {color_type:?}/{bit_depth:?} is incompatible with GRIB bits-per-value {bits_per_value}"
        ))),
    }
}

#[cfg(not(feature = "png"))]
fn unpack_png_into<T: DecodeSample>(
    _data_bytes: &[u8],
    _params: &PngPackingParams,
    _num_values: usize,
    _output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    Err(Error::UnsupportedDataTemplate(41))
}

#[cfg(feature = "png")]
fn validate_png_bits(bits_per_value: u8) -> Result<()> {
    if !matches!(bits_per_value, 1 | 2 | 4 | 8 | 16 | 24 | 32) {
        return Err(Error::UnsupportedPackingWidth(bits_per_value));
    }
    Ok(())
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn image_sample_count(width: u32, height: u32) -> Result<usize> {
    let width = usize::try_from(width).map_err(|_| Error::Other("image width overflow".into()))?;
    let height =
        usize::try_from(height).map_err(|_| Error::Other("image height overflow".into()))?;
    width
        .checked_mul(height)
        .ok_or_else(|| Error::Other("image sample count overflow".into()))
}

#[cfg(feature = "png")]
fn unpack_png_subbyte_grayscale<T: DecodeSample>(
    data: &[u8],
    width: u32,
    height: u32,
    bits_per_sample: u8,
    params: &ImagePackingParams,
    output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    let width = usize::try_from(width).map_err(|_| Error::Other("PNG width overflow".into()))?;
    let height = usize::try_from(height).map_err(|_| Error::Other("PNG height overflow".into()))?;
    let bits = usize::from(bits_per_sample);
    let row_bytes = width
        .checked_mul(bits)
        .ok_or_else(|| Error::Other("PNG row width overflow".into()))?
        .div_ceil(8);
    let expected_bytes = row_bytes
        .checked_mul(height)
        .ok_or_else(|| Error::Other("PNG data length overflow".into()))?;
    if data.len() < expected_bytes {
        return Err(Error::DataLengthMismatch {
            expected: expected_bytes,
            actual: data.len(),
        });
    }

    let mask = (1u8 << bits) - 1;
    for row in data[..expected_bytes].chunks_exact(row_bytes) {
        for x in 0..width {
            let bit_offset = x
                .checked_mul(bits)
                .ok_or_else(|| Error::Other("PNG bit offset overflow".into()))?;
            let shift = 8 - bits - (bit_offset % 8);
            let raw = u64::from((row[bit_offset / 8] >> shift) & mask);
            push_image_value(output, params, raw)?;
        }
    }

    Ok(())
}

#[cfg(feature = "png")]
fn unpack_png_bytes<T: DecodeSample>(
    data: &[u8],
    bytes_per_sample: usize,
    num_values: usize,
    params: &ImagePackingParams,
    output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    let expected_bytes = num_values
        .checked_mul(bytes_per_sample)
        .ok_or_else(|| Error::Other("PNG data length overflow".into()))?;
    if data.len() < expected_bytes {
        return Err(Error::DataLengthMismatch {
            expected: expected_bytes,
            actual: data.len(),
        });
    }

    for sample in data[..expected_bytes].chunks_exact(bytes_per_sample) {
        let raw = sample
            .iter()
            .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte));
        push_image_value(output, params, raw)?;
    }

    Ok(())
}

#[cfg(feature = "png")]
fn unpack_png_u16<T: DecodeSample>(
    data: &[u8],
    num_values: usize,
    params: &ImagePackingParams,
    output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    let expected_bytes = num_values
        .checked_mul(2)
        .ok_or_else(|| Error::Other("PNG data length overflow".into()))?;
    if data.len() < expected_bytes {
        return Err(Error::DataLengthMismatch {
            expected: expected_bytes,
            actual: data.len(),
        });
    }

    for sample in data[..expected_bytes].chunks_exact(2) {
        let raw = u64::from(u16::from_be_bytes(sample.try_into().unwrap()));
        push_image_value(output, params, raw)?;
    }

    Ok(())
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn push_image_value<T: DecodeSample>(
    output: &mut OutputCursor<'_, T>,
    params: &ImagePackingParams,
    raw: u64,
) -> Result<()> {
    validate_raw_value_fits(raw, params.bits_per_value)?;
    output.push_present(scale_image_value(params, raw))
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn validate_raw_value_fits(raw: u64, bits_per_value: u8) -> Result<()> {
    let max_value = if bits_per_value == u64::BITS as u8 {
        u64::MAX
    } else {
        (1u64 << bits_per_value) - 1
    };
    if raw > max_value {
        return Err(Error::Other(format!(
            "decoded image sample {raw} exceeds GRIB bits-per-value {bits_per_value}"
        )));
    }
    Ok(())
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn scale_image_value<T: DecodeSample>(params: &ImagePackingParams, raw: u64) -> T {
    let binary_factor = 2.0_f64.powi(params.binary_scale as i32);
    let decimal_factor = 10.0_f64.powi(-(params.decimal_scale as i32));
    T::from_f64(scale_decoded_value(
        params.reference_value as f64,
        raw as f64,
        binary_factor,
        decimal_factor,
    ))
}

#[cfg(test)]
fn unpack_complex(data_bytes: &[u8], params: &ComplexPackingParams) -> Result<Vec<f64>> {
    let mut values = vec![0.0; params.encoded_values];
    let mut output = OutputCursor::new(&mut values, None);
    unpack_complex_into(data_bytes, params, &mut output)?;
    output.finish()?;
    Ok(values)
}

fn unpack_complex_into<T: DecodeSample>(
    data_bytes: &[u8],
    params: &ComplexPackingParams,
    output: &mut OutputCursor<'_, T>,
) -> Result<()> {
    if params.num_groups == 0 {
        return Err(Error::InvalidSection {
            section: 5,
            reason: "complex packing requires at least one group".into(),
        });
    }

    let mut reader = BitReader::new(data_bytes);
    let mut spatial = params
        .spatial_differencing
        .map(|spatial| read_spatial_descriptors(&mut reader, spatial))
        .transpose()?
        .map(SpatialRestoreState::new);

    let layout = GroupReaderLayout::new(reader.bit_offset(), params)?;
    let mut reference_reader = BitReader::with_offset(data_bytes, layout.reference_offset);
    let mut width_reader = BitReader::with_offset(data_bytes, layout.width_offset);
    let mut length_reader = BitReader::with_offset(data_bytes, layout.length_offset);
    let mut value_reader = BitReader::with_offset(data_bytes, layout.value_offset);
    let binary_factor = 2.0_f64.powi(params.binary_scale as i32);
    let decimal_factor = 10.0_f64.powi(-(params.decimal_scale as i32));
    let reference = params.reference_value as f64;
    let mut actual_total = 0usize;

    for group_index in 0..params.num_groups {
        let group_reference = reference_reader.read(params.group_reference_bits as usize)?;
        let width_delta = width_reader.read(params.group_width_bits as usize)?;
        let group_width = usize::from(params.group_width_reference)
            .checked_add(width_delta as usize)
            .ok_or_else(|| Error::Other("group width overflow".into()))?;
        let group_length = read_group_length(&mut length_reader, params, group_index)?;

        actual_total = actual_total
            .checked_add(group_length)
            .ok_or_else(|| Error::Other("group length overflow".into()))?;

        if group_width == 0 {
            let raw_value = decode_constant_group_value(
                group_reference,
                params.group_reference_bits as usize,
                params.missing_value_management,
            )?;
            for _ in 0..group_length {
                let value = spatial
                    .as_mut()
                    .map_or(Ok(raw_value), |state| state.restore(raw_value))?;
                output.push_present(scale_complex_value(
                    reference,
                    binary_factor,
                    decimal_factor,
                    value,
                ))?;
            }
            continue;
        }

        if group_width > u64::BITS as usize {
            return Err(Error::UnsupportedPackingWidth(group_width as u8));
        }

        let group_reference = i64::try_from(group_reference)
            .map_err(|_| Error::Other("group reference exceeds i64 range".into()))?;
        for _ in 0..group_length {
            let packed = value_reader.read(group_width)?;
            let value = decode_group_value(
                group_reference,
                packed,
                group_width,
                params.missing_value_management,
            )?;
            let value = spatial
                .as_mut()
                .map_or(Ok(value), |state| state.restore(value))?;
            output.push_present(scale_complex_value(
                reference,
                binary_factor,
                decimal_factor,
                value,
            ))?;
        }
    }

    if actual_total != params.encoded_values {
        return Err(Error::DataLengthMismatch {
            expected: params.encoded_values,
            actual: actual_total,
        });
    }

    if output.values_written() != params.encoded_values {
        return Err(Error::DataLengthMismatch {
            expected: params.encoded_values,
            actual: output.values_written(),
        });
    }

    if let Some(spatial) = spatial {
        spatial.finish()?;
    }

    Ok(())
}

fn read_group_length(
    reader: &mut BitReader<'_>,
    params: &ComplexPackingParams,
    group_index: usize,
) -> Result<usize> {
    if params.scaled_group_length_bits as usize > u64::BITS as usize {
        return Err(Error::UnsupportedPackingWidth(
            params.scaled_group_length_bits,
        ));
    }

    if group_index + 1 == params.num_groups {
        return Ok(params.true_length_last_group as usize);
    }

    let scaled = reader
        .read(params.scaled_group_length_bits as usize)?
        .checked_mul(u64::from(params.group_length_increment))
        .ok_or_else(|| Error::Other("group length overflow".into()))?;
    let length = u64::from(params.group_length_reference)
        .checked_add(scaled)
        .ok_or_else(|| Error::Other("group length overflow".into()))?;
    usize::try_from(length).map_err(|_| Error::Other("group length overflow".into()))
}

fn read_spatial_descriptors(
    reader: &mut BitReader<'_>,
    params: SpatialDifferencingParams,
) -> Result<SpatialDescriptors> {
    if params.descriptor_octets == 0 {
        return Err(Error::InvalidSection {
            section: 5,
            reason: "spatial differencing requires at least one descriptor octet".into(),
        });
    }

    let bit_count = usize::from(params.descriptor_octets) * 8;
    if bit_count > u64::BITS as usize {
        return Err(Error::Other(
            "spatial differencing descriptors wider than 8 octets are not supported".into(),
        ));
    }

    let first_value = reader.read_signed(bit_count)?;
    let second_value = if params.order == 2 {
        Some(reader.read_signed(bit_count)?)
    } else {
        None
    };
    for _ in usize::from(params.order.min(2))..params.order as usize {
        let _ = reader.read_signed(bit_count)?;
    }
    let overall_minimum = reader.read_signed(bit_count)?;

    Ok(SpatialDescriptors {
        order: params.order,
        first_value,
        second_value,
        overall_minimum,
    })
}

fn decode_constant_group_value(
    group_reference: u64,
    group_reference_bits: usize,
    missing_value_management: u8,
) -> Result<Option<i64>> {
    if is_missing_code(
        group_reference,
        group_reference_bits,
        missing_value_management,
        MissingKind::Primary,
    )? || is_missing_code(
        group_reference,
        group_reference_bits,
        missing_value_management,
        MissingKind::Secondary,
    )? {
        return Ok(None);
    }

    let value = i64::try_from(group_reference)
        .map_err(|_| Error::Other("group reference exceeds i64 range".into()))?;
    Ok(Some(value))
}

fn decode_group_value(
    group_reference: i64,
    packed: u64,
    group_width: usize,
    missing_value_management: u8,
) -> Result<Option<i64>> {
    if is_missing_code(
        packed,
        group_width,
        missing_value_management,
        MissingKind::Primary,
    )? || is_missing_code(
        packed,
        group_width,
        missing_value_management,
        MissingKind::Secondary,
    )? {
        return Ok(None);
    }

    let packed =
        i64::try_from(packed).map_err(|_| Error::Other("packed value exceeds i64 range".into()))?;
    let value = group_reference
        .checked_add(packed)
        .ok_or_else(|| Error::Other("decoded complex packing value overflow".into()))?;
    Ok(Some(value))
}

fn is_missing_code(
    value: u64,
    bit_width: usize,
    missing_value_management: u8,
    kind: MissingKind,
) -> Result<bool> {
    let required_mode = match kind {
        MissingKind::Primary => 1,
        MissingKind::Secondary => 2,
    };
    if missing_value_management < required_mode {
        return Ok(false);
    }

    let Some(code) = missing_code(bit_width, kind)? else {
        return Ok(false);
    };
    Ok(value == code)
}

fn missing_code(bit_width: usize, kind: MissingKind) -> Result<Option<u64>> {
    if bit_width == 0 {
        return Ok(None);
    }
    if bit_width > u64::BITS as usize {
        return Err(Error::UnsupportedPackingWidth(bit_width as u8));
    }

    let max_value = if bit_width == u64::BITS as usize {
        u64::MAX
    } else {
        (1u64 << bit_width) - 1
    };

    let code = match kind {
        MissingKind::Primary => max_value,
        MissingKind::Secondary => max_value.saturating_sub(1),
    };
    Ok(Some(code))
}

fn scale_complex_value<T: DecodeSample>(
    reference: f64,
    binary_factor: f64,
    decimal_factor: f64,
    value: Option<i64>,
) -> T {
    match value {
        Some(value) => T::from_f64(scale_decoded_value(
            reference,
            value as f64,
            binary_factor,
            decimal_factor,
        )),
        None => T::nan(),
    }
}

fn scale_decoded_value(
    reference: f64,
    packed_delta: f64,
    binary_factor: f64,
    decimal_factor: f64,
) -> f64 {
    (reference + packed_delta * binary_factor) * decimal_factor
}

fn bitmap_bit(bitmap_payload: &[u8], index: usize) -> Result<bool> {
    read_bit(bitmap_payload, index).map_err(|_| Error::DataLengthMismatch {
        expected: index / 8 + 1,
        actual: bitmap_payload.len(),
    })
}

struct GroupReaderLayout {
    reference_offset: usize,
    width_offset: usize,
    length_offset: usize,
    value_offset: usize,
}

impl GroupReaderLayout {
    fn new(start_bit_offset: usize, params: &ComplexPackingParams) -> Result<Self> {
        if params.group_reference_bits as usize > u64::BITS as usize {
            return Err(Error::UnsupportedPackingWidth(params.group_reference_bits));
        }
        if params.group_width_bits as usize > u64::BITS as usize {
            return Err(Error::UnsupportedPackingWidth(params.group_width_bits));
        }
        if params.scaled_group_length_bits as usize > u64::BITS as usize {
            return Err(Error::UnsupportedPackingWidth(
                params.scaled_group_length_bits,
            ));
        }

        let reference_offset = start_bit_offset;
        let width_offset = align_bit_offset(add_group_bits(
            reference_offset,
            params.num_groups,
            params.group_reference_bits as usize,
        )?)?;
        let length_offset = align_bit_offset(add_group_bits(
            width_offset,
            params.num_groups,
            params.group_width_bits as usize,
        )?)?;
        let value_offset = align_bit_offset(add_group_bits(
            length_offset,
            params.num_groups,
            params.scaled_group_length_bits as usize,
        )?)?;

        Ok(Self {
            reference_offset,
            width_offset,
            length_offset,
            value_offset,
        })
    }
}

fn add_group_bits(start_bit_offset: usize, count: usize, bits_per_group: usize) -> Result<usize> {
    count
        .checked_mul(bits_per_group)
        .and_then(|total_bits| start_bit_offset.checked_add(total_bits))
        .ok_or_else(|| Error::Other("bit offset overflow".into()))
}

fn align_bit_offset(bit_offset: usize) -> Result<usize> {
    let remainder = bit_offset % 8;
    if remainder == 0 {
        Ok(bit_offset)
    } else {
        bit_offset
            .checked_add(8 - remainder)
            .ok_or_else(|| Error::Other("bit offset overflow".into()))
    }
}

struct OutputCursor<'a, T> {
    output: &'a mut [T],
    bitmap: Option<&'a [u8]>,
    next_index: usize,
    values_written: usize,
}

impl<'a, T: DecodeSample> OutputCursor<'a, T> {
    fn new(output: &'a mut [T], bitmap: Option<&'a [u8]>) -> Self {
        Self {
            output,
            bitmap,
            next_index: 0,
            values_written: 0,
        }
    }

    fn push_present(&mut self, value: T) -> Result<()> {
        match self.bitmap {
            Some(bitmap) => {
                while self.next_index < self.output.len() {
                    if bitmap_bit(bitmap, self.next_index)? {
                        self.output[self.next_index] = value;
                        self.next_index += 1;
                        self.values_written += 1;
                        return Ok(());
                    }

                    self.output[self.next_index] = T::nan();
                    self.next_index += 1;
                }

                let expected = count_bitmap_present_points(bitmap, self.output.len())?;
                Err(Error::DataLengthMismatch {
                    expected,
                    actual: self.values_written + 1,
                })
            }
            None => {
                if self.next_index >= self.output.len() {
                    return Err(Error::DataLengthMismatch {
                        expected: self.output.len(),
                        actual: self.next_index + 1,
                    });
                }

                self.output[self.next_index] = value;
                self.next_index += 1;
                self.values_written += 1;
                Ok(())
            }
        }
    }

    fn finish(mut self) -> Result<()> {
        if let Some(bitmap) = self.bitmap {
            while self.next_index < self.output.len() {
                if bitmap_bit(bitmap, self.next_index)? {
                    return Err(Error::DataLengthMismatch {
                        expected: count_bitmap_present_points(bitmap, self.output.len())?,
                        actual: self.values_written,
                    });
                }
                self.output[self.next_index] = T::nan();
                self.next_index += 1;
            }
        }

        if self.next_index != self.output.len() {
            return Err(Error::DataLengthMismatch {
                expected: self.output.len(),
                actual: self.next_index,
            });
        }

        Ok(())
    }
    fn values_written(&self) -> usize {
        self.values_written
    }
}

#[derive(Debug, Clone)]
struct SpatialDescriptors {
    order: u8,
    first_value: i64,
    second_value: Option<i64>,
    overall_minimum: i64,
}

#[derive(Debug, Clone)]
struct SpatialRestoreState {
    descriptors: SpatialDescriptors,
    previous: Option<i64>,
    previous_difference: Option<i64>,
    non_missing_seen: usize,
}

impl SpatialRestoreState {
    fn new(descriptors: SpatialDescriptors) -> Self {
        Self {
            descriptors,
            previous: None,
            previous_difference: None,
            non_missing_seen: 0,
        }
    }

    fn restore(&mut self, value: Option<i64>) -> Result<Option<i64>> {
        let Some(value) = value else {
            return Ok(None);
        };

        let restored = match self.descriptors.order {
            1 => self.restore_first_order(value)?,
            2 => self.restore_second_order(value)?,
            other => return Err(Error::UnsupportedSpatialDifferencingOrder(other)),
        };

        self.previous = Some(restored);
        self.non_missing_seen += 1;
        Ok(Some(restored))
    }

    fn finish(self) -> Result<()> {
        let expected = match self.descriptors.order {
            1 => 1,
            2 => 2,
            other => return Err(Error::UnsupportedSpatialDifferencingOrder(other)),
        };
        if self.non_missing_seen < expected {
            return Err(Error::DataLengthMismatch {
                expected,
                actual: self.non_missing_seen,
            });
        }
        Ok(())
    }

    fn restore_first_order(&mut self, value: i64) -> Result<i64> {
        if self.non_missing_seen == 0 {
            return Ok(self.descriptors.first_value);
        }

        let delta = value
            .checked_add(self.descriptors.overall_minimum)
            .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
        self.previous
            .and_then(|previous| previous.checked_add(delta))
            .ok_or_else(|| Error::Other("spatial differencing overflow".into()))
    }

    fn restore_second_order(&mut self, value: i64) -> Result<i64> {
        match self.non_missing_seen {
            0 => Ok(self.descriptors.first_value),
            1 => {
                let second_value = self.descriptors.second_value.ok_or(Error::InvalidSection {
                    section: 5,
                    reason: "missing second-order spatial differencing descriptors".into(),
                })?;
                self.previous_difference = second_value.checked_sub(self.descriptors.first_value);
                self.previous_difference
                    .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
                Ok(second_value)
            }
            _ => {
                let second_difference = value
                    .checked_add(self.descriptors.overall_minimum)
                    .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
                let difference = self
                    .previous_difference
                    .and_then(|previous_difference| {
                        previous_difference.checked_add(second_difference)
                    })
                    .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
                let next = self
                    .previous
                    .and_then(|previous| previous.checked_add(difference))
                    .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
                self.previous_difference = Some(difference);
                Ok(next)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingKind {
    Primary,
    Secondary,
}

#[cfg(test)]
mod tests {
    use super::{
        bitmap_payload, count_bitmap_present_points, decode_field, decode_payload, unpack_complex,
        unpack_simple, ComplexPackingParams, DataRepresentation, ImagePackingParams,
        PngPackingParams, SimplePackingParams, SpatialDifferencingParams,
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
    fn unpack_simple_applies_decimal_scale_to_reference_and_values() {
        let params = SimplePackingParams {
            encoded_values: 2,
            reference_value: 10.0,
            binary_scale: 0,
            decimal_scale: 1,
            bits_per_value: 8,
            original_field_type: 0,
        };

        let values = unpack_simple(&[0, 20], &params, 2).unwrap();
        assert!((values[0] - 1.0).abs() < 1e-12);
        assert!((values[1] - 3.0).abs() < 1e-12);
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

    #[cfg(feature = "png")]
    #[test]
    fn decodes_png_packing_grayscale8() {
        let payload = encode_png(
            2,
            2,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &[1, 2, 3, 4],
        );
        let representation = DataRepresentation::PngPacking(PngPackingParams {
            packing: ImagePackingParams {
                encoded_values: 4,
                reference_value: 10.0,
                binary_scale: 1,
                decimal_scale: 1,
                bits_per_value: 8,
                original_field_type: 0,
            },
        });

        let decoded = decode_payload(&payload, &representation, None, 4).unwrap();
        assert_float_values(&decoded, &[1.2, 1.4, 1.6, 1.8]);
    }

    #[cfg(feature = "png")]
    #[test]
    fn decodes_png_packing_grayscale16() {
        let payload = encode_png(
            2,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Sixteen,
            &[0x01, 0x00, 0x02, 0x00],
        );
        let representation = DataRepresentation::PngPacking(PngPackingParams {
            packing: ImagePackingParams {
                encoded_values: 2,
                reference_value: 0.0,
                binary_scale: 0,
                decimal_scale: 0,
                bits_per_value: 16,
                original_field_type: 0,
            },
        });

        let decoded = decode_payload(&payload, &representation, None, 2).unwrap();
        assert_eq!(decoded, vec![256.0, 512.0]);
    }

    #[cfg(feature = "png")]
    #[test]
    fn decodes_png_packing_grayscale_subbyte() {
        let payload = encode_png(
            4,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Four,
            &[0x12, 0x34],
        );
        let representation = DataRepresentation::PngPacking(PngPackingParams {
            packing: ImagePackingParams {
                encoded_values: 4,
                reference_value: 0.0,
                binary_scale: 0,
                decimal_scale: 0,
                bits_per_value: 4,
                original_field_type: 0,
            },
        });

        let decoded = decode_payload(&payload, &representation, None, 4).unwrap();
        assert_eq!(decoded, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[cfg(feature = "png")]
    #[test]
    fn decodes_png_packing_rgb8() {
        let payload = encode_png(
            2,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06],
        );
        let representation = DataRepresentation::PngPacking(PngPackingParams {
            packing: ImagePackingParams {
                encoded_values: 2,
                reference_value: 0.0,
                binary_scale: 0,
                decimal_scale: 0,
                bits_per_value: 24,
                original_field_type: 0,
            },
        });

        let decoded = decode_payload(&payload, &representation, None, 2).unwrap();
        assert_eq!(decoded, vec![0x01_02_03 as f64, 0x04_05_06 as f64]);
    }

    #[cfg(feature = "png")]
    #[test]
    fn decodes_png_packing_rgba8() {
        let payload = encode_png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[0x01, 0x02, 0x03, 0x04],
        );
        let representation = DataRepresentation::PngPacking(PngPackingParams {
            packing: ImagePackingParams {
                encoded_values: 1,
                reference_value: 0.0,
                binary_scale: 0,
                decimal_scale: 0,
                bits_per_value: 32,
                original_field_type: 0,
            },
        });

        let decoded = decode_payload(&payload, &representation, None, 1).unwrap();
        assert_eq!(decoded, vec![0x01_02_03_04 as f64]);
    }

    #[cfg(feature = "png")]
    #[test]
    fn decodes_png_packing_with_bitmap() {
        let payload = encode_png(
            3,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &[10, 20, 30],
        );
        let bitmap_section = [0, 0, 0, 7, 6, 0, 0b1011_0000];
        let representation = DataRepresentation::PngPacking(PngPackingParams {
            packing: ImagePackingParams {
                encoded_values: 3,
                reference_value: 0.0,
                binary_scale: 0,
                decimal_scale: 0,
                bits_per_value: 8,
                original_field_type: 0,
            },
        });

        let decoded = decode_payload(
            &payload,
            &representation,
            bitmap_payload(&bitmap_section).unwrap(),
            4,
        )
        .unwrap();
        assert_eq!(decoded[0], 10.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 20.0);
        assert_eq!(decoded[3], 30.0);
    }

    #[cfg(not(feature = "png"))]
    #[test]
    fn png_packing_requires_png_feature() {
        let representation = DataRepresentation::PngPacking(PngPackingParams {
            packing: ImagePackingParams {
                encoded_values: 1,
                reference_value: 0.0,
                binary_scale: 0,
                decimal_scale: 0,
                bits_per_value: 8,
                original_field_type: 0,
            },
        });

        let err = decode_payload(&[], &representation, None, 1).unwrap_err();
        assert!(matches!(err, Error::UnsupportedDataTemplate(41)));
    }

    #[cfg(not(feature = "jpeg2000"))]
    #[test]
    fn jpeg2000_packing_requires_jpeg2000_feature() {
        let representation = DataRepresentation::Jpeg2000Packing(super::Jpeg2000PackingParams {
            packing: ImagePackingParams {
                encoded_values: 1,
                reference_value: 0.0,
                binary_scale: 0,
                decimal_scale: 0,
                bits_per_value: 8,
                original_field_type: 0,
            },
            compression_type: 1,
            target_compression_ratio: 255,
        });

        let err = decode_payload(&[], &representation, None, 1).unwrap_err();
        assert!(matches!(err, Error::UnsupportedDataTemplate(40)));
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

    #[test]
    fn rejects_encoded_value_count_mismatch_without_bitmap() {
        let data_section = [0, 0, 0, 8, 7, 10, 20, 30];
        let representation = DataRepresentation::SimplePacking(SimplePackingParams {
            encoded_values: 3,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            bits_per_value: 8,
            original_field_type: 0,
        });

        let err = decode_field(&data_section, &representation, None, 4).unwrap_err();
        assert!(matches!(
            err,
            Error::DataLengthMismatch {
                expected: 4,
                actual: 3,
            }
        ));
    }

    #[test]
    fn rejects_bitmap_present_count_mismatch() {
        let data_section = [0, 0, 0, 7, 7, 10, 20];
        let bitmap_section = [0, 0, 0, 7, 6, 0, 0b1011_0000];
        let representation = DataRepresentation::SimplePacking(SimplePackingParams {
            encoded_values: 2,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            bits_per_value: 8,
            original_field_type: 0,
        });

        let bitmap = bitmap_payload(&bitmap_section).unwrap();
        let err = decode_field(&data_section, &representation, bitmap, 4).unwrap_err();
        assert!(matches!(
            err,
            Error::DataLengthMismatch {
                expected: 3,
                actual: 2,
            }
        ));
    }

    #[test]
    fn counts_bitmap_present_points_with_partial_bytes() {
        let present = count_bitmap_present_points(&[0b1011_1111], 3).unwrap();
        assert_eq!(present, 2);
    }

    #[test]
    fn unpacks_complex_packing_with_explicit_missing() {
        let params = ComplexPackingParams {
            encoded_values: 4,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            group_reference_bits: 4,
            original_field_type: 0,
            group_splitting_method: 1,
            missing_value_management: 1,
            primary_missing_substitute: u32::MAX,
            secondary_missing_substitute: u32::MAX,
            num_groups: 2,
            group_width_reference: 0,
            group_width_bits: 2,
            group_length_reference: 2,
            group_length_increment: 1,
            true_length_last_group: 2,
            scaled_group_length_bits: 0,
            spatial_differencing: None,
        };

        let values = unpack_complex(&[0x79, 0x90, 0x34], &params).unwrap();
        assert_eq!(values[0], 7.0);
        assert!(values[1].is_nan());
        assert_eq!(values[2], 9.0);
        assert!(values[3].is_nan());
    }

    #[test]
    fn unpacks_complex_packing_applies_decimal_scale_to_reference_and_values() {
        let params = ComplexPackingParams {
            encoded_values: 2,
            reference_value: 10.0,
            binary_scale: 0,
            decimal_scale: 1,
            group_reference_bits: 4,
            original_field_type: 0,
            group_splitting_method: 1,
            missing_value_management: 0,
            primary_missing_substitute: u32::MAX,
            secondary_missing_substitute: u32::MAX,
            num_groups: 1,
            group_width_reference: 4,
            group_width_bits: 0,
            group_length_reference: 2,
            group_length_increment: 1,
            true_length_last_group: 2,
            scaled_group_length_bits: 0,
            spatial_differencing: None,
        };

        let values = unpack_complex(&[0x10, 0x02], &params).unwrap();
        assert!((values[0] - 1.1).abs() < 1e-12);
        assert!((values[1] - 1.3).abs() < 1e-12);
    }

    #[test]
    fn unpacks_complex_packing_with_second_order_spatial_differencing() {
        let params = ComplexPackingParams {
            encoded_values: 4,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            group_reference_bits: 1,
            original_field_type: 0,
            group_splitting_method: 1,
            missing_value_management: 0,
            primary_missing_substitute: u32::MAX,
            secondary_missing_substitute: u32::MAX,
            num_groups: 1,
            group_width_reference: 1,
            group_width_bits: 0,
            group_length_reference: 4,
            group_length_increment: 1,
            true_length_last_group: 4,
            scaled_group_length_bits: 0,
            spatial_differencing: Some(SpatialDifferencingParams {
                order: 2,
                descriptor_octets: 2,
            }),
        };

        let values =
            unpack_complex(&[0x00, 0x0A, 0x00, 0x0D, 0x00, 0x03, 0x00, 0x10], &params).unwrap();
        assert_eq!(values, vec![10.0, 13.0, 19.0, 29.0]);
    }

    #[test]
    fn unpacks_complex_packing_with_spatial_differencing_and_missing_values() {
        let params = ComplexPackingParams {
            encoded_values: 4,
            reference_value: 0.0,
            binary_scale: 0,
            decimal_scale: 0,
            group_reference_bits: 1,
            original_field_type: 0,
            group_splitting_method: 1,
            missing_value_management: 1,
            primary_missing_substitute: u32::MAX,
            secondary_missing_substitute: u32::MAX,
            num_groups: 1,
            group_width_reference: 2,
            group_width_bits: 0,
            group_length_reference: 4,
            group_length_increment: 1,
            true_length_last_group: 4,
            scaled_group_length_bits: 0,
            spatial_differencing: Some(SpatialDifferencingParams {
                order: 1,
                descriptor_octets: 2,
            }),
        };

        let values = unpack_complex(&[0x00, 0x0A, 0x00, 0x03, 0x00, 0x32], &params).unwrap();
        assert_eq!(values[0], 10.0);
        assert!(values[1].is_nan());
        assert_eq!(values[2], 13.0);
        assert_eq!(values[3], 18.0);
    }

    #[cfg(feature = "png")]
    fn encode_png(
        width: u32,
        height: u32,
        color_type: png::ColorType,
        bit_depth: png::BitDepth,
        data: &[u8],
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut payload, width, height);
            encoder.set_color(color_type);
            encoder.set_depth(bit_depth);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(data).unwrap();
        }
        payload
    }

    #[cfg(feature = "png")]
    fn assert_float_values(actual: &[f64], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-12,
                "expected {expected}, got {actual}"
            );
        }
    }
}
