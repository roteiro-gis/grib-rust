//! Grid Definition Section (Section 3) parsing.

use crate::error::{Error, Result};
use crate::util::grib_i32;

/// Grid definition extracted from Section 3.
///
/// This enum is non-exhaustive because GRIB grid templates are open-ended:
/// WMO can add standard templates and centers can use local templates. Prefer
/// the query helpers on this type for common behavior, or keep a wildcard arm
/// when matching specific grid families.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GridDefinition {
    /// Template 3.0: Regular latitude/longitude (equidistant cylindrical).
    LatLon(LatLonGrid),
    /// Template 3.30: Lambert conformal.
    LambertConformal(LambertConformalGrid),
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

/// Template 3.30: Lambert conformal grid.
#[derive(Debug, Clone, PartialEq)]
pub struct LambertConformalGrid {
    pub number_of_points: u32,
    pub shape_of_earth: u8,
    pub scale_factor_radius: u8,
    pub scaled_value_radius: u32,
    pub scale_factor_major_axis: u8,
    pub scaled_value_major_axis: u32,
    pub scale_factor_minor_axis: u8,
    pub scaled_value_minor_axis: u32,
    pub nx: u32,
    pub ny: u32,
    pub lat_first: i32,
    pub lon_first: u32,
    pub resolution_and_component_flags: u8,
    pub lat_d: i32,
    pub lon_v: u32,
    pub dx: u32,
    pub dy: u32,
    pub projection_center_flag: u8,
    pub scanning_mode: u8,
    pub latin1: i32,
    pub latin2: i32,
    pub lat_southern_pole: i32,
    pub lon_southern_pole: u32,
}

impl GridDefinition {
    /// GRIB2 grid definition template number for typed templates.
    ///
    /// `Unsupported` stores the unhandled template number from the source
    /// message for diagnostics.
    pub fn template_number(&self) -> u16 {
        match self {
            Self::LatLon(_) => 0,
            Self::LambertConformal(_) => 30,
            Self::Unsupported(template) => *template,
        }
    }

