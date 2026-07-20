use grib_core::bit::BitWriter;
use grib_core::{
    CcsdsBlockSize, CcsdsFlags, ComplexPackingParams, DataRepresentation, GridDefinition,
    ScaledPackingParams, SpatialDifferencingParams,
};

#[cfg(feature = "ccsds")]
use grib_aec::{encode as aec_encode, AecFlags, AecParams};
#[cfg(feature = "jpeg2000")]
use grib_core::Jpeg2000PackingParams;
#[cfg(feature = "png")]
use grib_core::PngPackingParams;
#[cfg(feature = "ccsds")]
use grib_core::{filled_vec, CcsdsPackingParams};

use crate::{Error, Result, SpatialDifferencingOrder};

const COMPLEX_AUTO_GROUP_LEN: usize = 32;

#[derive(Debug, Clone)]
pub(crate) struct PackedField {
    pub(crate) representation: DataRepresentation,
    pub(crate) bitmap_payload: Option<Vec<u8>>,
    pub(crate) data_payload: Vec<u8>,
}

pub(crate) fn pack_simple_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    decimal_scale: i16,
) -> Result<PackedField> {
    let present = present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };

    let quantized = quantize_present_values(values, &present, decimal_scale, "simple packing")?;
    let (reference_value, deltas) = simple_packing_deltas(&quantized)?;
    let max_delta = deltas.iter().copied().max().unwrap_or(0);
    let bits_per_value = if max_delta == 0 {
        0
    } else {
        (u64::BITS - max_delta.leading_zeros()) as u8
    };

    let mut writer = BitWriter::with_capacity_bits(deltas.len() * usize::from(bits_per_value));
    if bits_per_value > 0 {
        for delta in &deltas {
            writer.write(*delta, usize::from(bits_per_value))?;
        }
        writer.align_to_byte()?;
    }

    let representation = DataRepresentation::SimplePacking(ScaledPackingParams {
        encoded_values: present_count,
        reference_value,
        binary_scale: 0,
        decimal_scale,
        bits_per_value,
        original_field_type: 0,
    });

    Ok(PackedField {
        representation,
        bitmap_payload,
        data_payload: writer.into_bytes(),
    })
}

pub(crate) fn pack_complex_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    decimal_scale: i16,
    spatial_differencing: Option<SpatialDifferencingOrder>,
) -> Result<PackedField> {
    let present = present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };

    let quantized = quantize_present_values(values, &present, decimal_scale, "complex packing")?;
    let (reference_value, deltas) = simple_packing_deltas(&quantized)?;
    let spatial_packing = spatial_differencing
        .map(|order| spatially_difference_values(&deltas, order))
        .transpose()?;
    let packed_values = spatial_packing
        .as_ref()
        .map_or(deltas.as_slice(), |spatial| spatial.values.as_slice());
    let groups = complex_groups(packed_values)?;
    let max_group_reference = groups
        .iter()
        .map(|group| group.reference)
        .max()
        .unwrap_or(0);
    let max_group_width = groups.iter().map(|group| group.width).max().unwrap_or(0);
    let group_reference_bits = bits_needed(max_group_reference)?;
    let group_width_bits = bits_needed(u64::from(max_group_width))?;
    let group_length_reference = complex_group_length_reference(present_count)?;
    let true_length_last_group = complex_true_length_last_group(present_count)?;

    let mut writer = BitWriter::new();
    if let Some(spatial) = &spatial_packing {
        write_spatial_descriptors(&mut writer, spatial)?;
    }
    for group in &groups {
        writer.write(group.reference, usize::from(group_reference_bits))?;
    }
    writer.align_to_byte()?;
    for group in &groups {
        writer.write(u64::from(group.width), usize::from(group_width_bits))?;
    }
    writer.align_to_byte()?;
    for group in &groups {
        for value in &group.values {
            writer.write(
                value
                    .checked_sub(group.reference)
                    .ok_or_else(|| Error::Other("complex group value underflow".into()))?,
                usize::from(group.width),
            )?;
        }
    }
    writer.align_to_byte()?;

    let representation = DataRepresentation::ComplexPacking(ComplexPackingParams {
        encoded_values: present_count,
        reference_value,
        binary_scale: 0,
        decimal_scale,
        group_reference_bits,
        original_field_type: 0,
        group_splitting_method: 1,
        missing_value_management: 0,
        primary_missing_substitute: u32::MAX,
        secondary_missing_substitute: u32::MAX,
        num_groups: groups.len(),
        group_width_reference: 0,
        group_width_bits,
        group_length_reference,
        group_length_increment: 1,
        true_length_last_group,
        scaled_group_length_bits: 0,
        spatial_differencing: spatial_packing.as_ref().map(|spatial| spatial.params),
    });

    Ok(PackedField {
        representation,
        bitmap_payload,
        data_payload: writer.into_bytes(),
    })
}

