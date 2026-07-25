use super::super::bits_needed;
use crate::{Error, Result};
use std::ops::Range;

const COMPLEX_AUTO_GROUP_LEN: usize = 32;

#[derive(Debug, Clone)]
pub(super) struct ComplexGroup {
    pub(super) reference: u64,
    pub(super) width: u8,
    pub(super) value_range: Range<usize>,
}

pub(super) fn complex_groups(deltas: &[u64]) -> Result<Vec<ComplexGroup>> {
    if deltas.is_empty() {
        return Ok(vec![ComplexGroup {
            reference: 0,
            width: 0,
            value_range: 0..0,
        }]);
    }

    let group_len = complex_group_len(deltas.len());
    let group_count = deltas.len().div_ceil(group_len);
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(group_count)
        .map_err(|error| Error::allocation("complex packing groups", group_count, error))?;
    for start in (0..deltas.len()).step_by(group_len) {
        let end = start.saturating_add(group_len).min(deltas.len());
        groups.push(complex_group(deltas, start..end)?);
    }
    Ok(groups)
}

pub(super) fn complex_scan_line_groups(
    deltas: &[u64],
    group_lengths: &[usize],
) -> Result<Vec<ComplexGroup>> {
    let declared_values = group_lengths.iter().try_fold(0usize, |total, length| {
        total.checked_add(*length).ok_or(Error::ArithmeticOverflow {
            operation: "summing row-wise complex group lengths",
        })
    })?;
    if declared_values != deltas.len() {
        return Err(Error::DataLengthMismatch {
            expected: deltas.len(),
            actual: declared_values,
        });
    }

    let mut groups = Vec::new();
    groups
        .try_reserve_exact(group_lengths.len())
        .map_err(|error| {
            Error::allocation(
                "row-wise complex packing groups",
                group_lengths.len(),
                error,
            )
        })?;
    let mut start = 0usize;
    for length in group_lengths {
        let end = start
            .checked_add(*length)
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing a row-wise complex group boundary",
            })?;
        groups.push(complex_group(deltas, start..end)?);
        start = end;
    }
    Ok(groups)
}

fn complex_group(deltas: &[u64], value_range: Range<usize>) -> Result<ComplexGroup> {
    let values = &deltas[value_range.clone()];
    let reference = values.iter().copied().min().unwrap_or(0);
    let max_value = values.iter().copied().max().unwrap_or(reference);
    if max_value > i64::MAX as u64 {
        return Err(Error::ValueOutOfRange(
            "complex packing value exceeds i64 decoder range".into(),
        ));
    }
    Ok(ComplexGroup {
        reference,
        width: bits_needed(max_value - reference),
        value_range,
    })
}

pub(super) fn complex_group_length_reference(value_count: usize) -> u32 {
    complex_group_len(value_count) as u32
}

pub(super) fn complex_true_length_last_group(value_count: usize) -> u32 {
    if value_count == 0 {
        return 0;
    }

    let group_len = complex_group_len(value_count);
    let remainder = value_count % group_len;
    let length = if remainder == 0 { group_len } else { remainder };
    length as u32
}

fn complex_group_len(value_count: usize) -> usize {
    COMPLEX_AUTO_GROUP_LEN.min(value_count)
}
