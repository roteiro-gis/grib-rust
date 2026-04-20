//! Section 0: Indicator Section parsing for GRIB1 and GRIB2.

use grib_core::binary::read_u24_be;

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
    /// Parse from the first bytes of a GRIB message.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 8 || &data[0..4] != b"GRIB" {
            return None;
        }

        let edition = data[7];
        match edition {
            1 => {
                let length = u64::from(read_u24_be(&data[4..7])?);
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
    fn reject_invalid_magic() {
        assert!(Indicator::parse(b"NOPE1234").is_none());
    }
}