#[cfg(feature = "jpeg2000")]
pub(crate) fn pack_jpeg2000_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    grid: &GridDefinition,
    decimal_scale: i16,
) -> Result<PackedField> {
    let prepared = prepare_image_packing(
        values,
        explicit_bitmap,
        grid,
        decimal_scale,
        "JPEG2000 packing",
        jpeg2000_bits_per_value,
    )?;
    let data_payload = encode_jpeg2000_payload(
        &prepared.deltas,
        prepared.params.bits_per_value,
        prepared.dimensions,
    )?;

    Ok(PackedField {
        representation: DataRepresentation::Jpeg2000Packing(Jpeg2000PackingParams {
            packing: prepared.params,
            compression_type: 0,
            target_compression_ratio: 0,
        }),
        bitmap_payload: prepared.bitmap_payload,
        data_payload,
    })
}

#[cfg(not(feature = "jpeg2000"))]
pub(crate) fn pack_jpeg2000_auto(
    _values: &[f64],
    _explicit_bitmap: Option<&[bool]>,
    _grid: &GridDefinition,
    _decimal_scale: i16,
) -> Result<PackedField> {
    Err(Error::UnsupportedDataTemplate(40))
}

#[cfg(feature = "png")]
pub(crate) fn pack_png_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    grid: &GridDefinition,
    decimal_scale: i16,
) -> Result<PackedField> {
    let prepared = prepare_image_packing(
        values,
        explicit_bitmap,
        grid,
        decimal_scale,
        "PNG packing",
        png_bits_per_value,
    )?;
    let data_payload = encode_png_payload(
        &prepared.deltas,
        prepared.params.bits_per_value,
        prepared.dimensions,
    )?;

    Ok(PackedField {
        representation: DataRepresentation::PngPacking(PngPackingParams {
            packing: prepared.params,
        }),
        bitmap_payload: prepared.bitmap_payload,
        data_payload,
    })
}

#[cfg(not(feature = "png"))]
pub(crate) fn pack_png_auto(
    _values: &[f64],
    _explicit_bitmap: Option<&[bool]>,
    _grid: &GridDefinition,
    _decimal_scale: i16,
) -> Result<PackedField> {
    Err(Error::UnsupportedDataTemplate(41))
}

#[cfg(feature = "ccsds")]
pub(crate) fn pack_ccsds_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    decimal_scale: i16,
    flags: CcsdsFlags,
    block_size: CcsdsBlockSize,
    reference_sample_interval: u16,
) -> Result<PackedField> {
    if flags.contains(CcsdsFlags::PAD_RSI) {
        return Err(Error::ValueOutOfRange(
            "CCSDS PAD_RSI is a legacy decode-only option".into(),
        ));
    }

    let present = present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };
    let quantized = quantize_present_values(values, &present, decimal_scale, "CCSDS packing")?;
    let (reference_value, deltas) = ccsds_packing_deltas(&quantized, decimal_scale)?;
    let max_delta = deltas.iter().copied().max().unwrap_or(0);
    let bits_per_value = ccsds_bits_per_value(max_delta, flags)?;
    if bits_per_value > 32 {
        return Err(Error::UnsupportedPackingWidth(bits_per_value));
    }

    let params = CcsdsPackingParams::new(
        ScaledPackingParams {
            encoded_values: present_count,
            reference_value,
            binary_scale: 0,
            decimal_scale,
            bits_per_value,
            original_field_type: 0,
        },
        flags,
        block_size,
        reference_sample_interval,
    )?;
    let data_payload = if bits_per_value == 0 {
        Vec::new()
    } else {
        encode_ccsds_payload(&deltas, &params)?
    };

    Ok(PackedField {
        representation: DataRepresentation::CcsdsPacking(params),
        bitmap_payload,
        data_payload,
    })
}

