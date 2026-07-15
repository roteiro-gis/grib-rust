//! Grid Definition Section (Section 3) parsing.

use crate::binary::decode_wmo_i32;
use crate::error::{Error, Result};
use crate::{ensure_limit, filled_vec};

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
    /// Template 3.1: Rotated latitude/longitude.
    RotatedLatLon(RotatedLatLonGrid),
    /// Template 3.10: Mercator.
    Mercator(MercatorGrid),
    /// Template 3.31: Albers equal-area.
    AlbersEqualArea(AlbersEqualAreaGrid),
    /// Template 3.30: Lambert conformal.
    LambertConformal(LambertConformalGrid),
    /// Template 3.20: Polar stereographic projection.
    PolarStereographic(PolarStereographicGrid),
    /// Template 3.40: Regular Gaussian latitude/longitude.
    RegularGaussian(RegularGaussianGrid),
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

/// Template 3.1: rotated latitude/longitude grid.
#[derive(Debug, Clone, PartialEq)]
pub struct RotatedLatLonGrid {
    /// Grid coordinates expressed in the rotated coordinate system.
    pub grid: LatLonGrid,
    pub lat_southern_pole: i32,
    pub lon_southern_pole: u32,
    pub angle_of_rotation: f32,
}

/// Template 3.40: regular Gaussian latitude/longitude grid.
#[derive(Debug, Clone, PartialEq)]
pub struct RegularGaussianGrid {
    pub ni: u32,
    pub nj: u32,
    pub lat_first: i32,
    pub lon_first: i32,
    pub lat_last: i32,
    pub lon_last: i32,
    pub di: u32,
    /// Number of Gaussian parallels between a pole and the equator.
    pub number_of_parallels: u32,
    pub scanning_mode: u8,
}

/// Template 3.10: Mercator grid.
#[derive(Debug, Clone, PartialEq)]
pub struct MercatorGrid {
    pub core: ProjectedGridCore,
    pub lat_d: i32,
    pub lat_last: i32,
    pub lon_last: i32,
    pub orientation_of_grid: i32,
}

/// Fields shared by all supported projected grid templates.
///
/// Longitudes use signed microdegrees consistently. Positive values follow
/// the WMO east-longitude convention; negative west-longitude inputs are
/// preserved rather than forced into a 0..360 representation.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedGridCore {
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
    pub lon_first: i32,
    pub resolution_and_component_flags: u8,
    pub dx: u32,
    pub dy: u32,
    pub scanning_mode: u8,
}

/// Template 3.30: Lambert conformal grid.
#[derive(Debug, Clone, PartialEq)]
pub struct LambertConformalGrid {
    pub core: ProjectedGridCore,
    pub lat_d: i32,
    pub lon_v: i32,
    pub projection_center_flag: u8,
    pub latin1: i32,
    pub latin2: i32,
    pub lat_southern_pole: i32,
    pub lon_southern_pole: i32,
}

/// Template 3.31: Albers equal-area grid.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbersEqualAreaGrid {
    pub core: ProjectedGridCore,
    pub lat_d: i32,
    pub lon_v: i32,
    pub projection_center_flag: u8,
    pub latin1: i32,
    pub latin2: i32,
    pub lat_southern_pole: i32,
    pub lon_southern_pole: i32,
}

/// Template 3.20: Polar stereographic projection.
#[derive(Debug, Clone, PartialEq)]
pub struct PolarStereographicGrid {
    pub core: ProjectedGridCore,
    pub lat_d: i32,
    pub lon_v: i32,
    pub projection_center_flag: u8,
}

impl GridDefinition {
    /// GRIB2 grid definition template number for typed templates.
    ///
    /// `Unsupported` stores the unhandled template number from the source
    /// message for diagnostics.
    pub fn template_number(&self) -> u16 {
        match self {
            Self::LatLon(_) => 0,
            Self::RotatedLatLon(_) => 1,
            Self::Mercator(_) => 10,
            Self::PolarStereographic(_) => 20,
            Self::LambertConformal(_) => 30,
            Self::AlbersEqualArea(_) => 31,
            Self::RegularGaussian(_) => 40,
            Self::Unsupported(template) => *template,
        }
    }

