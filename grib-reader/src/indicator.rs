//! Section 0: Indicator Section parsing for GRIB1 and GRIB2.

use grib_core::binary::read_u24_be;

const GRIB1_LARGE_MESSAGE_FLAG: u32 = 0x80_0000;
const GRIB1_LARGE_MESSAGE_BLOCK_BYTES: u64 = 120;

/// Parsed Indicator Section (Section 0).
#[derive(Debug, Clone)]
pub struct Indicator {
    /// GRIB edition number (1 or 2).
    pub edition: u8,
    /// Discipline (GRIB2 only): 0=Meteorological, 1=Hydrological, 2=Land surface, etc.
    pub discipline: u8,
    /// Total length of the GRIB message in bytes.
    pub total_length: u64,
}

impl Indicator {
    /// Read the GRIB edition byte without interpreting edition-specific length
    /// fields.
    pub fn edition(data: &[u8]) -> Option<u8> {
        if data.len() < 8 || &data[0..4] != b"GRIB" {
            return None;
        }
        Some(data[7])
    }

    /// Parse from the first bytes of a GRIB message.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let edition = Self::edition(data)?;
        match edition {
            1 => {
                let raw_length = read_u24_be(&data[4..7])?;
                let length = resolve_grib1_large_message_length(data, raw_length)
                    .unwrap_or(u64::from(raw_length));
                Some(Self {
                    edition,
                    discipline: 0,
                    total_length: length,
                })
            }
            2 => {
                if data.len() < 16 {
                    return None;
                }
                let discipline = data[6];
                let length = u64::from_be_bytes(data[8..16].try_into().ok()?);
                Some(Self {
                    edition,
                    discipline,
                    total_length: length,
                })
            }
            _ => None,
        }
    }
}

pub(crate) fn grib1_section4_length(data: &[u8], offset: usize) -> Option<usize> {
    let raw_total_length = read_u24_be(data.get(4..7)?)?;
    let raw_section4_length = read_u24_be(data.get(offset..offset.checked_add(3)?)?)?;
    let length = if uses_grib1_large_message_convention(raw_total_length, raw_section4_length) {
        let total_length =
            decode_grib1_large_message_length(raw_total_length, raw_section4_length)?;
        total_length
            .checked_sub(u64::try_from(offset).ok()?)?
            .checked_sub(4)?
    } else {
        u64::from(raw_section4_length)
    };
    usize::try_from(length).ok()
}

fn resolve_grib1_large_message_length(data: &[u8], raw_length: u32) -> Option<u64> {
    if raw_length & GRIB1_LARGE_MESSAGE_FLAG == 0 {
        return None;
    }

    let section4_offset = locate_grib1_section4(data)?;
    let raw_section4_length =
        read_u24_be(data.get(section4_offset..section4_offset.checked_add(3)?)?)?;
    if !uses_grib1_large_message_convention(raw_length, raw_section4_length) {
        return None;
    }
    decode_grib1_large_message_length(raw_length, raw_section4_length)
}

fn uses_grib1_large_message_convention(raw_total_length: u32, raw_section4_length: u32) -> bool {
    raw_total_length & GRIB1_LARGE_MESSAGE_FLAG != 0 && raw_section4_length < 120
}

fn decode_grib1_large_message_length(
    raw_total_length: u32,
    raw_section4_length: u32,
) -> Option<u64> {
    u64::from(raw_total_length & !GRIB1_LARGE_MESSAGE_FLAG)
        .checked_mul(GRIB1_LARGE_MESSAGE_BLOCK_BYTES)?
        .checked_sub(u64::from(raw_section4_length))?
        .checked_add(4)
}

fn locate_grib1_section4(data: &[u8]) -> Option<usize> {
    let product_offset = 8usize;
    let product_length = section_length(data, product_offset)?;
    if product_length < 8 {
        return None;
    }
    let product_end = product_offset.checked_add(product_length)?;
    let section_flags = *data.get(product_offset.checked_add(7)?)?;
    let mut offset = product_end;

    if section_flags & 0b1000_0000 != 0 {
        offset = skip_section(data, offset)?;
    }
    if section_flags & 0b0100_0000 != 0 {
        offset = skip_section(data, offset)?;
    }
    data.get(offset..offset.checked_add(3)?)?;
    Some(offset)
}

