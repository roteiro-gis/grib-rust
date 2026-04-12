//! Grid Definition Section (Section 3) parsing.

use crate::error::{Error, Result};
use crate::util::grib_i32;

/// Grid definition extracted from Section 3.
#[derive(Debug, Clone, PartialEq)]
pub enum GridDefinition {
    /// Template 3.0: Regular latitude/longitude (equidistant cylindrical).
    LatLon(LatLonGrid),
    /// Unsupported template (stored for diagnostics).
    Unsupported(u16),
}

/// Template 3.0: Regular latitude/longitude grid.
#[derive(Debug, Clone, PartialEq)]
pub struct LatLonGrid {
    pub ni: u32,
    pub nj: u32,
    pub lat_first: i32,
    pub lon_first: i32,
    pub lat_last: i32,
    pub lon_last: i32,
    pub di: u32,
    pub dj: u32,
    pub scanning_mode: u8,
}

impl GridDefinition {
    pub fn shape(&self) -> (usize, usize) {
        match self {
            Self::LatLon(g) => (g.ni as usize, g.nj as usize),
            Self::Unsupported(_) => (0, 0),
        }
    }

    pub fn ndarray_shape(&self) -> Vec<usize> {
        let (ni, nj) = self.shape();
        match self {
            Self::LatLon(_) if ni > 0 && nj > 0 => vec![nj, ni],
            _ => Vec::new(),
        }
    }

    pub fn num_points(&self) -> usize {
        let (ni, nj) = self.shape();
        ni.saturating_mul(nj)
    }

    pub fn parse(section_bytes: &[u8]) -> Result<Self> {
        if section_bytes.len() < 14 {
            return Err(Error::InvalidSection {
                section: 3,
                reason: format!("expected at least 14 bytes, got {}", section_bytes.len()),
            });
        }
        if section_bytes[4] != 3 {
            return Err(Error::InvalidSection {
                section: section_bytes[4],
                reason: "not a grid definition section".into(),
            });
        }

        let template = u16::from_be_bytes(section_bytes[12..14].try_into().unwrap());
        match template {
            0 => parse_latlon(section_bytes),
            _ => Ok(Self::Unsupported(template)),
        }
    }
}

impl LatLonGrid {
    pub fn longitudes(&self) -> Vec<f64> {
        let step = self.di as f64 / 1_000_000.0;
        let signed_step = if self.i_scans_positive() { step } else { -step };
        let start = self.lon_first as f64 / 1_000_000.0;
        (0..self.ni)
            .map(|index| start + signed_step * index as f64)
            .collect()
    }

    pub fn latitudes(&self) -> Vec<f64> {
        let step = self.dj as f64 / 1_000_000.0;
        let signed_step = if self.j_scans_positive() { step } else { -step };
        let start = self.lat_first as f64 / 1_000_000.0;
        (0..self.nj)
            .map(|index| start + signed_step * index as f64)
            .collect()
    }

    pub fn reorder_for_ndarray<T>(&self, mut values: Vec<T>) -> Result<Vec<T>> {
        self.reorder_for_ndarray_in_place(&mut values)?;
        Ok(values)
    }

    pub fn reorder_for_ndarray_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        let ni = self.ni as usize;
        let nj = self.nj as usize;
        if values.len() != ni * nj {
            return Err(Error::DataLengthMismatch {
                expected: ni * nj,
                actual: values.len(),
            });
        }

        if !self.i_points_are_consecutive() {
            return Err(Error::UnsupportedScanningMode(self.scanning_mode));
        }

        if self.adjacent_rows_alternate_direction() {
            for row in 0..nj {
                let reverse = if self.i_scans_positive() {
                    row % 2 == 1
                } else {
                    row % 2 == 0
                };
                if reverse {
                    values[row * ni..(row + 1) * ni].reverse();
                }
            }
        }

        Ok(())
    }

    fn i_scans_positive(&self) -> bool {
        self.scanning_mode & 0b1000_0000 == 0
    }

    fn j_scans_positive(&self) -> bool {
        self.scanning_mode & 0b0100_0000 != 0
    }

    fn i_points_are_consecutive(&self) -> bool {
        self.scanning_mode & 0b0010_0000 == 0
    }

    fn adjacent_rows_alternate_direction(&self) -> bool {
        self.scanning_mode & 0b0001_0000 != 0
    }
}

fn parse_latlon(data: &[u8]) -> Result<GridDefinition> {
    if data.len() < 72 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.0 requires 72 bytes, got {}", data.len()),
        });
    }

    let ni = u32::from_be_bytes(data[30..34].try_into().unwrap());
    let nj = u32::from_be_bytes(data[34..38].try_into().unwrap());
    let lat_first = grib_i32(&data[46..50]).unwrap();
    let lon_first = grib_i32(&data[50..54]).unwrap();
    let lat_last = grib_i32(&data[55..59]).unwrap();
    let lon_last = grib_i32(&data[59..63]).unwrap();
    let di = u32::from_be_bytes(data[63..67].try_into().unwrap());
    let dj = u32::from_be_bytes(data[67..71].try_into().unwrap());
    let scanning_mode = data[71];

    Ok(GridDefinition::LatLon(LatLonGrid {
        ni,
        nj,
        lat_first,
        lon_first,
        lat_last,
        lon_last,
        di,
        dj,
        scanning_mode,
    }))
}

#[cfg(test)]
mod tests {
    use super::{GridDefinition, LatLonGrid};

    #[test]
    fn reports_latlon_shape() {
        let grid = GridDefinition::LatLon(LatLonGrid {
            ni: 3,
            nj: 2,
            lat_first: 50_000_000,
            lon_first: -120_000_000,
            lat_last: 49_000_000,
            lon_last: -118_000_000,
            di: 1_000_000,
            dj: 1_000_000,
            scanning_mode: 0,
        });

        assert_eq!(grid.shape(), (3, 2));
        assert_eq!(grid.ndarray_shape(), vec![2, 3]);
        match grid {
            GridDefinition::LatLon(ref latlon) => {
                assert_eq!(latlon.longitudes(), vec![-120.0, -119.0, -118.0]);
                assert_eq!(latlon.latitudes(), vec![50.0, 49.0]);
            }
            GridDefinition::Unsupported(_) => panic!("expected lat/lon grid"),
        }
    }

    #[test]
    fn normalizes_alternating_row_scan() {
        let grid = LatLonGrid {
            ni: 3,
            nj: 2,
            lat_first: 0,
            lon_first: 0,
            lat_last: 0,
            lon_last: 0,
            di: 1,
            dj: 1,
            scanning_mode: 0b0001_0000,
        };

        let ordered = grid
            .reorder_for_ndarray(vec![1.0, 2.0, 3.0, 6.0, 5.0, 4.0])
            .unwrap();
        assert_eq!(ordered, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }
}