    pub fn as_lat_lon(&self) -> Option<&LatLonGrid> {
        match self {
            Self::LatLon(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn as_lambert_conformal(&self) -> Option<&LambertConformalGrid> {
        match self {
            Self::LambertConformal(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn unsupported_template(&self) -> Option<u16> {
        match self {
            Self::Unsupported(template) => Some(*template),
            _ => None,
        }
    }

    pub fn shape(&self) -> (usize, usize) {
        match self {
            Self::LatLon(g) => (g.ni as usize, g.nj as usize),
            Self::LambertConformal(g) => (g.nx as usize, g.ny as usize),
            Self::Unsupported(_) => (0, 0),
        }
    }

    pub fn ndarray_shape(&self) -> Vec<usize> {
        let (ni, nj) = self.shape();
        match self {
            Self::LatLon(_) | Self::LambertConformal(_) if ni > 0 && nj > 0 => vec![nj, ni],
            _ => Vec::new(),
        }
    }

    pub fn num_points(&self) -> usize {
        match self {
            Self::LatLon(_) => {
                let (ni, nj) = self.shape();
                ni.saturating_mul(nj)
            }
            Self::LambertConformal(g) => g.number_of_points as usize,
            Self::Unsupported(_) => 0,
        }
    }

    pub fn validate_supported_scan_order(&self) -> Result<()> {
        match self {
            Self::LatLon(grid) => grid.validate_supported_scan_order(),
            Self::LambertConformal(grid) => grid.validate_supported_scan_order(),
            Self::Unsupported(template) => Err(Error::UnsupportedGridTemplate(*template)),
        }
    }

    pub fn reorder_for_ndarray_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        match self {
            Self::LatLon(grid) => grid.reorder_for_ndarray_in_place(values),
            Self::LambertConformal(grid) => grid.reorder_for_ndarray_in_place(values),
            Self::Unsupported(template) => Err(Error::UnsupportedGridTemplate(*template)),
        }
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
            30 => parse_lambert_conformal(section_bytes),
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
        self.reorder_grib_scan_to_ndarray_in_place(&mut values)?;
        Ok(values)
    }

    pub fn reorder_for_ndarray_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        self.reorder_grib_scan_to_ndarray_in_place(values)
    }

    pub fn reorder_grib_scan_to_ndarray<T>(&self, mut values: Vec<T>) -> Result<Vec<T>> {
        self.reorder_grib_scan_to_ndarray_in_place(&mut values)?;
        Ok(values)
    }

    pub fn reorder_grib_scan_to_ndarray_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        transform_supported_scan_order_in_place(
            values,
            self.ni as usize,
            self.nj as usize,
            self.scanning_mode,
        )
    }

    pub fn reorder_ndarray_to_grib_scan<T>(&self, mut values: Vec<T>) -> Result<Vec<T>> {
        self.reorder_ndarray_to_grib_scan_in_place(&mut values)?;
        Ok(values)
    }

    pub fn reorder_ndarray_to_grib_scan_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        transform_supported_scan_order_in_place(
            values,
            self.ni as usize,
            self.nj as usize,
            self.scanning_mode,
        )
    }

    pub fn validate_supported_scan_order(&self) -> Result<()> {
        validate_supported_scan_order(self.scanning_mode)
    }

    fn i_scans_positive(&self) -> bool {
        i_scans_positive(self.scanning_mode)
    }

    fn j_scans_positive(&self) -> bool {
        j_scans_positive(self.scanning_mode)
    }
}

impl LambertConformalGrid {
    pub fn reorder_for_ndarray_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        transform_supported_scan_order_in_place(
            values,
            self.nx as usize,
            self.ny as usize,
            self.scanning_mode,
        )
    }

    pub fn validate_supported_scan_order(&self) -> Result<()> {
        validate_supported_scan_order(self.scanning_mode)
    }
}

fn transform_supported_scan_order_in_place<T>(
    values: &mut [T],
    ni: usize,
    nj: usize,
    scanning_mode: u8,
) -> Result<()> {
    validate_supported_scan_order(scanning_mode)?;
    if values.len() != ni * nj {
        return Err(Error::DataLengthMismatch {
            expected: ni * nj,
            actual: values.len(),
        });
    }

    if adjacent_rows_alternate_direction(scanning_mode) {
        reverse_alternating_rows(values, ni, nj, i_scans_positive(scanning_mode));
    }

    Ok(())
}

fn validate_supported_scan_order(scanning_mode: u8) -> Result<()> {
    if i_points_are_consecutive(scanning_mode) {
        Ok(())
    } else {
        Err(Error::UnsupportedScanningMode(scanning_mode))
    }
}

fn i_scans_positive(scanning_mode: u8) -> bool {
    scanning_mode & 0b1000_0000 == 0
}

fn j_scans_positive(scanning_mode: u8) -> bool {
    scanning_mode & 0b0100_0000 != 0
}

fn i_points_are_consecutive(scanning_mode: u8) -> bool {
    scanning_mode & 0b0010_0000 == 0
}

fn adjacent_rows_alternate_direction(scanning_mode: u8) -> bool {
    scanning_mode & 0b0001_0000 != 0
}

fn reverse_alternating_rows<T>(values: &mut [T], ni: usize, nj: usize, i_scans_positive: bool) {
    for row in 0..nj {
        let reverse = if i_scans_positive {
            row % 2 == 1
        } else {
            row % 2 == 0
        };
        if reverse {
            values[row * ni..(row + 1) * ni].reverse();
        }
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

fn parse_lambert_conformal(data: &[u8]) -> Result<GridDefinition> {
    if data.len() < 81 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.30 requires 81 bytes, got {}", data.len()),
        });
    }