fn skip_section(data: &[u8], offset: usize) -> Option<usize> {
    let length = section_length(data, offset)?;
    if length < 3 {
        return None;
    }
    let end = offset.checked_add(length)?;
    data.get(offset..end)?;
    Some(end)
}

fn section_length(data: &[u8], offset: usize) -> Option<usize> {
    let end = offset.checked_add(3)?;
    usize::try_from(read_u24_be(data.get(offset..end)?)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_grib2_indicator() {
        let mut data = Vec::new();
        data.extend_from_slice(b"GRIB");
        data.extend_from_slice(&[0, 0]);
        data.push(0); // discipline
        data.push(2); // edition
        data.extend_from_slice(&100u64.to_be_bytes());
        let ind = Indicator::parse(&data).unwrap();
        assert_eq!(ind.edition, 2);
        assert_eq!(ind.discipline, 0);
        assert_eq!(ind.total_length, 100);
    }

    #[test]
    fn parse_grib1_indicator() {
        let mut data = vec![0u8; 8];
        data[0..4].copy_from_slice(b"GRIB");
        data[4] = 0;
        data[5] = 3;
        data[6] = 232; // 1000
        data[7] = 1;
        let ind = Indicator::parse(&data).unwrap();
        assert_eq!(ind.edition, 1);
        assert_eq!(ind.total_length, 1000);
    }

    #[test]
    fn resolves_grib1_large_message_convention_from_section4_padding() {
        let units = 70_500u32;
        let section4_padding = 17u32;
        let mut data = vec![0u8; 8 + 28 + 32 + 3];
        data[0..4].copy_from_slice(b"GRIB");
        data[4..7].copy_from_slice(&(GRIB1_LARGE_MESSAGE_FLAG | units).to_be_bytes()[1..]);
        data[7] = 1;
        data[8..11].copy_from_slice(&28u32.to_be_bytes()[1..]);
        data[15] = 0b1000_0000;
        data[36..39].copy_from_slice(&32u32.to_be_bytes()[1..]);
        data[68..71].copy_from_slice(&section4_padding.to_be_bytes()[1..]);

        let indicator = Indicator::parse(&data).unwrap();
        assert_eq!(
            indicator.total_length,
            u64::from(units) * GRIB1_LARGE_MESSAGE_BLOCK_BYTES - u64::from(section4_padding) + 4
        );
        assert!(indicator.total_length > u64::from(GRIB1_LARGE_MESSAGE_FLAG));
    }

    #[test]
    fn preserves_ordinary_grib1_length_when_section4_is_not_padding() {
        let raw_length = GRIB1_LARGE_MESSAGE_FLAG | 1_000;
        let mut data = vec![0u8; 8 + 28 + 3];
        data[0..4].copy_from_slice(b"GRIB");
        data[4..7].copy_from_slice(&raw_length.to_be_bytes()[1..]);
        data[7] = 1;
        data[8..11].copy_from_slice(&28u32.to_be_bytes()[1..]);
        data[36..39].copy_from_slice(&120u32.to_be_bytes()[1..]);

        assert_eq!(
            Indicator::parse(&data).unwrap().total_length,
            u64::from(raw_length)
        );
    }

    #[test]
    fn reject_invalid_magic() {
        assert!(Indicator::parse(b"NOPE1234").is_none());
        assert_eq!(Indicator::edition(b"NOPE1234"), None);
    }

    #[test]
    fn reads_unsupported_edition_without_parsing_indicator() {
        let mut data = Vec::new();
        data.extend_from_slice(b"GRIB");
        data.extend_from_slice(&[0, 0]);
        data.push(0);
        data.push(3);
        data.extend_from_slice(&20u64.to_be_bytes());

        assert_eq!(Indicator::edition(&data), Some(3));
        assert!(Indicator::parse(&data).is_none());
    }
}
