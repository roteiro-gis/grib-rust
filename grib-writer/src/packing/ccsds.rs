#[cfg(feature = "ccsds")]
use grib_aec::{encode as aec_encode, AecFlags, AecParams};
#[cfg(feature = "ccsds")]
use grib_core::{filled_vec, CcsdsPackingParams, DataRepresentation, ScaledPackingParams};
use grib_core::{CcsdsBlockSize, CcsdsFlags};

use super::PackedField;
#[cfg(feature = "ccsds")]
use super::{
    bits_needed, pack_bitmap, present_mask, quantize_present_values, simple_packing_deltas,
};
use crate::{Error, Result};

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
    let mut bits = bits_needed(max_delta);
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