    Ok(GridDefinition::LambertConformal(LambertConformalGrid {
        number_of_points: u32::from_be_bytes(data[6..10].try_into().unwrap()),
        shape_of_earth: data[14],
        scale_factor_radius: data[15],
        scaled_value_radius: u32::from_be_bytes(data[16..20].try_into().unwrap()),
        scale_factor_major_axis: data[20],
        scaled_value_major_axis: u32::from_be_bytes(data[21..25].try_into().unwrap()),
        scale_factor_minor_axis: data[25],
        scaled_value_minor_axis: u32::from_be_bytes(data[26..30].try_into().unwrap()),
        nx: u32::from_be_bytes(data[30..34].try_into().unwrap()),
        ny: u32::from_be_bytes(data[34..38].try_into().unwrap()),
        lat_first: grib_i32(&data[38..42]).unwrap(),
        lon_first: u32::from_be_bytes(data[42..46].try_into().unwrap()),
        resolution_and_component_flags: data[46],
        lat_d: grib_i32(&data[47..51]).unwrap(),
        lon_v: u32::from_be_bytes(data[51..55].try_into().unwrap()),
        dx: u32::from_be_bytes(data[55..59].try_into().unwrap()),
        dy: u32::from_be_bytes(data[59..63].try_into().unwrap()),
        projection_center_flag: data[63],
        scanning_mode: data[64],
        latin1: grib_i32(&data[65..69]).unwrap(),
        latin2: grib_i32(&data[69..73]).unwrap(),
        lat_southern_pole: grib_i32(&data[73..77]).unwrap(),
        lon_southern_pole: u32::from_be_bytes(data[77..81].try_into().unwrap()),
    }))
}