#[cfg(not(feature = "ccsds"))]
pub(crate) fn pack_ccsds_auto(
    _values: &[f64],
    _explicit_bitmap: Option<&[bool]>,
    _decimal_scale: i16,
    _flags: CcsdsFlags,
    _block_size: CcsdsBlockSize,
    _reference_sample_interval: u16,
) -> Result<PackedField> {
    Err(Error::UnsupportedDataTemplate(42))
}

#[cfg(feature = "ccsds")]
fn ccsds_bits_per_value(max_delta: u64, flags: CcsdsFlags) -> Result<u8> {
    let mut bits = bits_needed(max_delta)?;
    if bits == 0 || !flags.contains(CcsdsFlags::SIGNED) {
        return Ok(bits);
    }

    let container_bits = match bits {
        1..=8 => 8,
        9..=16 => 16,
        17..=32 => 32,
        _ => return Err(Error::UnsupportedPackingWidth(bits)),
    };
    if bits < container_bits {
        // libaec sign-extends widths smaller than their sample container.
        // ecCodes expands 17--24-bit samples to four-byte containers, even
        // when the template advertises THREE_BYTE. Reserve a zero sign bit so
        // GRIB's non-negative packed differences retain the same numeric value
        // in both compact libaec clients and ecCodes' canonical layout.
        bits = bits
            .checked_add(1)
            .ok_or(Error::UnsupportedPackingWidth(bits))?;
    }
    Ok(bits)
}

#[cfg(feature = "ccsds")]
fn ccsds_packing_deltas(quantized: &[f64], decimal_scale: i16) -> Result<(f32, Vec<u64>)> {
    let Some(&first) = quantized.first() else {
        return Ok((0.0, Vec::new()));
    };
    if quantized.iter().all(|value| *value == first) {
        let decimal_factor = 10.0_f64.powi(-i32::from(decimal_scale));
        let constant = first * decimal_factor;
        let reference_value = constant as f32;
        if !constant.is_finite() || !reference_value.is_finite() {
            return Err(Error::ValueOutOfRange(
                "constant CCSDS reference value does not fit IEEE f32".into(),
            ));
        }
        return Ok((
            reference_value,
            filled_vec(quantized.len(), 0_u64, "constant CCSDS packed")?,
        ));
    }

    simple_packing_deltas(quantized)
}

#[cfg(feature = "ccsds")]
fn encode_ccsds_payload(deltas: &[u64], params: &CcsdsPackingParams) -> Result<Vec<u8>> {
    let bits_per_value = params.packing().bits_per_value;
    let bytes_per_sample = ccsds_bytes_per_sample(bits_per_value, params.flags())?;
    let byte_count =
        deltas
            .len()
            .checked_mul(bytes_per_sample)
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing CCSDS input byte count",
            })?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(byte_count)
        .map_err(|error| Error::allocation("bytes for CCSDS input samples", byte_count, error))?;
    for &delta in deltas {
        let sample =
            u32::try_from(delta).map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?;
        if params.flags().contains(CcsdsFlags::MSB) {
            samples.extend_from_slice(&sample.to_be_bytes()[4 - bytes_per_sample..]);
        } else {
            samples.extend_from_slice(&sample.to_le_bytes()[..bytes_per_sample]);
        }
    }

    let codec_params = AecParams::new(
        bits_per_value,
        params.block_size().samples(),
        params.reference_sample_interval(),
        ccsds_encoder_flags(params.flags()),
    )
    .map_err(|error| Error::Codec {
        codec: "CCSDS/AEC",
        operation: "configure",
        reason: error.to_string(),
    })?;
    aec_encode(&samples, codec_params).map_err(|error| Error::Codec {
        codec: "CCSDS/AEC",
        operation: "encode",
        reason: error.to_string(),
    })
}

