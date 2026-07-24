use grib_core::binary::encode_ieee_binary128;
use grib_core::{DataRepresentation, IeeePackingParams, IeeePrecision};

use super::{pack_bitmap, PackedField};
use crate::{Error, Result};

pub(crate) fn pack_ieee(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    precision: IeeePrecision,
) -> Result<PackedField> {
    let present = ieee_present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    let payload_len = present_count
        .checked_mul(precision.bytes_per_value())
        .ok_or(Error::ArithmeticOverflow {
            operation: "computing IEEE-packed payload length",
        })?;
    let mut data_payload = Vec::new();
    data_payload
        .try_reserve_exact(payload_len)
        .map_err(|error| Error::allocation("IEEE-packed bytes", payload_len, error))?;

    for (&value, &present) in values.iter().zip(&present) {
        if !present {
            continue;
        }
        match precision {
            IeeePrecision::Single => {
                let encoded = value as f32;
                if value.is_finite() && !encoded.is_finite() {
                    return Err(Error::ValueOutOfRange(format!(
                        "value {value} does not fit IEEE binary32"
                    )));
                }
                data_payload.extend_from_slice(&encoded.to_be_bytes());
            }
            IeeePrecision::Double => data_payload.extend_from_slice(&value.to_be_bytes()),
            IeeePrecision::Quadruple => {
                data_payload.extend_from_slice(&encode_ieee_binary128(value));
            }
            _ => return Err(Error::UnsupportedPackingWidth(precision.bits_per_value())),
        }
    }

    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };

    Ok(PackedField {
        representation: DataRepresentation::IeeePacking(IeeePackingParams {
            encoded_values: present_count,
            precision,
        }),
        bitmap_payload,
        data_payload,
    })
}

fn ieee_present_mask(values: &[f64], explicit_bitmap: Option<&[bool]>) -> Result<Vec<bool>> {
    let mut present = Vec::new();
    present
        .try_reserve_exact(values.len())
        .map_err(|error| Error::allocation("IEEE presence values", values.len(), error))?;

    match explicit_bitmap {
        Some(bitmap) => present.extend_from_slice(bitmap),
        None => present.extend(values.iter().map(|value| !value.is_nan())),
    }
    Ok(present)
}
