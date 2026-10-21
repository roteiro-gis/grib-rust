mod ccsds;
mod complex;
mod ieee;
mod image;
mod simple;

pub(crate) use ccsds::pack_ccsds_auto;
pub(crate) use complex::{pack_complex_auto, pack_complex_row_by_row};
pub(crate) use ieee::pack_ieee;
pub(crate) use image::{pack_jpeg2000_auto, pack_png_auto};
pub(crate) use simple::pack_simple_auto;

use grib_core::bit::BitWriter;
use grib_core::{CcsdsBlockSize, CcsdsFlags, DataRepresentation, GridDefinition, IeeePrecision};

use crate::{Error, Result};

/// Field packing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackingStrategy {
    /// Simple packing with binary scale 0 and automatic bit-width selection.
    SimpleAuto { decimal_scale: i16 },
    /// GRIB2 complex packing with fixed-size general groups.
    ComplexAuto {
        decimal_scale: i16,
        spatial_differencing: Option<SpatialDifferencingOrder>,
    },
    /// GRIB2 complex packing with one group per grid scan line.
    ///
    /// This emits Code Table 5.4 group splitting method 0. Bitmap omissions
    /// reduce each line's group length, and an entirely missing line remains a
    /// zero-length group.
    ComplexRowByRow {
        decimal_scale: i16,
        spatial_differencing: Option<SpatialDifferencingOrder>,
    },
    /// GRIB2 IEEE floating-point packing template 5.4.
    ///
    /// Without an explicit bitmap, NaN values are treated as missing. An
    /// explicit bitmap can mark NaN and infinity as present because the IEEE
    /// template represents them directly.
    Ieee { precision: IeeePrecision },
    /// GRIB2 lossless JPEG 2000 code stream packing template 5.40.
    ///
    /// The OpenJPEG encoder supports automatically selected component
    /// precision through 25 bits. Wider fields return
    /// [`Error::UnsupportedPackingWidth`].
    Jpeg2000Auto { decimal_scale: i16 },
    /// GRIB2 PNG image packing template 5.41.
    PngAuto { decimal_scale: i16 },
    /// GRIB2 CCSDS/AEC packing template 5.42.
    ///
    /// The interoperable baseline uses [`CcsdsFlags::DEFAULT`],
    /// [`CcsdsBlockSize::THIRTY_TWO`], and reference-sample interval 128.
    /// [`CcsdsFlags::PAD_RSI`] is decode-only and is rejected by the writer.
    CcsdsAuto {
        decimal_scale: i16,
        flags: CcsdsFlags,
        block_size: CcsdsBlockSize,
        reference_sample_interval: u16,
    },
}

/// Spatial differencing order for GRIB2 complex packing template 5.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialDifferencingOrder {
    /// First-order spatial differencing.
    First,
    /// Second-order spatial differencing.
    Second,
}

#[derive(Debug, Clone)]
pub(crate) struct PackedField {
    pub(crate) representation: DataRepresentation,
    pub(crate) bitmap_payload: Option<Vec<u8>>,
    pub(crate) data_payload: Vec<u8>,
}

fn quantize_present_values(
    values: &[f64],
    present: &[bool],
    decimal_scale: i16,
    packing_name: &str,
) -> Result<Vec<f64>> {
    let decimal_factor = 10.0_f64.powi(i32::from(decimal_scale));
    if !decimal_factor.is_finite() || decimal_factor <= 0.0 {
        return Err(Error::ValueOutOfRange(format!(
            "invalid decimal scale for {packing_name}: {decimal_scale}"
        )));
    }

    values
        .iter()
        .zip(present)
        .filter_map(|(value, present)| present.then_some(*value))
        .map(|value| {
            if !value.is_finite() {
                return Err(Error::ValueOutOfRange(format!(
                    "present values must be finite for {packing_name}"
                )));
            }
            let scaled = value * decimal_factor;
            if !scaled.is_finite() {
                return Err(Error::ArithmeticOverflow {
                    operation: "applying a decimal packing scale",
                });
            }
            Ok(scaled.round())
        })
        .collect()
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
                    return Err(Error::ValueOutOfRange(
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
                    Err(Error::ValueOutOfRange(
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
    let reference_value = f32_not_greater_than(min_value).ok_or_else(|| {
        Error::ValueOutOfRange(format!(
            "simple-packing reference value {min_value} does not fit IEEE f32"
        ))
    })?;
    let reference = f64::from(reference_value);

    let mut deltas = Vec::with_capacity(quantized.len());
    for value in quantized {
        let delta = (value - reference).round();
        if !delta.is_finite() || delta < 0.0 || delta > u64::MAX as f64 {
            return Err(Error::ValueOutOfRange(
                "packed simple-packing delta does not fit in u64".into(),
            ));
        }
        deltas.push(delta as u64);
    }

    Ok((reference_value, deltas))
}

fn bits_needed(value: u64) -> u8 {
    let bits = if value == 0 {
        0
    } else {
        u64::BITS - value.leading_zeros()
    };
    bits as u8
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
