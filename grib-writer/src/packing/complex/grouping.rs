use super::super::bits_needed;
use crate::{Error, Result};

const COMPLEX_AUTO_GROUP_LEN: usize = 32;

#[derive(Debug, Clone)]
pub(super) struct ComplexGroup {
    pub(super) reference: u64,
    pub(super) width: u8,
    pub(super) values: Vec<u64>,
}

pub(super) fn complex_groups(deltas: &[u64]) -> Result<Vec<ComplexGroup>> {
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

pub(super) fn complex_group_length_reference(value_count: usize) -> Result<u32> {
    u32::try_from(complex_group_len(value_count))
        .map_err(|_| Error::Other("complex group length exceeds u32".into()))
}

pub(super) fn complex_true_length_last_group(value_count: usize) -> Result<u32> {
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
