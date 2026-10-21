mod grouping;
mod spatial;

use grib_core::bit::BitWriter;
use grib_core::{ComplexPackingParams, DataRepresentation, GridDefinition};

use self::grouping::{
    complex_group_length_reference, complex_groups, complex_scan_line_groups,
    complex_true_length_last_group,
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
    pack_complex(
        values,
        explicit_bitmap,
        decimal_scale,
        spatial_differencing,
        None,
    )
}

pub(crate) fn pack_complex_row_by_row(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    grid: &GridDefinition,
    decimal_scale: i16,
    spatial_differencing: Option<SpatialDifferencingOrder>,
) -> Result<PackedField> {
    let present = present_mask(values, explicit_bitmap)?;
    let group_lengths = scan_line_group_lengths(grid, values.len(), &present)?;
    pack_complex_from_present(
        values,
        present,
        decimal_scale,
        spatial_differencing,
        Some(group_lengths),
    )
}

fn pack_complex(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    decimal_scale: i16,
    spatial_differencing: Option<SpatialDifferencingOrder>,
    group_lengths: Option<Vec<usize>>,
) -> Result<PackedField> {
    let present = present_mask(values, explicit_bitmap)?;
    pack_complex_from_present(
        values,
        present,
        decimal_scale,
        spatial_differencing,
        group_lengths,
    )
}

fn pack_complex_from_present(
    values: &[f64],
    present: Vec<bool>,
    decimal_scale: i16,
    spatial_differencing: Option<SpatialDifferencingOrder>,
    group_lengths: Option<Vec<usize>>,
) -> Result<PackedField> {
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
    let groups = match group_lengths.as_deref() {
        Some(lengths) => complex_scan_line_groups(packed_values, lengths)?,
        None => complex_groups(packed_values)?,
    };
    let max_group_reference = groups
        .iter()
        .map(|group| group.reference)
        .max()
        .unwrap_or(0);
    let max_group_width = groups.iter().map(|group| group.width).max().unwrap_or(0);
    let group_reference_bits = bits_needed(max_group_reference);
    let group_width_bits = bits_needed(u64::from(max_group_width));
    let row_by_row = group_lengths.is_some();
    let group_length_reference = if row_by_row {
        0
    } else {
        complex_group_length_reference(present_count)
    };
    let group_length_increment = u8::from(!row_by_row);
    let true_length_last_group = if row_by_row {
        0
    } else {
        complex_true_length_last_group(present_count)
    };

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
        for value in &packed_values[group.value_range.clone()] {
            writer.write(
                value
                    .checked_sub(group.reference)
                    .ok_or(Error::ArithmeticOverflow {
                        operation: "subtracting a complex-packing group reference",
                    })?,
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
        group_splitting_method: u8::from(!row_by_row),
        missing_value_management: 0,
        primary_missing_substitute: u32::MAX,
        secondary_missing_substitute: u32::MAX,
        num_groups: groups.len(),
        group_width_reference: 0,
        group_width_bits,
        group_length_reference,
        group_length_increment,
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

fn scan_line_group_lengths(
    grid: &GridDefinition,
    value_count: usize,
    present: &[bool],
) -> Result<Vec<usize>> {
    let (points_per_line, number_of_lines) = grid.scan_line_shape()?;
    let expected_values =
        points_per_line
            .checked_mul(number_of_lines)
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing row-wise complex grid size",
            })?;
    if value_count != expected_values {
        return Err(Error::DataLengthMismatch {
            expected: expected_values,
            actual: value_count,
        });
    }
    if present.len() != value_count {
        return Err(Error::DataLengthMismatch {
            expected: value_count,
            actual: present.len(),
        });
    }
    if points_per_line == 0 && number_of_lines != 0 {
        return Err(Error::ValueOutOfRange(
            "row-wise complex packing requires non-empty scan lines".into(),
        ));
    }

    let mut lengths = Vec::new();
    lengths
        .try_reserve_exact(number_of_lines)
        .map_err(|error| {
            Error::allocation("row-wise complex group lengths", number_of_lines, error)
        })?;
    for line in present.chunks(points_per_line.max(1)) {
        lengths.push(line.iter().filter(|value| **value).count());
    }
    if lengths.len() != number_of_lines {
        return Err(Error::DataLengthMismatch {
            expected: number_of_lines,
            actual: lengths.len(),
        });
    }
    Ok(lengths)
}