    pub fn as_lat_lon(&self) -> Option<&LatLonGrid> {
        match self {
            Self::LatLon(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn as_rotated_lat_lon(&self) -> Option<&RotatedLatLonGrid> {
        match self {
            Self::RotatedLatLon(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn as_regular_gaussian(&self) -> Option<&RegularGaussianGrid> {
        match self {
            Self::RegularGaussian(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn as_polar_stereographic(&self) -> Option<&PolarStereographicGrid> {
        match self {
            Self::PolarStereographic(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn as_mercator(&self) -> Option<&MercatorGrid> {
        match self {
            Self::Mercator(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn as_lambert_conformal(&self) -> Option<&LambertConformalGrid> {
        match self {
            Self::LambertConformal(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn as_albers_equal_area(&self) -> Option<&AlbersEqualAreaGrid> {
        match self {
            Self::AlbersEqualArea(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn projected_core(&self) -> Option<&ProjectedGridCore> {
        match self {
            Self::Mercator(grid) => Some(&grid.core),
            Self::PolarStereographic(grid) => Some(&grid.core),
            Self::LambertConformal(grid) => Some(&grid.core),
            Self::AlbersEqualArea(grid) => Some(&grid.core),
            Self::LatLon(_)
            | Self::RotatedLatLon(_)
            | Self::RegularGaussian(_)
            | Self::Unsupported(_) => None,
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
            Self::RotatedLatLon(g) => (g.grid.ni as usize, g.grid.nj as usize),
            Self::Mercator(g) => (g.core.nx as usize, g.core.ny as usize),
            Self::PolarStereographic(g) => (g.core.nx as usize, g.core.ny as usize),
            Self::LambertConformal(g) => (g.core.nx as usize, g.core.ny as usize),
            Self::AlbersEqualArea(g) => (g.core.nx as usize, g.core.ny as usize),
            Self::RegularGaussian(g) => (g.ni as usize, g.nj as usize),
            Self::Unsupported(_) => (0, 0),
        }
    }

    pub fn shape_num_points(&self) -> Result<usize> {
        match self {
            Self::LatLon(g) => checked_grid_point_count(g.ni, g.nj),
            Self::RotatedLatLon(g) => checked_grid_point_count(g.grid.ni, g.grid.nj),
            Self::Mercator(g) => checked_grid_point_count(g.core.nx, g.core.ny),
            Self::PolarStereographic(g) => checked_grid_point_count(g.core.nx, g.core.ny),
            Self::LambertConformal(g) => checked_grid_point_count(g.core.nx, g.core.ny),
            Self::AlbersEqualArea(g) => checked_grid_point_count(g.core.nx, g.core.ny),
            Self::RegularGaussian(g) => checked_grid_point_count(g.ni, g.nj),
            Self::Unsupported(_) => Ok(0),
        }
    }

    pub fn ndarray_shape(&self) -> Vec<usize> {
        let (ni, nj) = self.shape();
        match self {
            Self::LatLon(_)
            | Self::RotatedLatLon(_)
            | Self::RegularGaussian(_)
            | Self::Mercator(_)
            | Self::PolarStereographic(_)
            | Self::LambertConformal(_)
            | Self::AlbersEqualArea(_)
                if ni > 0 && nj > 0 =>
            {
                vec![nj, ni]
            }
            _ => Vec::new(),
        }
    }

    pub fn num_points(&self) -> usize {
        self.checked_num_points().unwrap_or(usize::MAX)
    }

    pub fn checked_num_points(&self) -> Result<usize> {
        match self {
            Self::LatLon(_) => self.shape_num_points(),
            Self::RotatedLatLon(_) => self.shape_num_points(),
            Self::Mercator(g) => Ok(g.core.number_of_points as usize),
            Self::PolarStereographic(g) => Ok(g.core.number_of_points as usize),
            Self::LambertConformal(g) => Ok(g.core.number_of_points as usize),
            Self::AlbersEqualArea(g) => Ok(g.core.number_of_points as usize),
            Self::RegularGaussian(_) => self.shape_num_points(),
            Self::Unsupported(_) => Ok(0),
        }
    }

    pub fn declared_num_points(&self) -> Option<usize> {
        match self {
            Self::Mercator(g) => Some(g.core.number_of_points as usize),
            Self::PolarStereographic(g) => Some(g.core.number_of_points as usize),
            Self::LambertConformal(g) => Some(g.core.number_of_points as usize),
            Self::AlbersEqualArea(g) => Some(g.core.number_of_points as usize),
            Self::LatLon(_)
            | Self::RotatedLatLon(_)
            | Self::RegularGaussian(_)
            | Self::Unsupported(_) => None,
        }
    }

    /// One-dimensional latitude axis for unrotated geographic grids.
    pub fn latitudes(&self) -> Result<Option<Vec<f64>>> {
        self.latitudes_with_limit(None)
    }

    pub fn latitudes_with_limit(&self, max_axis_points: Option<usize>) -> Result<Option<Vec<f64>>> {
        match self {
            Self::LatLon(grid) => Ok(Some(grid.latitudes_with_limit(max_axis_points)?)),
            Self::RegularGaussian(grid) => Ok(Some(grid.latitudes_with_limit(max_axis_points)?)),
            _ => Ok(None),
        }
    }

    /// One-dimensional longitude axis for unrotated geographic grids.
    pub fn longitudes(&self) -> Result<Option<Vec<f64>>> {
        self.longitudes_with_limit(None)
    }

    pub fn longitudes_with_limit(
        &self,
        max_axis_points: Option<usize>,
    ) -> Result<Option<Vec<f64>>> {
        match self {
            Self::LatLon(grid) => Ok(Some(grid.longitudes_with_limit(max_axis_points)?)),
            Self::RegularGaussian(grid) => Ok(Some(grid.longitudes_with_limit(max_axis_points)?)),
            _ => Ok(None),
        }
    }

    /// Projected x coordinates in metres relative to the first grid point.
    ///
    /// Regular latitude/longitude grids return `None`; use
    /// [`LatLonGrid::longitudes`] for those grids.
    pub fn projected_x_coordinates(&self) -> Result<Option<Vec<f64>>> {
        self.projected_x_coordinates_with_limit(None)
    }

    pub fn projected_x_coordinates_with_limit(
        &self,
        max_axis_points: Option<usize>,
    ) -> Result<Option<Vec<f64>>> {
        match self {
            Self::Mercator(grid) => Ok(Some(grid.core.x_coordinates_with_limit(max_axis_points)?)),
            Self::PolarStereographic(grid) => {
                Ok(Some(grid.core.x_coordinates_with_limit(max_axis_points)?))
            }
            Self::LambertConformal(grid) => {
                Ok(Some(grid.core.x_coordinates_with_limit(max_axis_points)?))
            }
            Self::AlbersEqualArea(grid) => {
                Ok(Some(grid.core.x_coordinates_with_limit(max_axis_points)?))
            }
            _ => Ok(None),
        }
    }

    /// Projected y coordinates in metres relative to the first grid point.
    ///
    /// Regular latitude/longitude grids return `None`; use
    /// [`LatLonGrid::latitudes`] for those grids.
    pub fn projected_y_coordinates(&self) -> Result<Option<Vec<f64>>> {
        self.projected_y_coordinates_with_limit(None)
    }

    pub fn projected_y_coordinates_with_limit(
        &self,
        max_axis_points: Option<usize>,
    ) -> Result<Option<Vec<f64>>> {
        match self {
            Self::Mercator(grid) => Ok(Some(grid.core.y_coordinates_with_limit(max_axis_points)?)),
            Self::PolarStereographic(grid) => {
                Ok(Some(grid.core.y_coordinates_with_limit(max_axis_points)?))
            }
            Self::LambertConformal(grid) => {
                Ok(Some(grid.core.y_coordinates_with_limit(max_axis_points)?))
            }
            Self::AlbersEqualArea(grid) => {
                Ok(Some(grid.core.y_coordinates_with_limit(max_axis_points)?))
            }
            _ => Ok(None),
        }
    }

    pub fn validate_supported_scan_order(&self) -> Result<()> {
        match self {
            Self::LatLon(grid) => grid.validate_supported_scan_order(),
            Self::RotatedLatLon(grid) => grid.grid.validate_supported_scan_order(),
            Self::Mercator(grid) => grid.core.validate_supported_scan_order(),
            Self::PolarStereographic(grid) => grid.core.validate_supported_scan_order(),
            Self::LambertConformal(grid) => grid.core.validate_supported_scan_order(),
            Self::AlbersEqualArea(grid) => grid.core.validate_supported_scan_order(),
            Self::RegularGaussian(grid) => grid.validate_supported_scan_order(),
            Self::Unsupported(template) => Err(Error::UnsupportedGridTemplate(*template)),
        }
    }

    /// Converts GRIB scan traversal into row-major order by undoing alternating
    /// row scans. The encoded i/j direction is deliberately preserved.
    pub fn reorder_for_ndarray_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        match self {
            Self::LatLon(grid) => grid.reorder_for_ndarray_in_place(values),
            Self::RotatedLatLon(grid) => grid.grid.reorder_for_ndarray_in_place(values),
            Self::Mercator(grid) => grid.core.reorder_for_ndarray_in_place(values),
            Self::PolarStereographic(grid) => grid.core.reorder_for_ndarray_in_place(values),
            Self::LambertConformal(grid) => grid.core.reorder_for_ndarray_in_place(values),
            Self::AlbersEqualArea(grid) => grid.core.reorder_for_ndarray_in_place(values),
            Self::RegularGaussian(grid) => transform_supported_scan_order_in_place(
                values,
                grid.ni as usize,
                grid.nj as usize,
                grid.scanning_mode,
            ),
            Self::Unsupported(template) => Err(Error::UnsupportedGridTemplate(*template)),
        }
    }

    /// Converts GRIB scan traversal to deterministic row-major orientation:
    /// columns increase in +i order and rows increase in -j order (north to
    /// south for conventional geographic grids).
    pub fn normalize_north_up_in_place<T>(&self, values: &mut [T]) -> Result<()> {
        match self {
            Self::LatLon(grid) => normalize_north_up_in_place(
                values,
                grid.ni as usize,
                grid.nj as usize,
                grid.scanning_mode,
            ),
            Self::RotatedLatLon(grid) => normalize_north_up_in_place(
                values,
                grid.grid.ni as usize,
                grid.grid.nj as usize,
                grid.grid.scanning_mode,
            ),
            Self::Mercator(grid) => normalize_north_up_in_place(
                values,
                grid.core.nx as usize,
                grid.core.ny as usize,
                grid.core.scanning_mode,
            ),
            Self::PolarStereographic(grid) => normalize_north_up_in_place(
                values,
                grid.core.nx as usize,
                grid.core.ny as usize,
                grid.core.scanning_mode,
            ),
            Self::LambertConformal(grid) => normalize_north_up_in_place(
                values,
                grid.core.nx as usize,
                grid.core.ny as usize,
                grid.core.scanning_mode,
            ),
            Self::AlbersEqualArea(grid) => normalize_north_up_in_place(
                values,
                grid.core.nx as usize,
                grid.core.ny as usize,
                grid.core.scanning_mode,
            ),
            Self::RegularGaussian(grid) => normalize_north_up_in_place(
                values,
                grid.ni as usize,
                grid.nj as usize,
                grid.scanning_mode,
            ),
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
            1 => parse_rotated_latlon(section_bytes),
            10 => parse_mercator(section_bytes),
            20 => parse_polar_stereographic(section_bytes),
            30 => parse_lambert_conformal(section_bytes),
            31 => parse_albers_equal_area(section_bytes),
            40 => parse_regular_gaussian(section_bytes),
            _ => Ok(Self::Unsupported(template)),
        }
    }
}

impl LatLonGrid {
    pub fn longitudes(&self) -> Result<Vec<f64>> {
        self.longitudes_with_limit(None)
    }

    pub fn longitudes_with_limit(&self, max_axis_points: Option<usize>) -> Result<Vec<f64>> {
        let step = self.di as f64 / 1_000_000.0;
        let signed_step = if self.i_scans_positive() { step } else { -step };
        let start = self.lon_first as f64 / 1_000_000.0;
        linear_axis(
            self.ni,
            start,
            signed_step,
            max_axis_points,
            "longitude axis",
        )
    }

    pub fn latitudes(&self) -> Result<Vec<f64>> {
        self.latitudes_with_limit(None)
    }

    pub fn latitudes_with_limit(&self, max_axis_points: Option<usize>) -> Result<Vec<f64>> {
        let step = self.dj as f64 / 1_000_000.0;
        let signed_step = if self.j_scans_positive() { step } else { -step };
        let start = self.lat_first as f64 / 1_000_000.0;
        linear_axis(
            self.nj,
            start,
            signed_step,
            max_axis_points,
            "latitude axis",
        )
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

impl RotatedLatLonGrid {
    pub fn rotated_longitudes(&self) -> Result<Vec<f64>> {
        self.grid.longitudes()
    }

    pub fn rotated_longitudes_with_limit(
        &self,
        max_axis_points: Option<usize>,
    ) -> Result<Vec<f64>> {
        self.grid.longitudes_with_limit(max_axis_points)
    }

    pub fn rotated_latitudes(&self) -> Result<Vec<f64>> {
        self.grid.latitudes()
    }

    pub fn rotated_latitudes_with_limit(&self, max_axis_points: Option<usize>) -> Result<Vec<f64>> {
        self.grid.latitudes_with_limit(max_axis_points)
    }
}

impl RegularGaussianGrid {
    pub fn longitudes(&self) -> Result<Vec<f64>> {
        self.longitudes_with_limit(None)
    }

    pub fn longitudes_with_limit(&self, max_axis_points: Option<usize>) -> Result<Vec<f64>> {
        let step = self.longitude_increment_degrees()?;
        let signed_step = if i_scans_positive(self.scanning_mode) {
            step
        } else {
            -step
        };
        linear_axis(
            self.ni,
            f64::from(self.lon_first) / 1_000_000.0,
            signed_step,
            max_axis_points,
            "Gaussian longitude axis",
        )
    }

    pub fn latitudes(&self) -> Result<Vec<f64>> {
        self.latitudes_with_limit(None)
    }

    pub fn latitudes_with_limit(&self, max_axis_points: Option<usize>) -> Result<Vec<f64>> {
        let requested = self.nj as usize;
        ensure_limit("Gaussian latitude axis", requested, max_axis_points)?;
        let all_latitudes = gaussian_latitudes(self.number_of_parallels, max_axis_points)?;
        if requested > all_latitudes.len() {
            return Err(Error::InvalidSection {
                section: 3,
                reason: format!(
                    "Gaussian grid requests {} rows but N={} defines only {} parallels",
                    self.nj,
                    self.number_of_parallels,
                    all_latitudes.len()
                ),
            });
        }
        if requested == 0 {
            return Ok(Vec::new());
        }

        let first = f64::from(self.lat_first) / 1_000_000.0;
        let first_index = all_latitudes
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                (*left - first).abs().total_cmp(&(*right - first).abs())
            })
            .map(|(index, _)| index)
            .ok_or_else(|| Error::InvalidSection {
                section: 3,
                reason: "Gaussian grid has no latitude parallels".into(),
            })?;
        if (all_latitudes[first_index] - first).abs() > 0.001 {
            return Err(Error::InvalidSection {
                section: 3,
                reason: format!(
                    "first latitude {first} is not a Gaussian parallel for N={}",
                    self.number_of_parallels
                ),
            });
        }

        let ascending = j_scans_positive(self.scanning_mode);
        let last_index = if ascending {
            first_index.checked_sub(requested - 1)
        } else {
            first_index.checked_add(requested - 1)
        }
        .filter(|index| *index < all_latitudes.len())
        .ok_or_else(|| Error::InvalidSection {
            section: 3,
            reason: "Gaussian latitude scan extends beyond the defined parallels".into(),
        })?;

        let mut result = Vec::new();
        result.try_reserve_exact(requested).map_err(|error| {
            Error::allocation("Gaussian latitude coordinates", requested, error)
        })?;
        if ascending {
            result.extend(
                (last_index..=first_index)
                    .rev()
                    .map(|index| all_latitudes[index]),
            );
        } else {
            result.extend(all_latitudes[first_index..=last_index].iter().copied());
        }

        let declared_last = f64::from(self.lat_last) / 1_000_000.0;
        if (result[requested - 1] - declared_last).abs() > 0.001 {
            return Err(Error::InvalidSection {
                section: 3,
                reason: format!(
                    "last latitude {declared_last} does not match the Gaussian scan endpoint {}",
                    result[requested - 1]
                ),
            });
        }
        Ok(result)
    }

    pub fn validate_supported_scan_order(&self) -> Result<()> {
        validate_supported_scan_order(self.scanning_mode)
    }

    fn longitude_increment_degrees(&self) -> Result<f64> {
        if self.di != u32::MAX {
            let increment = f64::from(self.di) / 1_000_000.0;
            self.validate_longitude_endpoint(increment)?;
            return Ok(increment);
        }
        if self.ni == 0 {
            return Err(Error::InvalidSection {
                section: 3,
                reason: "Gaussian grid has missing Di and zero Ni".into(),
            });
        }
        let inferred = 360.0 / f64::from(self.ni);
        self.validate_longitude_endpoint(inferred)?;
        Ok(inferred)
    }

    fn validate_longitude_endpoint(&self, increment: f64) -> Result<()> {
        if self.ni == 0 {
            return Err(Error::InvalidSection {
                section: 3,
                reason: "Gaussian grid has zero Ni".into(),
            });
        }
        let expected_last = f64::from(self.lon_first) / 1_000_000.0
            + if i_scans_positive(self.scanning_mode) {
                increment * f64::from(self.ni - 1)
            } else {
                -increment * f64::from(self.ni - 1)
            };
        let declared_last = f64::from(self.lon_last) / 1_000_000.0;
        let wrapped_difference = (expected_last - declared_last + 180.0).rem_euclid(360.0) - 180.0;
        if wrapped_difference.abs() > 0.001 {
            return Err(Error::InvalidSection {
                section: 3,
                reason: "Gaussian longitude endpoint does not match Ni, Di, and scanning mode"
                    .into(),
            });
        }
        Ok(())
    }
}

fn gaussian_latitudes(
    number_of_parallels: u32,
    max_axis_points: Option<usize>,
) -> Result<Vec<f64>> {
    if number_of_parallels == 0 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: "Gaussian grid N must be nonzero".into(),
        });
    }
    let order = usize::try_from(u64::from(number_of_parallels) * 2)
        .map_err(|_| Error::ValueOutOfRange("Gaussian parallel count does not fit usize".into()))?;
    ensure_limit("Gaussian parallel calculation", order, max_axis_points)?;
    let mut latitudes = filled_vec(order, 0.0, "Gaussian latitude coordinates")?;
    let order_f64 = order as f64;

    for index in 0..order / 2 {
        let mut root = (std::f64::consts::PI * (index as f64 + 0.75) / (order_f64 + 0.5)).cos();
        let mut converged = false;
        for _ in 0..32 {
            let (polynomial, previous) = legendre_pair(order, root);
            let derivative = order_f64 * (root * polynomial - previous) / (root * root - 1.0);
            let next = root - polynomial / derivative;
            if (next - root).abs() <= 4.0 * f64::EPSILON * next.abs().max(1.0) {
                root = next;
                converged = true;
                break;
            }
            root = next;
        }
        if !converged || !root.is_finite() {
            return Err(Error::InvalidSection {
                section: 3,
                reason: format!(
                    "failed to compute Gaussian latitude root {index} for N={number_of_parallels}"
                ),
            });
        }
        let latitude = root.asin().to_degrees();
        latitudes[index] = latitude;
        latitudes[order - index - 1] = -latitude;
    }
    Ok(latitudes)
}

fn legendre_pair(order: usize, value: f64) -> (f64, f64) {
    let mut previous = 0.0;
    let mut current = 1.0;
    for degree in 1..=order {
        let older = previous;
        previous = current;
        current = ((2 * degree - 1) as f64 * value * previous - (degree - 1) as f64 * older)
            / degree as f64;
    }
    (current, previous)
}

impl ProjectedGridCore {
    pub fn x_coordinates(&self) -> Result<Vec<f64>> {
        self.x_coordinates_with_limit(None)
    }

    pub fn x_coordinates_with_limit(&self, max_axis_points: Option<usize>) -> Result<Vec<f64>> {
        projected_x_coordinates(self.nx, self.dx, self.scanning_mode, max_axis_points)
    }

    pub fn y_coordinates(&self) -> Result<Vec<f64>> {
        self.y_coordinates_with_limit(None)
    }

    pub fn y_coordinates_with_limit(&self, max_axis_points: Option<usize>) -> Result<Vec<f64>> {
        projected_y_coordinates(self.ny, self.dy, self.scanning_mode, max_axis_points)
    }

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
    let expected = ni.checked_mul(nj).ok_or(Error::ArithmeticOverflow {
        operation: "computing grid point count",
    })?;
    if values.len() != expected {
        return Err(Error::DataLengthMismatch {
            expected,
            actual: values.len(),
        });
    }

    if adjacent_rows_alternate_direction(scanning_mode) {
        reverse_alternating_rows(values, ni, nj, i_scans_positive(scanning_mode));
    }

    Ok(())
}

fn normalize_north_up_in_place<T>(
    values: &mut [T],
    ni: usize,
    nj: usize,
    scanning_mode: u8,
) -> Result<()> {
    transform_supported_scan_order_in_place(values, ni, nj, scanning_mode)?;
    if ni == 0 {
        return Ok(());
    }

    if !i_scans_positive(scanning_mode) {
        for row in values.chunks_exact_mut(ni) {
            row.reverse();
        }
    }
    if j_scans_positive(scanning_mode) {
        // Reversing the whole slice reverses both row order and each row;
        // reverse each row again to preserve column orientation.
        values.reverse();
        for row in values.chunks_exact_mut(ni) {
            row.reverse();
        }
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

fn projected_x_coordinates(
    count: u32,
    spacing_millimetres: u32,
    scanning_mode: u8,
    max_axis_points: Option<usize>,
) -> Result<Vec<f64>> {
    projected_axis(
        count,
        spacing_millimetres,
        i_scans_positive(scanning_mode),
        max_axis_points,
        "projected x axis",
    )
}

fn projected_y_coordinates(
    count: u32,
    spacing_millimetres: u32,
    scanning_mode: u8,
    max_axis_points: Option<usize>,
) -> Result<Vec<f64>> {
    projected_axis(
        count,
        spacing_millimetres,
        j_scans_positive(scanning_mode),
        max_axis_points,
        "projected y axis",
    )
}

fn projected_axis(
    count: u32,
    spacing_millimetres: u32,
    scans_positive: bool,
    max_axis_points: Option<usize>,
    what: &'static str,
) -> Result<Vec<f64>> {
    let step = f64::from(spacing_millimetres) / 1_000.0;
    let signed_step = if scans_positive { step } else { -step };
    linear_axis(count, 0.0, signed_step, max_axis_points, what)
}

fn linear_axis(
    count: u32,
    start: f64,
    signed_step: f64,
    max_axis_points: Option<usize>,
    what: &'static str,
) -> Result<Vec<f64>> {
    let count = count as usize;
    ensure_limit(what, count, max_axis_points)?;
    let mut axis = Vec::new();
    axis.try_reserve(count)
        .map_err(|error| Error::allocation(format!("{what} coordinates"), count, error))?;
    axis.extend((0..count).map(|index| start + signed_step * index as f64));
    Ok(axis)
}

fn checked_grid_point_count(nx: u32, ny: u32) -> Result<usize> {
    let count = u64::from(nx) * u64::from(ny);
    usize::try_from(count).map_err(|_| {
        Error::ValueOutOfRange(format!("grid point count {count} does not fit in usize"))
    })
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

    reject_quasi_regular_grid(data, 0)?;
    require_microdegree_angular_unit(data, 0)?;
    let grid = parse_latlon_fields(data);
    validate_geographic_coordinates(
        grid.lat_first,
        grid.lon_first,
        grid.lat_last,
        grid.lon_last,
        0,
    )?;
    Ok(GridDefinition::LatLon(grid))
}

fn parse_rotated_latlon(data: &[u8]) -> Result<GridDefinition> {
    if data.len() < 84 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.1 requires 84 bytes, got {}", data.len()),
        });
    }

    reject_quasi_regular_grid(data, 1)?;
    require_microdegree_angular_unit(data, 1)?;
    let grid = parse_latlon_fields(data);
    validate_geographic_coordinates(
        grid.lat_first,
        grid.lon_first,
        grid.lat_last,
        grid.lon_last,
        1,
    )?;

    let lat_southern_pole = decode_wmo_i32(&data[72..76]).unwrap();
    let lon_southern_pole = u32::from_be_bytes(data[76..80].try_into().unwrap());
    if lat_southern_pole.unsigned_abs() > 90_000_000 || lon_southern_pole > 360_000_000 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: "rotated-grid southern pole is outside geographic bounds".into(),
        });
    }
    let angle_of_rotation = f32::from_be_bytes(data[80..84].try_into().unwrap());
    if !angle_of_rotation.is_finite() {
        return Err(Error::InvalidSection {
            section: 3,
            reason: "rotated-grid angle of rotation must be finite".into(),
        });
    }

    Ok(GridDefinition::RotatedLatLon(RotatedLatLonGrid {
        grid,
        lat_southern_pole,
        lon_southern_pole,
        angle_of_rotation,
    }))
}

fn parse_regular_gaussian(data: &[u8]) -> Result<GridDefinition> {
    if data.len() < 72 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.40 requires 72 bytes, got {}", data.len()),
        });
    }

    reject_quasi_regular_grid(data, 40)?;
    require_microdegree_angular_unit(data, 40)?;
    let grid = RegularGaussianGrid {
        ni: u32::from_be_bytes(data[30..34].try_into().unwrap()),
        nj: u32::from_be_bytes(data[34..38].try_into().unwrap()),
        lat_first: decode_wmo_i32(&data[46..50]).unwrap(),
        lon_first: decode_wmo_i32(&data[50..54]).unwrap(),
        lat_last: decode_wmo_i32(&data[55..59]).unwrap(),
        lon_last: decode_wmo_i32(&data[59..63]).unwrap(),
        di: u32::from_be_bytes(data[63..67].try_into().unwrap()),
        number_of_parallels: u32::from_be_bytes(data[67..71].try_into().unwrap()),
        scanning_mode: data[71],
    };
    validate_geographic_coordinates(
        grid.lat_first,
        grid.lon_first,
        grid.lat_last,
        grid.lon_last,
        40,
    )?;
    if grid.number_of_parallels == 0 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: "template 3.40 number of parallels N must be nonzero".into(),
        });
    }
    let available_rows = u64::from(grid.number_of_parallels) * 2;
    if u64::from(grid.nj) > available_rows {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!(
                "template 3.40 Nj={} exceeds the {available_rows} parallels defined by N={}",
                grid.nj, grid.number_of_parallels
            ),
        });
    }
    Ok(GridDefinition::RegularGaussian(grid))
}

fn parse_latlon_fields(data: &[u8]) -> LatLonGrid {
    LatLonGrid {
        ni: u32::from_be_bytes(data[30..34].try_into().unwrap()),
        nj: u32::from_be_bytes(data[34..38].try_into().unwrap()),
        lat_first: decode_wmo_i32(&data[46..50]).unwrap(),
        lon_first: decode_wmo_i32(&data[50..54]).unwrap(),
        lat_last: decode_wmo_i32(&data[55..59]).unwrap(),
        lon_last: decode_wmo_i32(&data[59..63]).unwrap(),
        di: u32::from_be_bytes(data[63..67].try_into().unwrap()),
        dj: u32::from_be_bytes(data[67..71].try_into().unwrap()),
        scanning_mode: data[71],
    }
}

fn reject_quasi_regular_grid(data: &[u8], template: u16) -> Result<()> {
    let ni = u32::from_be_bytes(data[30..34].try_into().unwrap());
    let nj = u32::from_be_bytes(data[34..38].try_into().unwrap());
    if ni == u32::MAX || nj == u32::MAX || data[10] != 0 {
        return Err(Error::UnsupportedQuasiRegularGrid { template });
    }
    Ok(())
}

fn require_microdegree_angular_unit(data: &[u8], template: u16) -> Result<()> {
    let encoded_basic_angle = u32::from_be_bytes(data[38..42].try_into().unwrap());
    let encoded_subdivisions = u32::from_be_bytes(data[42..46].try_into().unwrap());
    let basic_angle = match encoded_basic_angle {
        0 | u32::MAX => 1,
        value => value,
    };
    let subdivisions = match encoded_subdivisions {
        0 | u32::MAX => 1_000_000,
        value => value,
    };
    if u64::from(basic_angle) * 1_000_000 != u64::from(subdivisions) {
        return Err(Error::UnsupportedGridAngularUnit {
            template,
            basic_angle,
            subdivisions,
        });
    }
    Ok(())
}

fn validate_geographic_coordinates(
    lat_first: i32,
    lon_first: i32,
    lat_last: i32,
    lon_last: i32,
    template: u16,
) -> Result<()> {
    if lat_first.unsigned_abs() > 90_000_000
        || lat_last.unsigned_abs() > 90_000_000
        || lon_first.unsigned_abs() > 360_000_000
        || lon_last.unsigned_abs() > 360_000_000
    {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.{template} coordinates are outside geographic bounds"),
        });
    }
    Ok(())
}

fn parse_mercator(data: &[u8]) -> Result<GridDefinition> {
    if data.len() < 72 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.10 requires 72 bytes, got {}", data.len()),
        });
    }