#[cfg(test)]
mod tests {
    use super::{GridDefinition, LambertConformalGrid, LatLonGrid};
    use crate::binary::encode_wmo_i32;

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
        assert_eq!(grid.template_number(), 0);
        assert!(grid.as_lat_lon().is_some());
        assert!(grid.as_lambert_conformal().is_none());
        assert_eq!(grid.unsupported_template(), None);
        match grid {
            GridDefinition::LatLon(ref latlon) => {
                assert_eq!(latlon.longitudes(), vec![-120.0, -119.0, -118.0]);
                assert_eq!(latlon.latitudes(), vec![50.0, 49.0]);
            }
            other => panic!("expected lat/lon grid, got {other:?}"),
        }
    }

    #[test]
    fn parses_lambert_conformal_template() {
        let section = build_lambert_section();
        let grid = GridDefinition::parse(&section).unwrap();

        assert_eq!(grid.shape(), (3, 2));
        assert_eq!(grid.ndarray_shape(), vec![2, 3]);
        assert_eq!(grid.num_points(), 6);
        assert_eq!(grid.template_number(), 30);
        assert!(grid.as_lat_lon().is_none());
        assert!(grid.as_lambert_conformal().is_some());
        assert_eq!(grid.unsupported_template(), None);
        match grid {
            GridDefinition::LambertConformal(lambert) => {
                assert_eq!(lambert.number_of_points, 6);
                assert_eq!(lambert.shape_of_earth, 1);
                assert_eq!(lambert.scaled_value_radius, 6_371_200);
                assert_eq!(lambert.nx, 3);
                assert_eq!(lambert.ny, 2);
                assert_eq!(lambert.lat_first, 12_190_000);
                assert_eq!(lambert.lon_first, 226_541_000);
                assert_eq!(lambert.resolution_and_component_flags, 0x08);
                assert_eq!(lambert.lat_d, 25_000_000);
                assert_eq!(lambert.lon_v, 265_000_000);
                assert_eq!(lambert.dx, 2_539_703);
                assert_eq!(lambert.dy, 2_539_703);
                assert_eq!(lambert.projection_center_flag, 0);
                assert_eq!(lambert.scanning_mode, 0);
                assert_eq!(lambert.latin1, 25_000_000);
                assert_eq!(lambert.latin2, 25_000_000);
                assert_eq!(lambert.lat_southern_pole, -90_000_000);
                assert_eq!(lambert.lon_southern_pole, 0);
            }
            other => panic!("expected Lambert conformal grid, got {other:?}"),
        }
    }

    #[test]
    fn reports_unsupported_template_helpers() {
        let grid = GridDefinition::Unsupported(3_276);

        assert_eq!(grid.template_number(), 3_276);
        assert!(grid.as_lat_lon().is_none());
        assert!(grid.as_lambert_conformal().is_none());
        assert_eq!(grid.unsupported_template(), Some(3_276));
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

    #[test]
    fn converts_ndarray_order_to_alternating_scan_order() {
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

        let scan_order = grid
            .reorder_ndarray_to_grib_scan(vec![1, 2, 3, 4, 5, 6])
            .unwrap();
        assert_eq!(scan_order, vec![1, 2, 3, 6, 5, 4]);
    }

    #[test]
    fn normalizes_lambert_alternating_row_scan() {
        let grid = LambertConformalGrid {
            number_of_points: 6,
            shape_of_earth: 1,
            scale_factor_radius: 0,
            scaled_value_radius: 6_371_200,
            scale_factor_major_axis: 0,
            scaled_value_major_axis: 0,
            scale_factor_minor_axis: 0,
            scaled_value_minor_axis: 0,
            nx: 3,
            ny: 2,
            lat_first: 0,
            lon_first: 0,
            resolution_and_component_flags: 0,
            lat_d: 0,
            lon_v: 0,
            dx: 1,
            dy: 1,
            projection_center_flag: 0,
            scanning_mode: 0b0001_0000,
            latin1: 0,
            latin2: 0,
            lat_southern_pole: 0,
            lon_southern_pole: 0,
        };

        let mut values = vec![1.0, 2.0, 3.0, 6.0, 5.0, 4.0];
        grid.reorder_for_ndarray_in_place(&mut values).unwrap();
        assert_eq!(values, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn preserves_non_alternating_scan_modes_in_current_reader_order() {
        for scanning_mode in [0b0000_0000, 0b1000_0000, 0b0100_0000, 0b1100_0000] {
            let grid = LatLonGrid {
                ni: 3,
                nj: 2,
                lat_first: 0,
                lon_first: 0,
                lat_last: 0,
                lon_last: 0,
                di: 1,
                dj: 1,
                scanning_mode,
            };

            let values = vec![1, 2, 3, 4, 5, 6];
            assert_eq!(
                grid.reorder_grib_scan_to_ndarray(values.clone()).unwrap(),
                values
            );
            assert_eq!(
                grid.reorder_ndarray_to_grib_scan(values.clone()).unwrap(),
                values
            );
        }
    }

    #[test]
    fn rejects_j_consecutive_scan_order() {
        let grid = LatLonGrid {
            ni: 3,
            nj: 2,
            lat_first: 0,
            lon_first: 0,
            lat_last: 0,
            lon_last: 0,
            di: 1,
            dj: 1,
            scanning_mode: 0b0010_0000,
        };

        let err = grid
            .reorder_ndarray_to_grib_scan(vec![1, 2, 3, 4, 5, 6])
            .unwrap_err();
        assert!(matches!(
            err,
            crate::Error::UnsupportedScanningMode(0b0010_0000)
        ));
    }

    fn build_lambert_section() -> Vec<u8> {
        let mut section = vec![0u8; 81];
        section[..4].copy_from_slice(&81u32.to_be_bytes());
        section[4] = 3;
        section[6..10].copy_from_slice(&6u32.to_be_bytes());
        section[12..14].copy_from_slice(&30u16.to_be_bytes());
        section[14] = 1;
        section[16..20].copy_from_slice(&6_371_200u32.to_be_bytes());
        section[30..34].copy_from_slice(&3u32.to_be_bytes());
        section[34..38].copy_from_slice(&2u32.to_be_bytes());
        section[38..42].copy_from_slice(&encode_wmo_i32(12_190_000).unwrap());
        section[42..46].copy_from_slice(&226_541_000u32.to_be_bytes());
        section[46] = 0x08;
        section[47..51].copy_from_slice(&encode_wmo_i32(25_000_000).unwrap());
        section[51..55].copy_from_slice(&265_000_000u32.to_be_bytes());
        section[55..59].copy_from_slice(&2_539_703u32.to_be_bytes());
        section[59..63].copy_from_slice(&2_539_703u32.to_be_bytes());
        section[65..69].copy_from_slice(&encode_wmo_i32(25_000_000).unwrap());
        section[69..73].copy_from_slice(&encode_wmo_i32(25_000_000).unwrap());
        section[73..77].copy_from_slice(&encode_wmo_i32(-90_000_000).unwrap());
        section
    }
}