#[cfg(feature = "ccsds")]
fn ccsds_bytes_per_sample(bits_per_value: u8, flags: CcsdsFlags) -> Result<usize> {
    match bits_per_value {
        1..=8 => Ok(1),
        9..=16 => Ok(2),
        17..=24 if flags.contains(CcsdsFlags::THREE_BYTE) => Ok(3),
        17..=32 => Ok(4),
        bits => Err(Error::UnsupportedPackingWidth(bits)),
    }
}

#[cfg(feature = "ccsds")]
fn ccsds_encoder_flags(flags: CcsdsFlags) -> AecFlags {
    let mut codec_flags = AecFlags::NONE;
    for (wire_flag, codec_flag) in [
        (CcsdsFlags::SIGNED, AecFlags::DATA_SIGNED),
        (CcsdsFlags::THREE_BYTE, AecFlags::DATA_3BYTE),
        (CcsdsFlags::MSB, AecFlags::DATA_MSB),
        (CcsdsFlags::PREPROCESS, AecFlags::DATA_PREPROCESS),
        (CcsdsFlags::RESTRICTED, AecFlags::RESTRICTED),
        (CcsdsFlags::NOT_ENFORCE, AecFlags::NOT_ENFORCE),
    ] {
        if flags.contains(wire_flag) {
            codec_flags |= codec_flag;
        }
    }
    codec_flags
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
#[derive(Debug, Clone)]
struct PreparedImagePacking {
    params: ScaledPackingParams,
    bitmap_payload: Option<Vec<u8>>,
    deltas: Vec<u64>,
    dimensions: ImageDimensions,
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImageDimensions {
    width: u32,
    height: u32,
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn prepare_image_packing(
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
        return Err(Error::Other(format!(
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
    let minimum_bits = bits_needed(max_delta)?.max(1);
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
            width: u32::try_from(width)
                .map_err(|_| Error::Other("image width exceeds u32".into()))?,
            height: u32::try_from(height)
                .map_err(|_| Error::Other("image height exceeds u32".into()))?,
        });
    }

    Ok(ImageDimensions {
        width: u32::try_from(present_count)
            .map_err(|_| Error::Other("image width exceeds u32".into()))?,
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
fn jpeg2000_bits_per_value(minimum_bits: u8) -> Result<u8> {
    if (1..=31).contains(&minimum_bits) {
        Ok(minimum_bits)
    } else {
        Err(Error::UnsupportedPackingWidth(minimum_bits))
    }
}

#[cfg(feature = "png")]
fn png_bits_per_value(minimum_bits: u8) -> Result<u8> {
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
fn encode_png_payload(
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
        let mut writer = encoder
            .write_header()
            .map_err(|err| Error::Other(format!("PNG encode failed: {err}")))?;
        writer
            .write_image_data(&image_data)
            .map_err(|err| Error::Other(format!("PNG encode failed: {err}")))?;
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
            let mut data = Vec::with_capacity(deltas.len() * 2);
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
    let width =
        usize::try_from(dimensions.width).map_err(|_| Error::Other("PNG width overflow".into()))?;
    let height = usize::try_from(dimensions.height)
        .map_err(|_| Error::Other("PNG height overflow".into()))?;
    let bits = usize::from(bits_per_value);
    let row_bits = width
        .checked_mul(bits)
        .ok_or_else(|| Error::Other("PNG row width overflow".into()))?;
    let row_bytes = row_bits.div_ceil(8);
    let mut data = vec![
        0;
        row_bytes
            .checked_mul(height)
            .ok_or_else(|| Error::Other("PNG data length overflow".into()))?
    ];

    for (index, delta) in deltas.iter().copied().enumerate() {
        let row = index / width;
        let column = index % width;
        let bit_offset = column
            .checked_mul(bits)
            .ok_or_else(|| Error::Other("PNG bit offset overflow".into()))?;
        let byte_index = row
            .checked_mul(row_bytes)
            .and_then(|row_offset| row_offset.checked_add(bit_offset / 8))
            .ok_or_else(|| Error::Other("PNG byte offset overflow".into()))?;
        let shift = 8 - bits - (bit_offset % 8);
        data[byte_index] |= (delta as u8) << shift;
    }

    Ok(data)
}

#[cfg(feature = "png")]
fn pack_png_multibyte_samples(deltas: &[u64], bytes_per_sample: usize) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(
        deltas
            .len()
            .checked_mul(bytes_per_sample)
            .ok_or_else(|| Error::Other("PNG data length overflow".into()))?,
    );
    for delta in deltas {
        let bytes = u32::try_from(*delta)
            .map_err(|_| Error::UnsupportedPackingWidth((bytes_per_sample * 8) as u8))?
            .to_be_bytes();
        data.extend_from_slice(&bytes[4 - bytes_per_sample..]);
    }
    Ok(data)
}

#[cfg(feature = "jpeg2000")]
fn encode_jpeg2000_payload(
    deltas: &[u64],
    bits_per_value: u8,
    dimensions: ImageDimensions,
) -> Result<Vec<u8>> {
    validate_image_sample_count(deltas.len(), dimensions)?;

    let component = openjp2::opj_image_comptparm {
        dx: 1,
        dy: 1,
        w: dimensions.width,
        h: dimensions.height,
        prec: u32::from(bits_per_value),
        bpp: u32::from(bits_per_value),
        sgnd: 0,
        ..Default::default()
    };
    let mut image = openjp2::opj_image::create(&[component], openjp2::OPJ_CLRSPC_GRAY)
        .ok_or_else(|| Error::Other("failed to create JPEG2000 image".into()))?;
    image.x1 = dimensions.width;
    image.y1 = dimensions.height;

    let components = image
        .comps_mut()
        .ok_or_else(|| Error::Other("JPEG2000 image has no components".into()))?;
    let component = components
        .get_mut(0)
        .ok_or_else(|| Error::Other("JPEG2000 image has no components".into()))?;
    component.bpp = u32::from(bits_per_value);
    component.prec = u32::from(bits_per_value);
    let data = components
        .get_mut(0)
        .and_then(|component| component.data_mut())
        .ok_or_else(|| Error::Other("JPEG2000 image component has no data".into()))?;
    if data.len() != deltas.len() {
        return Err(Error::DataLengthMismatch {
            expected: deltas.len(),
            actual: data.len(),
        });
    }
    for (target, delta) in data.iter_mut().zip(deltas) {
        *target =
            i32::try_from(*delta).map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?;
    }

    let path = tempfile::Builder::new()
        .prefix("grib-writer-")
        .suffix(".j2k")
        .tempfile()
        .map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))?
        .into_temp_path();

    {
        let mut stream = openjp2::Stream::new_file(&path, 64 * 1024, false)
            .map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))?;
        let mut codec = openjp2::Codec::new_encoder(openjp2::OPJ_CODEC_J2K)
            .ok_or_else(|| Error::Other("failed to create JPEG2000 encoder".into()))?;
        let mut params = openjp2::opj_cparameters_t {
            tcp_numlayers: 1,
            cp_disto_alloc: 1,
            numresolution: jpeg2000_num_resolutions(dimensions),
            ..Default::default()
        };

        if codec.setup_encoder(&mut params, &mut image) == 0 {
            return Err(Error::Other("JPEG2000 encoder setup failed".into()));
        }
        if codec.start_compress(&mut image, &mut stream) == 0 {
            return Err(Error::Other("JPEG2000 start-compress failed".into()));
        }
        if codec.encode(&mut stream) == 0 {
            return Err(Error::Other("JPEG2000 codestream encode failed".into()));
        }
        if codec.end_compress(&mut stream) == 0 {
            return Err(Error::Other("JPEG2000 end-compress failed".into()));
        }
        stream
            .flush()
            .map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))?;
    }

    std::fs::read(&path).map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))
}