    Ok(GridDefinition::Mercator(MercatorGrid {
        core: parse_projected_core(data, 64, 59),
        lat_d: decode_wmo_i32(&data[47..51]).unwrap(),
        lat_last: decode_wmo_i32(&data[51..55]).unwrap(),
        lon_last: decode_wmo_i32(&data[55..59]).unwrap(),
        orientation_of_grid: decode_wmo_i32(&data[60..64]).unwrap(),
    }))
}

fn parse_polar_stereographic(data: &[u8]) -> Result<GridDefinition> {
    if data.len() < 65 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.20 requires 65 bytes, got {}", data.len()),
        });
    }

    Ok(GridDefinition::PolarStereographic(PolarStereographicGrid {
        core: parse_projected_core(data, 55, 64),
        lat_d: decode_wmo_i32(&data[47..51]).unwrap(),
        lon_v: decode_wmo_i32(&data[51..55]).unwrap(),
        projection_center_flag: data[63],
    }))
}

fn parse_albers_equal_area(data: &[u8]) -> Result<GridDefinition> {
    if data.len() < 81 {
        return Err(Error::InvalidSection {
            section: 3,
            reason: format!("template 3.31 requires 81 bytes, got {}", data.len()),
        });
    }

    Ok(GridDefinition::AlbersEqualArea(AlbersEqualAreaGrid {
        core: parse_projected_core(data, 55, 64),
        lat_d: decode_wmo_i32(&data[47..51]).unwrap(),
        lon_v: decode_wmo_i32(&data[51..55]).unwrap(),
        projection_center_flag: data[63],
        latin1: decode_wmo_i32(&data[65..69]).unwrap(),
        latin2: decode_wmo_i32(&data[69..73]).unwrap(),
        lat_southern_pole: decode_wmo_i32(&data[73..77]).unwrap(),
        lon_southern_pole: decode_wmo_i32(&data[77..81]).unwrap(),
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
        core: parse_projected_core(data, 55, 64),
        lat_d: decode_wmo_i32(&data[47..51]).unwrap(),
        lon_v: decode_wmo_i32(&data[51..55]).unwrap(),
        projection_center_flag: data[63],
        latin1: decode_wmo_i32(&data[65..69]).unwrap(),
        latin2: decode_wmo_i32(&data[69..73]).unwrap(),
        lat_southern_pole: decode_wmo_i32(&data[73..77]).unwrap(),
        lon_southern_pole: decode_wmo_i32(&data[77..81]).unwrap(),
    }))
}

