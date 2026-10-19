mod grouping;
mod spatial;

use grib_core::bit::BitWriter;
use grib_core::{ComplexPackingParams, DataRepresentation};

use self::grouping::{
    complex_group_length_reference, complex_groups, complex_true_length_last_group,
};
use self::spatial::{spatially_difference_values, write_spatial_descriptors};
use super::{
    bits_needed, pack_bitmap, present_mask, quantize_present_values, simple_packing_deltas,
    PackedField, SpatialDifferencingOrder,
};
use crate::{Error, Result};

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
