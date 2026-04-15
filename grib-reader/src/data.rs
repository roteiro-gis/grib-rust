//! Data Representation Section (Section 5) and Data Section (Section 7) decoding.

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

    let layout = GroupReaderLayout::new(reader.bit_offset, params)?;
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
    let byte_index = index / 8;
    let bit_index = index % 8;
    let byte = bitmap_payload
        .get(byte_index)
        .copied()
        .ok_or(Error::DataLengthMismatch {
            expected: byte_index + 1,
            actual: bitmap_payload.len(),
        })?;
    Ok(((byte >> (7 - bit_index)) & 1) != 0)
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

    fn with_offset(data: &'a [u8], bit_offset: usize) -> Self {
        Self { data, bit_offset }
    }

    fn read(&mut self, bit_count: usize) -> Result<u64> {
        if bit_count == 0 {
            return Ok(0);
        }

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

    fn read_signed(&mut self, bit_count: usize) -> Result<i64> {
        let value = self.read(bit_count)?;
        if bit_count == 0 {
            return Ok(0);
        }

        let sign_mask = 1u64 << (bit_count - 1);
        if value & sign_mask == 0 {
            return i64::try_from(value)
                .map_err(|_| Error::Other("signed value exceeds i64 range".into()));
        }

        let magnitude_mask = sign_mask - 1;
        let magnitude = value & magnitude_mask;
        let magnitude = i64::try_from(magnitude)
            .map_err(|_| Error::Other("signed value exceeds i64 range".into()))?;
        Ok(-magnitude)
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
        bitmap_payload, count_bitmap_present_points, decode_field, unpack_complex, unpack_simple,
        ComplexPackingParams, DataRepresentation, SimplePackingParams, SpatialDifferencingParams,
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
}
