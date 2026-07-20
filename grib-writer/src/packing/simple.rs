use grib_core::bit::BitWriter;
use grib_core::{DataRepresentation, ScaledPackingParams};

use super::{
    pack_bitmap, present_mask, quantize_present_values, simple_packing_deltas, PackedField,
};
use crate::Result;

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
