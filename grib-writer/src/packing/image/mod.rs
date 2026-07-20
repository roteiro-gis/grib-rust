#[cfg(any(feature = "jpeg2000", feature = "png"))]
mod codec;

#[cfg(any(feature = "jpeg2000", feature = "png"))]
use self::codec::prepare_image_packing;
#[cfg(feature = "jpeg2000")]
use self::codec::{encode_jpeg2000_payload, jpeg2000_bits_per_value};
#[cfg(feature = "png")]
use self::codec::{encode_png_payload, png_bits_per_value};
#[cfg(any(feature = "jpeg2000", feature = "png"))]
use grib_core::DataRepresentation;
use grib_core::GridDefinition;
#[cfg(feature = "jpeg2000")]
use grib_core::Jpeg2000PackingParams;
#[cfg(feature = "png")]
use grib_core::PngPackingParams;

use super::PackedField;
#[cfg(any(not(feature = "jpeg2000"), not(feature = "png")))]
use crate::Error;
use crate::Result;

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