#[cfg(feature = "jpeg2000")]
fn jpeg2000_num_resolutions(dimensions: ImageDimensions) -> i32 {
    let min_dimension = dimensions.width.min(dimensions.height);
    let mut resolutions = 1;
    while resolutions < 32 && min_dimension >= (1u32 << resolutions) {
        resolutions += 1;
    }
    resolutions
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn validate_image_sample_count(sample_count: usize, dimensions: ImageDimensions) -> Result<()> {
    let width = usize::try_from(dimensions.width)
        .map_err(|_| Error::Other("image width overflow".into()))?;
    let height = usize::try_from(dimensions.height)
        .map_err(|_| Error::Other("image height overflow".into()))?;
    let expected = width
        .checked_mul(height)
        .ok_or_else(|| Error::Other("image sample count overflow".into()))?;
    if sample_count != expected {
        return Err(Error::DataLengthMismatch {
            expected,
            actual: sample_count,
        });
    }
    Ok(())
}

fn quantize_present_values(
    values: &[f64],
    present: &[bool],
    decimal_scale: i16,
    packing_name: &str,
) -> Result<Vec<f64>> {
    let decimal_factor = 10.0_f64.powi(i32::from(decimal_scale));
    if !decimal_factor.is_finite() || decimal_factor <= 0.0 {
        return Err(Error::Other(format!(
            "invalid decimal scale for {packing_name}: {decimal_scale}"
        )));
    }

    values
        .iter()
        .zip(present)
        .filter_map(|(value, present)| present.then_some(*value))
        .map(|value| {
            if !value.is_finite() {
                return Err(Error::Other(format!(
                    "present values must be finite for {packing_name}"
                )));
            }
            let scaled = value * decimal_factor;
            if !scaled.is_finite() {
                return Err(Error::Other(format!(
                    "scaled value overflow during {packing_name}"
                )));
            }
            Ok(scaled.round())
        })
        .collect()
}

impl SpatialDifferencingOrder {
    const fn grib_order(self) -> u8 {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }

    const fn min_values(self) -> usize {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }
}

#[derive(Debug, Clone)]
struct SpatialPacking {
    params: SpatialDifferencingParams,
    descriptors: SpatialDescriptors,
    values: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
struct SpatialDescriptors {
    first_value: i64,
    second_value: Option<i64>,
    overall_minimum: i64,
}

fn spatially_difference_values(
    values: &[u64],
    order: SpatialDifferencingOrder,
) -> Result<SpatialPacking> {
    if values.len() < order.min_values() {
        return Err(Error::DataLengthMismatch {
            expected: order.min_values(),
            actual: values.len(),
        });
    }

    let values = values
        .iter()
        .copied()
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| Error::Other("spatial differencing value exceeds i64".into()))
        })
        .collect::<Result<Vec<_>>>()?;

    let (descriptors, differenced) = match order {
        SpatialDifferencingOrder::First => first_order_spatial_difference(&values)?,
        SpatialDifferencingOrder::Second => second_order_spatial_difference(&values)?,
    };
    let descriptor_octets = spatial_descriptor_octets(&descriptors)?;

    Ok(SpatialPacking {
        params: SpatialDifferencingParams {
            order: order.grib_order(),
            descriptor_octets,
        },
        descriptors,
        values: differenced,
    })
}