fn parse_projected_core(
    data: &[u8],
    spacing_offset: usize,
    scanning_mode_offset: usize,
) -> ProjectedGridCore {
    ProjectedGridCore {
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
        lat_first: decode_wmo_i32(&data[38..42]).unwrap(),
        lon_first: decode_wmo_i32(&data[42..46]).unwrap(),
        resolution_and_component_flags: data[46],
        dx: u32::from_be_bytes(data[spacing_offset..spacing_offset + 4].try_into().unwrap()),
        dy: u32::from_be_bytes(
            data[spacing_offset + 4..spacing_offset + 8]
                .try_into()
                .unwrap(),
        ),
        scanning_mode: data[scanning_mode_offset],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AlbersEqualAreaGrid, GridDefinition, LambertConformalGrid, LatLonGrid, MercatorGrid,
        PolarStereographicGrid, ProjectedGridCore, RegularGaussianGrid,
    };
    use crate::binary::encode_wmo_i32;

    fn projected_core(nx: u32, ny: u32, dx: u32, dy: u32, scanning_mode: u8) -> ProjectedGridCore {
        ProjectedGridCore {
            number_of_points: nx * ny,
            shape_of_earth: 6,
            scale_factor_radius: 0,
            scaled_value_radius: 0,
            scale_factor_major_axis: 0,
            scaled_value_major_axis: 0,
            scale_factor_minor_axis: 0,
            scaled_value_minor_axis: 0,
            nx,
            ny,
            lat_first: 0,
            lon_first: 0,
            resolution_and_component_flags: 0,
            dx,
            dy,
            scanning_mode,
        }
    }

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
        assert!(grid.as_mercator().is_none());
        assert!(grid.as_polar_stereographic().is_none());
        assert!(grid.as_lambert_conformal().is_none());
        assert!(grid.as_albers_equal_area().is_none());
        assert_eq!(grid.unsupported_template(), None);
        match grid {
            GridDefinition::LatLon(ref latlon) => {
                assert_eq!(latlon.longitudes().unwrap(), vec![-120.0, -119.0, -118.0]);
                assert_eq!(latlon.latitudes().unwrap(), vec![50.0, 49.0]);
            }
            other => panic!("expected lat/lon grid, got {other:?}"),
        }
    }

    #[test]
    fn parses_mercator_template() {
        let section = build_mercator_section(0);
        let grid = GridDefinition::parse(&section).unwrap();

        assert_eq!(grid.shape(), (3, 2));
        assert_eq!(grid.ndarray_shape(), vec![2, 3]);
        assert_eq!(grid.num_points(), 6);
        assert_eq!(grid.template_number(), 10);
        assert!(grid.as_lat_lon().is_none());
        assert!(grid.as_mercator().is_some());
        assert!(grid.as_lambert_conformal().is_none());
        assert_eq!(grid.unsupported_template(), None);
        match grid {
            GridDefinition::Mercator(mercator) => {
                assert_eq!(mercator.core.number_of_points, 6);
                assert_eq!(mercator.core.shape_of_earth, 6);
                assert_eq!(mercator.core.nx, 3);
                assert_eq!(mercator.core.ny, 2);
                assert_eq!(mercator.core.lat_first, -20_000_000);
                assert_eq!(mercator.core.lon_first, -100_000_000);
                assert_eq!(mercator.core.resolution_and_component_flags, 0x08);
                assert_eq!(mercator.lat_d, 0);
                assert_eq!(mercator.lat_last, 20_000_000);
                assert_eq!(mercator.lon_last, -98_000_000);
                assert_eq!(mercator.core.scanning_mode, 0);
                assert_eq!(mercator.orientation_of_grid, 0);
                assert_eq!(mercator.core.dx, 1_000_000);
                assert_eq!(mercator.core.dy, 2_000_000);
            }
            other => panic!("expected Mercator grid, got {other:?}"),
        }
    }

    #[test]
    fn parses_rotated_latlon_template() {
        let section = build_rotated_latlon_section();
        let grid = GridDefinition::parse(&section).unwrap();

        assert_eq!(grid.shape(), (3, 2));
        assert_eq!(grid.ndarray_shape(), vec![2, 3]);
        assert_eq!(grid.template_number(), 1);
        assert!(grid.as_lat_lon().is_none());
        let rotated = grid.as_rotated_lat_lon().unwrap();
        assert_eq!(rotated.lat_southern_pole, -30_000_000);
        assert_eq!(rotated.lon_southern_pole, 10_000_000);
        assert_eq!(rotated.angle_of_rotation, 15.5);
        assert_eq!(
            rotated.rotated_longitudes().unwrap(),
            vec![-20.0, 0.0, 20.0]
        );
        assert_eq!(rotated.rotated_latitudes().unwrap(), vec![-10.0, 0.0]);
        assert_eq!(grid.latitudes().unwrap(), None);
    }

    #[test]
    fn rejects_non_finite_rotated_grid_angle() {
        let mut section = build_rotated_latlon_section();
        section[80..84].copy_from_slice(&f32::NAN.to_be_bytes());
        assert!(matches!(
            GridDefinition::parse(&section),
            Err(crate::Error::InvalidSection { section: 3, .. })
        ));
    }

    #[test]
    fn parses_regular_gaussian_and_computes_latitudes() {
        let section = build_regular_gaussian_section(90_000_000);
        let grid = GridDefinition::parse(&section).unwrap();

        assert_eq!(grid.shape(), (4, 4));
        assert_eq!(grid.ndarray_shape(), vec![4, 4]);
        assert_eq!(grid.template_number(), 40);
        let gaussian = grid.as_regular_gaussian().unwrap();
        assert_eq!(
            gaussian,
            &RegularGaussianGrid {
                ni: 4,
                nj: 4,
                lat_first: 59_444_408,
                lon_first: 0,
                lat_last: -59_444_408,
                lon_last: 270_000_000,
                di: 90_000_000,
                number_of_parallels: 2,
                scanning_mode: 0,
            }
        );
        let latitudes = gaussian.latitudes().unwrap();
        let expected = [
            59.444_408_289_166_77,
            19.875_719_147_440_904,
            -19.875_719_147_440_904,
            -59.444_408_289_166_77,
        ];
        for (actual, expected) in latitudes.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12);
        }
        assert_eq!(
            gaussian.longitudes().unwrap(),
            vec![0.0, 90.0, 180.0, 270.0]
        );
        assert_eq!(grid.latitudes().unwrap().unwrap(), latitudes);
    }

    #[test]
    fn infers_missing_gaussian_increment_for_global_grid() {
        let section = build_regular_gaussian_section(u32::MAX);
        let GridDefinition::RegularGaussian(grid) = GridDefinition::parse(&section).unwrap() else {
            panic!("expected regular Gaussian grid");
        };
        assert_eq!(grid.longitudes().unwrap(), vec![0.0, 90.0, 180.0, 270.0]);
    }

    #[test]
    fn rejects_inconsistent_gaussian_longitude_endpoint() {
        let mut section = build_regular_gaussian_section(90_000_000);
        section[59..63].copy_from_slice(&encode_wmo_i32(180_000_000).unwrap());
        let GridDefinition::RegularGaussian(grid) = GridDefinition::parse(&section).unwrap() else {
            panic!("expected regular Gaussian grid");
        };
        assert!(matches!(
            grid.longitudes(),
            Err(crate::Error::InvalidSection { section: 3, .. })
        ));
    }

    #[test]
    fn rejects_quasi_regular_geographic_grids_with_typed_error() {
        for (template, mut section) in [
            (1, build_rotated_latlon_section()),
            (40, build_regular_gaussian_section(90_000_000)),
        ] {
            section[30..34].copy_from_slice(&u32::MAX.to_be_bytes());
            section[10] = 2;
            assert!(matches!(
                GridDefinition::parse(&section),
                Err(crate::Error::UnsupportedQuasiRegularGrid { template: actual })
                    if actual == template
            ));
        }
    }

    #[test]
    fn rejects_non_microdegree_geographic_angle_units_explicitly() {
        let mut section = build_regular_gaussian_section(90_000_000);
        section[38..42].copy_from_slice(&1u32.to_be_bytes());
        section[42..46].copy_from_slice(&1_000u32.to_be_bytes());
        assert!(matches!(
            GridDefinition::parse(&section),
            Err(crate::Error::UnsupportedGridAngularUnit {
                template: 40,
                basic_angle: 1,
                subdivisions: 1_000,
            })
        ));
    }

    #[test]
    fn parses_polar_stereographic_template() {
        let section = build_polar_stereographic_section(0);
        let grid = GridDefinition::parse(&section).unwrap();

        assert_eq!(grid.shape(), (3, 2));
        assert_eq!(grid.ndarray_shape(), vec![2, 3]);
        assert_eq!(grid.num_points(), 6);
        assert_eq!(grid.template_number(), 20);
        assert!(grid.as_lat_lon().is_none());
        assert!(grid.as_mercator().is_none());
        assert!(grid.as_polar_stereographic().is_some());
        assert!(grid.as_lambert_conformal().is_none());
        assert!(grid.as_albers_equal_area().is_none());
        assert_eq!(grid.unsupported_template(), None);
        match grid {
            GridDefinition::PolarStereographic(polar) => {
                assert_eq!(polar.core.number_of_points, 6);
                assert_eq!(polar.core.shape_of_earth, 6);
                assert_eq!(polar.core.nx, 3);
                assert_eq!(polar.core.ny, 2);
                assert_eq!(polar.core.lat_first, 41_612_949);
                assert_eq!(polar.core.lon_first, 185_117_126);
                assert_eq!(polar.core.resolution_and_component_flags, 0x08);
                assert_eq!(polar.lat_d, 60_000_000);
                assert_eq!(polar.lon_v, 225_000_000);
                assert_eq!(polar.core.dx, 3_000_000);
                assert_eq!(polar.core.dy, 3_000_000);
                assert_eq!(polar.projection_center_flag, 0);
                assert_eq!(polar.core.scanning_mode, 0);
            }
            other => panic!("expected polar stereographic grid, got {other:?}"),
        }
    }

    #[test]
    fn parses_albers_equal_area_template() {
        let section = build_albers_section();
        let grid = GridDefinition::parse(&section).unwrap();

        assert_eq!(grid.shape(), (3, 2));
        assert_eq!(grid.ndarray_shape(), vec![2, 3]);
        assert_eq!(grid.num_points(), 6);
        assert_eq!(grid.template_number(), 31);
        assert!(grid.as_lat_lon().is_none());
        assert!(grid.as_albers_equal_area().is_some());
        assert_eq!(grid.unsupported_template(), None);
        match grid {
            GridDefinition::AlbersEqualArea(albers) => {
                assert_eq!(albers.core.number_of_points, 6);
                assert_eq!(albers.core.shape_of_earth, 6);
                assert_eq!(albers.core.nx, 3);
                assert_eq!(albers.core.ny, 2);
                assert_eq!(albers.core.lat_first, 23_000_000);
                assert_eq!(albers.core.lon_first, 240_000_000);
                assert_eq!(albers.core.resolution_and_component_flags, 0x08);
                assert_eq!(albers.lat_d, 25_000_000);
                assert_eq!(albers.lon_v, 265_000_000);
                assert_eq!(albers.core.dx, 4_000_000);
                assert_eq!(albers.core.dy, 5_000_000);
                assert_eq!(albers.projection_center_flag, 0);
                assert_eq!(albers.core.scanning_mode, 0);
                assert_eq!(albers.latin1, 29_500_000);
                assert_eq!(albers.latin2, 45_500_000);
                assert_eq!(albers.lat_southern_pole, -90_000_000);
                assert_eq!(albers.lon_southern_pole, 0);
            }
            other => panic!("expected Albers equal-area grid, got {other:?}"),
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
        assert!(grid.as_albers_equal_area().is_none());
        assert_eq!(grid.unsupported_template(), None);
        match grid {
            GridDefinition::LambertConformal(lambert) => {
                assert_eq!(lambert.core.number_of_points, 6);
                assert_eq!(lambert.core.shape_of_earth, 1);
                assert_eq!(lambert.core.scaled_value_radius, 6_371_200);
                assert_eq!(lambert.core.nx, 3);
                assert_eq!(lambert.core.ny, 2);
                assert_eq!(lambert.core.lat_first, 12_190_000);
                assert_eq!(lambert.core.lon_first, 226_541_000);
                assert_eq!(lambert.core.resolution_and_component_flags, 0x08);
                assert_eq!(lambert.lat_d, 25_000_000);
                assert_eq!(lambert.lon_v, 265_000_000);
                assert_eq!(lambert.core.dx, 2_539_703);
                assert_eq!(lambert.core.dy, 2_539_703);
                assert_eq!(lambert.projection_center_flag, 0);
                assert_eq!(lambert.core.scanning_mode, 0);
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
        assert!(grid.as_mercator().is_none());
        assert!(grid.as_polar_stereographic().is_none());
        assert!(grid.as_lambert_conformal().is_none());
        assert!(grid.as_albers_equal_area().is_none());
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
    fn normalizes_polar_stereographic_alternating_row_scan() {
        let grid = PolarStereographicGrid {
            core: projected_core(3, 2, 1, 1, 0b0001_0000),
            lat_d: 0,
            lon_v: 0,
            projection_center_flag: 0,
        };

        let mut values = vec![1.0, 2.0, 3.0, 6.0, 5.0, 4.0];
        grid.core.reorder_for_ndarray_in_place(&mut values).unwrap();
        assert_eq!(values, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn normalizes_mercator_alternating_row_scan() {
        let grid = MercatorGrid {
            core: projected_core(3, 2, 1, 1, 0b0001_0000),
            lat_d: 0,
            lat_last: 0,
            lon_last: 0,
            orientation_of_grid: 0,
        };

        let mut values = vec![1.0, 2.0, 3.0, 6.0, 5.0, 4.0];
        grid.core.reorder_for_ndarray_in_place(&mut values).unwrap();
        assert_eq!(values, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn normalizes_lambert_alternating_row_scan() {
        let grid = LambertConformalGrid {
            core: ProjectedGridCore {
                shape_of_earth: 1,
                scaled_value_radius: 6_371_200,
                ..projected_core(3, 2, 1, 1, 0b0001_0000)
            },
            lat_d: 0,
            lon_v: 0,
            projection_center_flag: 0,
            latin1: 0,
            latin2: 0,
            lat_southern_pole: 0,
            lon_southern_pole: 0,
        };

        let mut values = vec![1.0, 2.0, 3.0, 6.0, 5.0, 4.0];
        grid.core.reorder_for_ndarray_in_place(&mut values).unwrap();
        assert_eq!(values, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn normalizes_albers_alternating_row_scan() {
        let grid = AlbersEqualAreaGrid {
            core: projected_core(3, 2, 1, 1, 0b0001_0000),
            lat_d: 0,
            lon_v: 0,
            projection_center_flag: 0,
            latin1: 0,
            latin2: 0,
            lat_southern_pole: 0,
            lon_southern_pole: 0,
        };

        let mut values = vec![1.0, 2.0, 3.0, 6.0, 5.0, 4.0];
        grid.core.reorder_for_ndarray_in_place(&mut values).unwrap();
        assert_eq!(values, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn generates_projected_coordinate_offsets() {
        let mercator = GridDefinition::Mercator(MercatorGrid {
            core: projected_core(3, 2, 1_000_000, 2_000_000, 0),
            lat_d: 0,
            lat_last: 0,
            lon_last: 0,
            orientation_of_grid: 0,
        });
        assert_eq!(
            mercator.projected_x_coordinates().unwrap().unwrap(),
            vec![0.0, 1_000.0, 2_000.0]
        );
        assert_eq!(
            mercator.projected_y_coordinates().unwrap().unwrap(),
            vec![-0.0, -2_000.0]
        );

        let polar = GridDefinition::PolarStereographic(PolarStereographicGrid {
            core: projected_core(3, 2, 3_000_000, 6_000_000, 0),
            lat_d: 0,
            lon_v: 0,
            projection_center_flag: 0,
        });
        assert_eq!(
            polar.projected_x_coordinates().unwrap().unwrap(),
            vec![0.0, 3_000.0, 6_000.0]
        );
        assert_eq!(
            polar.projected_y_coordinates().unwrap().unwrap(),
            vec![-0.0, -6_000.0]
        );

        let lambert = GridDefinition::LambertConformal(LambertConformalGrid {
            core: ProjectedGridCore {
                shape_of_earth: 1,
                scaled_value_radius: 6_371_200,
                ..projected_core(3, 2, 2_539_703, 2_539_703, 0b1100_0000)
            },
            lat_d: 0,
            lon_v: 0,
            projection_center_flag: 0,
            latin1: 0,
            latin2: 0,
            lat_southern_pole: 0,
            lon_southern_pole: 0,
        });
        assert_eq!(
            lambert.projected_x_coordinates().unwrap().unwrap(),
            vec![-0.0, -2_539.703, -5_079.406]
        );
        assert_eq!(
            lambert.projected_y_coordinates().unwrap().unwrap(),
            vec![0.0, 2_539.703]
        );

        let albers = GridDefinition::AlbersEqualArea(AlbersEqualAreaGrid {
            core: projected_core(3, 2, 4_000_000, 5_000_000, 0b1100_0000),
            lat_d: 0,
            lon_v: 0,
            projection_center_flag: 0,
            latin1: 0,
            latin2: 0,
            lat_southern_pole: 0,
            lon_southern_pole: 0,
        });
        assert_eq!(
            albers.projected_x_coordinates().unwrap().unwrap(),
            vec![-0.0, -4_000.0, -8_000.0]
        );
        assert_eq!(
            albers.projected_y_coordinates().unwrap().unwrap(),
            vec![0.0, 5_000.0]
        );
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
    fn normalizes_scan_directions_to_north_up_west_first_order() {
        let grid = GridDefinition::LatLon(LatLonGrid {
            ni: 3,
            nj: 2,
            lat_first: 0,
            lon_first: 0,
            lat_last: 0,
            lon_last: 0,
            di: 1,
            dj: 1,
            scanning_mode: 0b1100_0000,
        });
        let mut values = vec![1, 2, 3, 4, 5, 6];

        grid.normalize_north_up_in_place(&mut values).unwrap();

        assert_eq!(values, vec![6, 5, 4, 3, 2, 1]);
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

    fn build_polar_stereographic_section(scanning_mode: u8) -> Vec<u8> {
        let mut section = vec![0u8; 65];
        section[..4].copy_from_slice(&65u32.to_be_bytes());
        section[4] = 3;
        section[6..10].copy_from_slice(&6u32.to_be_bytes());
        section[12..14].copy_from_slice(&20u16.to_be_bytes());
        section[14] = 6;
        section[30..34].copy_from_slice(&3u32.to_be_bytes());
        section[34..38].copy_from_slice(&2u32.to_be_bytes());
        section[38..42].copy_from_slice(&encode_wmo_i32(41_612_949).unwrap());
        section[42..46].copy_from_slice(&185_117_126u32.to_be_bytes());
        section[46] = 0x08;
        section[47..51].copy_from_slice(&encode_wmo_i32(60_000_000).unwrap());
        section[51..55].copy_from_slice(&225_000_000u32.to_be_bytes());
        section[55..59].copy_from_slice(&3_000_000u32.to_be_bytes());
        section[59..63].copy_from_slice(&3_000_000u32.to_be_bytes());
        section[64] = scanning_mode;
        section
    }

    fn build_rotated_latlon_section() -> Vec<u8> {
        let mut section = vec![0u8; 84];
        section[..4].copy_from_slice(&84u32.to_be_bytes());
        section[4] = 3;
        section[6..10].copy_from_slice(&6u32.to_be_bytes());
        section[12..14].copy_from_slice(&1u16.to_be_bytes());
        section[30..34].copy_from_slice(&3u32.to_be_bytes());
        section[34..38].copy_from_slice(&2u32.to_be_bytes());
        section[46..50].copy_from_slice(&encode_wmo_i32(-10_000_000).unwrap());
        section[50..54].copy_from_slice(&encode_wmo_i32(-20_000_000).unwrap());
        section[55..59].copy_from_slice(&encode_wmo_i32(0).unwrap());
        section[59..63].copy_from_slice(&encode_wmo_i32(20_000_000).unwrap());
        section[63..67].copy_from_slice(&20_000_000u32.to_be_bytes());
        section[67..71].copy_from_slice(&10_000_000u32.to_be_bytes());
        section[71] = 0b0100_0000;
        section[72..76].copy_from_slice(&encode_wmo_i32(-30_000_000).unwrap());
        section[76..80].copy_from_slice(&10_000_000u32.to_be_bytes());
        section[80..84].copy_from_slice(&15.5f32.to_be_bytes());
        section
    }

    fn build_regular_gaussian_section(di: u32) -> Vec<u8> {
        let mut section = vec![0u8; 72];
        section[..4].copy_from_slice(&72u32.to_be_bytes());
        section[4] = 3;
        section[6..10].copy_from_slice(&16u32.to_be_bytes());
        section[12..14].copy_from_slice(&40u16.to_be_bytes());
        section[30..34].copy_from_slice(&4u32.to_be_bytes());
        section[34..38].copy_from_slice(&4u32.to_be_bytes());
        section[46..50].copy_from_slice(&encode_wmo_i32(59_444_408).unwrap());
        section[50..54].copy_from_slice(&encode_wmo_i32(0).unwrap());
        section[55..59].copy_from_slice(&encode_wmo_i32(-59_444_408).unwrap());
        section[59..63].copy_from_slice(&encode_wmo_i32(270_000_000).unwrap());
        section[63..67].copy_from_slice(&di.to_be_bytes());
        section[67..71].copy_from_slice(&2u32.to_be_bytes());
        section
    }

    fn build_mercator_section(scanning_mode: u8) -> Vec<u8> {
        let mut section = vec![0u8; 72];
        section[..4].copy_from_slice(&72u32.to_be_bytes());
        section[4] = 3;
        section[6..10].copy_from_slice(&6u32.to_be_bytes());
        section[12..14].copy_from_slice(&10u16.to_be_bytes());
        section[14] = 6;
        section[30..34].copy_from_slice(&3u32.to_be_bytes());
        section[34..38].copy_from_slice(&2u32.to_be_bytes());
        section[38..42].copy_from_slice(&encode_wmo_i32(-20_000_000).unwrap());
        section[42..46].copy_from_slice(&encode_wmo_i32(-100_000_000).unwrap());
        section[46] = 0x08;
        section[47..51].copy_from_slice(&encode_wmo_i32(0).unwrap());
        section[51..55].copy_from_slice(&encode_wmo_i32(20_000_000).unwrap());
        section[55..59].copy_from_slice(&encode_wmo_i32(-98_000_000).unwrap());
        section[59] = scanning_mode;
        section[64..68].copy_from_slice(&1_000_000u32.to_be_bytes());
        section[68..72].copy_from_slice(&2_000_000u32.to_be_bytes());
        section
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

    fn build_albers_section() -> Vec<u8> {
        let mut section = vec![0u8; 81];
        section[..4].copy_from_slice(&81u32.to_be_bytes());
        section[4] = 3;
        section[6..10].copy_from_slice(&6u32.to_be_bytes());
        section[12..14].copy_from_slice(&31u16.to_be_bytes());
        section[14] = 6;
        section[30..34].copy_from_slice(&3u32.to_be_bytes());
        section[34..38].copy_from_slice(&2u32.to_be_bytes());
        section[38..42].copy_from_slice(&encode_wmo_i32(23_000_000).unwrap());
        section[42..46].copy_from_slice(&240_000_000u32.to_be_bytes());
        section[46] = 0x08;
        section[47..51].copy_from_slice(&encode_wmo_i32(25_000_000).unwrap());
        section[51..55].copy_from_slice(&265_000_000u32.to_be_bytes());
        section[55..59].copy_from_slice(&4_000_000u32.to_be_bytes());
        section[59..63].copy_from_slice(&5_000_000u32.to_be_bytes());
        section[65..69].copy_from_slice(&encode_wmo_i32(29_500_000).unwrap());
        section[69..73].copy_from_slice(&encode_wmo_i32(45_500_000).unwrap());
        section[73..77].copy_from_slice(&encode_wmo_i32(-90_000_000).unwrap());
        section
    }
}
