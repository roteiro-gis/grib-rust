//! GRIB Edition 1 parsing.

use crate::data::{decode_payload, DataRepresentation};
use crate::error::{Error, Result};
use crate::sections::SectionRef;

use grib_core::binary::read_u24_be;
pub use grib_core::grib1::{BinaryDataSection, GridDescription, ProductDefinition};

pub fn bitmap_payload(section_bytes: &[u8]) -> Result<Option<&[u8]>> {
    if section_bytes.len() < 6 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("expected at least 6 bytes, got {}", section_bytes.len()),
        });
    }
    let indicator = u16::from_be_bytes(section_bytes[4..6].try_into().unwrap());
    if indicator == 0 {
        Ok(Some(&section_bytes[6..]))
    } else {
        Err(Error::UnsupportedBitmapIndicator(
            if indicator <= u16::from(u8::MAX) {
                indicator as u8
            } else {
                u8::MAX
            },
        ))
    }
}

pub fn decode_simple_field(
    data_section: &[u8],
    representation: &DataRepresentation,
    bitmap_section: Option<&[u8]>,
    num_grid_points: usize,
) -> Result<Vec<f64>> {
    decode_payload(
        data_section,
        representation,
        bitmap_section,
        num_grid_points,
    )
}

pub fn parse_message_sections(message_bytes: &[u8]) -> Result<Grib1Sections> {
    if message_bytes.len() < 8 + 28 + 11 + 4 {
        return Err(Error::InvalidMessage(format!(
            "GRIB1 message too short: {} bytes",
            message_bytes.len()
        )));
    }

    let payload_limit = message_bytes.len() - 4;
    let pds = parse_section(message_bytes, 8, 1, payload_limit)?;
    let pds_bytes = &message_bytes[pds.offset..pds.offset + pds.length];
    let product = ProductDefinition::parse(pds_bytes)?;

    let mut cursor = pds.offset + pds.length;
    let grid = if product.has_grid_definition {
        let section_ref = parse_section(message_bytes, cursor, 2, payload_limit)?;
        cursor += section_ref.length;
        Some(section_ref)
    } else {
        None
    };

    let bitmap = if product.has_bitmap {
        let section_ref = parse_section(message_bytes, cursor, 3, payload_limit)?;
        cursor += section_ref.length;
        Some(section_ref)
    } else {
        None
    };

    let data = parse_section(message_bytes, cursor, 4, payload_limit)?;
    if data.offset + data.length != payload_limit {
        return Err(Error::InvalidMessage(
            "GRIB1 message contains trailing bytes before end marker".into(),
        ));
    }

    Ok(Grib1Sections {
        product,
        pds,
        grid,
        bitmap,
        data,
    })
}

#[derive(Debug, Clone)]
pub struct Grib1Sections {
    pub product: ProductDefinition,
    pub pds: SectionRef,
    pub grid: Option<SectionRef>,
    pub bitmap: Option<SectionRef>,
    pub data: SectionRef,
}

fn parse_section(
    message_bytes: &[u8],
    offset: usize,
    number: u8,
    payload_limit: usize,
) -> Result<SectionRef> {
    let length_bytes = message_bytes
        .get(offset..offset + 3)
        .ok_or(Error::Truncated {
            offset: offset as u64,
        })?;
    let length = read_u24_be(length_bytes).ok_or(Error::Truncated {
        offset: offset as u64,
    })? as usize;
    if length < 3 {
        return Err(Error::InvalidSection {
            section: number,
            reason: format!("section length {length} is smaller than the 3-byte header"),
        });
    }

    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::InvalidMessage("GRIB1 section length overflow".into()))?;
    if end > payload_limit {
        return Err(Error::Truncated {
            offset: offset as u64,
        });
    }

    Ok(section(number, offset, length))
}

fn section(number: u8, offset: usize, length: usize) -> SectionRef {
    SectionRef {
        number,
        offset,
        length,
    }
}

#[cfg(test)]
mod tests {
    use super::{bitmap_payload, parse_message_sections};
    use crate::error::Error;

    #[test]
    fn parses_minimal_section_layout() {
        let mut message = Vec::new();
        message.extend_from_slice(b"GRIB");
        message.extend_from_slice(&[0, 0, 64, 1]);
        let mut pds = vec![0u8; 28];
        pds[..3].copy_from_slice(&[0, 0, 28]);
        pds[7] = 0b1000_0000;
        pds[24] = 21;
        message.extend_from_slice(&pds);
        let mut gds = vec![0u8; 32];
        gds[..3].copy_from_slice(&[0, 0, 32]);
        message.extend_from_slice(&gds);
        let mut bds = vec![0u8; 12];
        bds[..3].copy_from_slice(&[0, 0, 12]);
        message.extend_from_slice(&bds);
        message.extend_from_slice(b"7777");

        let sections = parse_message_sections(&message).unwrap();
        assert!(sections.grid.is_some());
        assert!(sections.bitmap.is_none());
        assert_eq!(sections.data.length, 12);
    }

    #[test]
    fn rejects_section_length_beyond_message_boundary() {
        let mut message = Vec::new();
        message.extend_from_slice(b"GRIB");
        message.extend_from_slice(&[0, 0, 64, 1]);
        let mut pds = vec![0u8; 28];
        pds[..3].copy_from_slice(&[0, 0, 28]);
        pds[7] = 0b1000_0000;
        pds[24] = 21;
        message.extend_from_slice(&pds);
        let mut gds = vec![0u8; 32];
        gds[..3].copy_from_slice(&[0, 0, 250]);
        message.extend_from_slice(&gds);
        let mut bds = vec![0u8; 12];
        bds[..3].copy_from_slice(&[0, 0, 12]);
        message.extend_from_slice(&bds);
        message.extend_from_slice(b"7777");

        assert!(parse_message_sections(&message).is_err());
    }

    #[test]
    fn reports_small_predefined_bitmap_indicator() {
        let err = bitmap_payload(&[0, 0, 6, 0, 0, 5]).unwrap_err();
        assert!(matches!(err, Error::UnsupportedBitmapIndicator(5)));
    }
}