fn first_order_spatial_difference(values: &[i64]) -> Result<(SpatialDescriptors, Vec<u64>)> {
    let mut differences = Vec::with_capacity(values.len().saturating_sub(1));
    for pair in values.windows(2) {
        differences.push(
            pair[1]
                .checked_sub(pair[0])
                .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?,
        );
    }
    let overall_minimum = differences.iter().copied().min().unwrap_or(0);

    let mut differenced = Vec::with_capacity(values.len());
    differenced.push(0);
    for difference in differences {
        differenced.push(spatial_difference_delta(difference, overall_minimum)?);
    }

    Ok((
        SpatialDescriptors {
            first_value: values[0],
            second_value: None,
            overall_minimum,
        },
        differenced,
    ))
}

fn second_order_spatial_difference(values: &[i64]) -> Result<(SpatialDescriptors, Vec<u64>)> {
    let first_value = values[0];
    let second_value = values[1];
    let mut previous_difference = second_value
        .checked_sub(first_value)
        .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
    let mut second_differences = Vec::with_capacity(values.len().saturating_sub(2));

    for index in 2..values.len() {
        let difference = values[index]
            .checked_sub(values[index - 1])
            .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
        second_differences.push(
            difference
                .checked_sub(previous_difference)
                .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?,
        );
        previous_difference = difference;
    }

    let overall_minimum = second_differences.iter().copied().min().unwrap_or(0);
    let mut differenced = Vec::with_capacity(values.len());
    differenced.push(0);
    differenced.push(0);
    for second_difference in second_differences {
        differenced.push(spatial_difference_delta(
            second_difference,
            overall_minimum,
        )?);
    }

    Ok((
        SpatialDescriptors {
            first_value,
            second_value: Some(second_value),
            overall_minimum,
        },
        differenced,
    ))
}

