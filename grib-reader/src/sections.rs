//! GRIB2 section scanning and logical field indexing.

use crate::error::{Error, Result};

/// A located section within a GRIB2 message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionRef {
    pub number: u8,
    pub offset: usize,
    pub length: usize,
}

/// The sections that make up one logical field inside a GRIB2 message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSections {
    pub identification: SectionRef,
    pub grid: SectionRef,
    pub product: SectionRef,
    pub data_representation: SectionRef,
    pub bitmap: Option<SectionRef>,
    pub data: SectionRef,
}

/// Scan a GRIB2 message and return all sections, including the end section.
pub fn scan_sections(msg_bytes: &[u8]) -> Result<Vec<SectionRef>> {
    if msg_bytes.len() < 20 {
        return Err(Error::InvalidMessage(format!(
            "GRIB2 message too short: {} bytes",
            msg_bytes.len()
        )));
    }

    let mut sections = Vec::new();
    let mut pos = 16;

    while pos < msg_bytes.len() {
        if pos + 4 <= msg_bytes.len() && &msg_bytes[pos..pos + 4] == b"7777" {
            if pos != msg_bytes.len() - 4 {
                return Err(Error::InvalidMessage(
                    "end section 7777 encountered before message end".into(),
                ));
            }
            sections.push(SectionRef {
                number: 8,
                offset: pos,
                length: 4,
            });
            return Ok(sections);
        }

        if pos + 5 > msg_bytes.len() {
            return Err(Error::Truncated { offset: pos as u64 });
        }

        let length = u32::from_be_bytes(msg_bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let number = msg_bytes[pos + 4];

        if length < 5 {
            return Err(Error::InvalidSection {
                section: number,
                reason: format!("section length {length} is smaller than the 5-byte header"),
            });
        }
        if pos + length > msg_bytes.len() {
            return Err(Error::Truncated { offset: pos as u64 });
        }

        sections.push(SectionRef {
            number,
            offset: pos,
            length,
        });
        pos += length;
    }

    Err(Error::InvalidMessage("missing end section 7777".into()))
}

/// Split a GRIB2 message into one or more logical fields.
pub fn index_fields(msg_bytes: &[u8]) -> Result<Vec<FieldSections>> {
    let sections = scan_sections(msg_bytes)?;
    let identification = sections
        .iter()
        .copied()
        .find(|section| section.number == 1)
        .ok_or_else(|| Error::InvalidSectionOrder("missing identification section".into()))?;

    let mut fields = Vec::new();
    let mut current_grid = None;
    let mut current_product = None;
    let mut current_representation = None;
    let mut current_bitmap = None;

    for section in sections {
        match section.number {
            1 | 2 => {}
            3 => {
                current_grid = Some(section);
                current_product = None;
                current_representation = None;
                current_bitmap = None;
            }
            4 => {
                if current_grid.is_none() {
                    return Err(Error::InvalidSectionOrder(
                        "product definition encountered before grid definition".into(),
                    ));
                }
                current_product = Some(section);
                current_representation = None;
                current_bitmap = None;
            }
            5 => {
                if current_product.is_none() {
                    return Err(Error::InvalidSectionOrder(
                        "data representation encountered before product definition".into(),
                    ));
                }
                current_representation = Some(section);
                current_bitmap = None;
            }
            6 => {
                if current_representation.is_none() {
                    return Err(Error::InvalidSectionOrder(
                        "bitmap encountered before data representation".into(),
                    ));
                }
                current_bitmap = Some(section);
            }
            7 => {
                let grid = current_grid.ok_or_else(|| {
                    Error::InvalidSectionOrder(
                        "data section encountered before grid definition".into(),
                    )
                })?;
                let product = current_product.ok_or_else(|| {
                    Error::InvalidSectionOrder(
                        "data section encountered before product definition".into(),
                    )
                })?;
                let data_representation = current_representation.ok_or_else(|| {
                    Error::InvalidSectionOrder(
                        "data section encountered before data representation".into(),
                    )
                })?;
                fields.push(FieldSections {
                    identification,
                    grid,
                    product,
                    data_representation,
                    bitmap: current_bitmap,
                    data: section,
                });
                current_product = None;
                current_representation = None;
                current_bitmap = None;
            }
            8 => break,
            other => {
                return Err(Error::InvalidSection {
                    section: other,
                    reason: "unexpected section number".into(),
                });
            }
        }
    }

    if fields.is_empty() {
        return Err(Error::InvalidSectionOrder(
            "message did not contain a complete field".into(),
        ));
    }

    Ok(fields)
}

#[cfg(test)]
mod tests {
    use crate::error::Error;

    use super::{index_fields, scan_sections};

    fn section(number: u8, payload_len: usize) -> Vec<u8> {
        let len = (payload_len + 5) as u32;
        let mut bytes = len.to_be_bytes().to_vec();
        bytes.push(number);
        bytes.resize(len as usize, 0);
        bytes
    }

    #[test]
    fn scan_minimal_sections() {
        let mut data = vec![0u8; 16];
        data.extend_from_slice(&section(1, 16));
        data.extend_from_slice(&section(3, 20));
        data.extend_from_slice(&section(4, 29));
        data.extend_from_slice(&section(5, 16));
        data.extend_from_slice(&section(7, 3));
        data.extend_from_slice(b"7777");

        let sections = scan_sections(&data).unwrap();
        assert_eq!(sections.len(), 6);
        assert_eq!(sections[0].number, 1);
        assert_eq!(sections[4].number, 7);
        assert_eq!(sections[5].number, 8);
    }

    #[test]
    fn indexes_repeated_fields() {
        let mut data = vec![0u8; 16];
        data.extend_from_slice(&section(1, 16));
        data.extend_from_slice(&section(3, 20));
        data.extend_from_slice(&section(4, 29));
        data.extend_from_slice(&section(5, 16));
        data.extend_from_slice(&section(7, 3));
        data.extend_from_slice(&section(4, 29));
        data.extend_from_slice(&section(5, 16));
        data.extend_from_slice(&section(7, 3));
        data.extend_from_slice(b"7777");

        let fields = index_fields(&data).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].grid.number, 3);
        assert_eq!(fields[1].product.number, 4);
    }

    #[test]
    fn rejects_early_end_section_before_message_end() {
        let mut data = vec![0u8; 16];
        data.extend_from_slice(&section(1, 16));
        data.extend_from_slice(b"7777");
        data.extend_from_slice(&[0, 0, 0, 0]);

        let err = scan_sections(&data).unwrap_err();
        assert!(matches!(err, Error::InvalidMessage(_)));
    }
}