fn spatial_difference_delta(value: i64, overall_minimum: i64) -> Result<u64> {
    let delta = value
        .checked_sub(overall_minimum)
        .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
    u64::try_from(delta)
        .map_err(|_| Error::Other("spatial differencing produced negative delta".into()))
}

fn spatial_descriptor_octets(descriptors: &SpatialDescriptors) -> Result<u8> {
    let values = [
        Some(descriptors.first_value),
        descriptors.second_value,
        Some(descriptors.overall_minimum),
    ];
    for octets in 1..=8 {
        if values
            .iter()
            .flatten()
            .all(|value| signed_magnitude_fits(*value, octets))
        {
            return Ok(octets);
        }
    }

    Err(Error::Other(
        "spatial differencing descriptor exceeds signed-magnitude range".into(),
    ))
}

fn signed_magnitude_fits(value: i64, octets: u8) -> bool {
    signed_magnitude_bits(value, octets).is_ok()
}

fn write_spatial_descriptors(writer: &mut BitWriter, spatial: &SpatialPacking) -> Result<()> {
    let bit_count = usize::from(spatial.params.descriptor_octets) * 8;
    writer.write(
        signed_magnitude_bits(
            spatial.descriptors.first_value,
            spatial.params.descriptor_octets,
        )?,
        bit_count,
    )?;
    if let Some(second_value) = spatial.descriptors.second_value {
        writer.write(
            signed_magnitude_bits(second_value, spatial.params.descriptor_octets)?,
            bit_count,
        )?;
    }
    writer.write(
        signed_magnitude_bits(
            spatial.descriptors.overall_minimum,
            spatial.params.descriptor_octets,
        )?,
        bit_count,
    )
}

fn signed_magnitude_bits(value: i64, octets: u8) -> Result<u64> {
    let bit_count = u32::from(octets) * 8;
    if bit_count == 0 || bit_count > u64::BITS {
        return Err(Error::Other(
            "spatial differencing descriptor width must be 1..=8 octets".into(),
        ));
    }
    let magnitude = value
        .checked_abs()
        .ok_or_else(|| Error::Other("spatial differencing descriptor magnitude overflow".into()))?
        as u64;
    let magnitude_bits = bit_count - 1;
    let max_magnitude = if magnitude_bits == u64::BITS {
        u64::MAX
    } else {
        (1u64 << magnitude_bits) - 1
    };
    if magnitude > max_magnitude {
        return Err(Error::Other(
            "spatial differencing descriptor exceeds signed-magnitude range".into(),
        ));
    }

    let sign_bit = if value < 0 {
        1u64 << (bit_count - 1)
    } else {
        0
    };
    Ok(sign_bit | magnitude)
}

pub(crate) fn reorder_field_to_grib_scan_order(
    grid: &GridDefinition,
    values: &mut [f64],
    bitmap: Option<&mut [bool]>,
) -> Result<()> {
    grid.reorder_for_ndarray_in_place(values)?;
    if let Some(bitmap) = bitmap {
        grid.reorder_for_ndarray_in_place(bitmap)?;
    }
    Ok(())
}

fn present_mask(values: &[f64], explicit_bitmap: Option<&[bool]>) -> Result<Vec<bool>> {
    match explicit_bitmap {
        Some(bitmap) => values
            .iter()
            .zip(bitmap)
            .map(|(value, present)| {
                if *present && !value.is_finite() {
                    return Err(Error::Other(
                        "explicit bitmap marks a non-finite value as present".into(),
                    ));
                }
                Ok(*present)
            })
            .collect(),
        None => values
            .iter()
            .map(|value| {
                if value.is_nan() {
                    Ok(false)
                } else if value.is_finite() {
                    Ok(true)
                } else {
                    Err(Error::Other(
                        "infinite values cannot be written as packed data".into(),
                    ))
                }
            })
            .collect(),
    }
}

fn simple_packing_deltas(quantized: &[f64]) -> Result<(f32, Vec<u64>)> {
    if quantized.is_empty() {
        return Ok((0.0, Vec::new()));
    }

    let min_value = quantized.iter().copied().fold(f64::INFINITY, f64::min);
    let reference_value = f32_not_greater_than(min_value)
        .ok_or_else(|| Error::Other("failed to choose simple-packing reference value".into()))?;
    let reference = f64::from(reference_value);

    let mut deltas = Vec::with_capacity(quantized.len());
    for value in quantized {
        let delta = (value - reference).round();
        if !delta.is_finite() || delta < 0.0 || delta > u64::MAX as f64 {
            return Err(Error::Other(
                "packed simple-packing delta does not fit in u64".into(),
            ));
        }
        deltas.push(delta as u64);
    }

    Ok((reference_value, deltas))
}

#[derive(Debug, Clone)]
struct ComplexGroup {
    reference: u64,
    width: u8,
    values: Vec<u64>,
}

fn complex_groups(deltas: &[u64]) -> Result<Vec<ComplexGroup>> {
    if deltas.is_empty() {
        return Ok(vec![ComplexGroup {
            reference: 0,
            width: 0,
            values: Vec::new(),
        }]);
    }

    let group_len = complex_group_len(deltas.len());
    let mut groups = Vec::with_capacity(deltas.len().div_ceil(group_len));
    for chunk in deltas.chunks(group_len) {
        let reference = chunk.iter().copied().min().unwrap_or(0);
        let max_value = chunk.iter().copied().max().unwrap_or(reference);
        if max_value > i64::MAX as u64 {
            return Err(Error::Other(
                "complex packing value exceeds i64 decoder range".into(),
            ));
        }
        let width = bits_needed(max_value - reference)?;
        groups.push(ComplexGroup {
            reference,
            width,
            values: chunk.to_vec(),
        });
    }
    Ok(groups)
}

fn complex_group_length_reference(value_count: usize) -> Result<u32> {
    u32::try_from(complex_group_len(value_count))
        .map_err(|_| Error::Other("complex group length exceeds u32".into()))
}

fn complex_true_length_last_group(value_count: usize) -> Result<u32> {
    if value_count == 0 {
        return Ok(0);
    }

    let group_len = complex_group_len(value_count);
    let remainder = value_count % group_len;
    let length = if remainder == 0 { group_len } else { remainder };
    u32::try_from(length).map_err(|_| Error::Other("complex group length exceeds u32".into()))
}

fn complex_group_len(value_count: usize) -> usize {
    COMPLEX_AUTO_GROUP_LEN.min(value_count)
}

fn bits_needed(value: u64) -> Result<u8> {
    let bits = if value == 0 {
        0
    } else {
        u64::BITS - value.leading_zeros()
    };
    u8::try_from(bits).map_err(|_| Error::Other("bit width exceeds u8".into()))
}

fn f32_not_greater_than(value: f64) -> Option<f32> {
    if !value.is_finite() || value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return None;
    }

    let mut candidate = value as f32;
    while f64::from(candidate) > value {
        candidate = next_down_f32(candidate)?;
    }
    Some(candidate)
}

fn next_down_f32(value: f32) -> Option<f32> {
    if value.is_nan() || value == f32::NEG_INFINITY {
        return None;
    }
    if value == 0.0 {
        return Some(-f32::from_bits(1));
    }
    let bits = value.to_bits();
    Some(if value.is_sign_positive() {
        f32::from_bits(bits - 1)
    } else {
        f32::from_bits(bits + 1)
    })
}

fn pack_bitmap(present: &[bool]) -> Result<Vec<u8>> {
    let mut writer = BitWriter::with_capacity_bits(present.len());
    for present in present {
        writer.write(u64::from(*present), 1)?;
    }
    writer.align_to_byte()?;
    Ok(writer.into_bytes())
}
