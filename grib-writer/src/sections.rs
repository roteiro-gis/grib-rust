use grib_core::binary::{
    encode_wmo_i16, encode_wmo_i32, encode_wmo_i8, write_u16_be, write_u32_be, write_u64_be,
    write_u8_be,
};
use grib_core::{
    AlbersEqualAreaGrid, AnalysisOrForecastTemplate, CcsdsPackingParams, ComplexPackingParams,
    DataRepresentation, FixedSurface, GridDefinition, Identification, IeeePackingParams,
    Jpeg2000PackingParams, LambertConformalGrid, LatLonGrid, MercatorGrid, PngPackingParams,
    PolarStereographicGrid, ProbabilityLimit, ProbabilityType, ProductDefinition,
    ProductDefinitionTemplate, ProjectedGridCore, ReferenceTime, ScaledPackingParams,
    StatisticalInterval, StatisticalTimeRange,
};

use crate::packing::PackedField;
use crate::{Error, Result};

fn encode_grib_i32(value: i32, field: &str) -> Result<[u8; 4]> {
    encode_wmo_i32(value)
        .ok_or_else(|| Error::ValueOutOfRange(format!("{field} does not fit GRIB signed i32")))
}

fn encode_grib_i16(value: i16, field: &str) -> Result<[u8; 2]> {
    encode_wmo_i16(value)
        .ok_or_else(|| Error::ValueOutOfRange(format!("{field} does not fit GRIB signed i16")))
}

fn encode_grib_i8(value: i16, field: &str) -> Result<u8> {
    encode_wmo_i8(value)
        .ok_or_else(|| Error::ValueOutOfRange(format!("{field} does not fit GRIB signed i8")))
}

fn checked_u32(value: usize, field: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::ValueOutOfRange(format!("{field} {value} exceeds u32")))
}

pub(crate) fn write_indicator_placeholder(out: &mut Vec<u8>, discipline: u8) -> Result<()> {
    out.extend_from_slice(b"GRIB");
    write_u16_be(out, 0)?;
    write_u8_be(out, discipline)?;
    write_u8_be(out, 2)?;
    write_u64_be(out, 0)
}

pub(crate) fn write_identification_section(
    out: &mut Vec<u8>,
    identification: &Identification,
) -> Result<()> {
    write_u32_be(out, 21)?;
    write_u8_be(out, 1)?;
    write_u16_be(out, identification.center_id)?;
    write_u16_be(out, identification.subcenter_id)?;
    write_u8_be(out, identification.master_table_version)?;
    write_u8_be(out, identification.local_table_version)?;
    write_u8_be(out, identification.significance_of_reference_time)?;
    write_u16_be(out, identification.reference_year)?;
    write_u8_be(out, identification.reference_month)?;
    write_u8_be(out, identification.reference_day)?;
    write_u8_be(out, identification.reference_hour)?;
    write_u8_be(out, identification.reference_minute)?;
    write_u8_be(out, identification.reference_second)?;
    write_u8_be(out, identification.production_status)?;
    write_u8_be(out, identification.processed_data_type)
}

pub(crate) fn write_grid_section(out: &mut Vec<u8>, grid: &GridDefinition) -> Result<()> {
    match grid {
        GridDefinition::LatLon(grid) => write_latlon_grid_section(out, grid),
        GridDefinition::Mercator(grid) => write_mercator_grid_section(out, grid),
        GridDefinition::PolarStereographic(grid) => {
            write_polar_stereographic_grid_section(out, grid)
        }
        GridDefinition::LambertConformal(grid) => write_lambert_conformal_grid_section(out, grid),
        GridDefinition::AlbersEqualArea(grid) => write_albers_equal_area_grid_section(out, grid),
        _ => Err(Error::UnsupportedGridTemplate(grid.template_number())),
    }
}

fn write_latlon_grid_section(out: &mut Vec<u8>, grid: &LatLonGrid) -> Result<()> {
    let mut section = vec![0u8; 72];
    section[..4].copy_from_slice(&72u32.to_be_bytes());
    section[4] = 3;
    section[6..10].copy_from_slice(&checked_latlon_point_count(grid)?.to_be_bytes());
    section[12..14].copy_from_slice(&0u16.to_be_bytes());
    section[30..34].copy_from_slice(&grid.ni.to_be_bytes());
    section[34..38].copy_from_slice(&grid.nj.to_be_bytes());
    section[46..50].copy_from_slice(&encode_grib_i32(
        grid.lat_first,
        "latitude of first grid point",
    )?);
    section[50..54].copy_from_slice(&encode_grib_i32(
        grid.lon_first,
        "longitude of first grid point",
    )?);
    section[55..59].copy_from_slice(&encode_grib_i32(
        grid.lat_last,
        "latitude of last grid point",
    )?);
    section[59..63].copy_from_slice(&encode_grib_i32(
        grid.lon_last,
        "longitude of last grid point",
    )?);
    section[63..67].copy_from_slice(&grid.di.to_be_bytes());
    section[67..71].copy_from_slice(&grid.dj.to_be_bytes());
    section[71] = grid.scanning_mode;
    out.extend_from_slice(&section);
    Ok(())
}

fn write_mercator_grid_section(out: &mut Vec<u8>, grid: &MercatorGrid) -> Result<()> {
    checked_projected_point_count(
        grid.core.nx,
        grid.core.ny,
        grid.core.number_of_points,
        "Mercator grid",
    )?;

    let mut section = vec![0u8; 72];
    section[..4].copy_from_slice(&72u32.to_be_bytes());
    section[4] = 3;
    section[12..14].copy_from_slice(&10u16.to_be_bytes());
    write_projected_grid_core(&mut section, &grid.core, 64, 59)?;
    section[47..51].copy_from_slice(&encode_grib_i32(
        grid.lat_d,
        "latitude where grid lengths are specified",
    )?);
    section[51..55].copy_from_slice(&encode_grib_i32(
        grid.lat_last,
        "latitude of last grid point",
    )?);
    section[55..59].copy_from_slice(&encode_grib_i32(
        grid.lon_last,
        "longitude of last grid point",
    )?);
    section[60..64].copy_from_slice(&encode_grib_i32(
        grid.orientation_of_grid,
        "Mercator orientation",
    )?);
    out.extend_from_slice(&section);
    Ok(())
}

fn write_polar_stereographic_grid_section(
    out: &mut Vec<u8>,
    grid: &PolarStereographicGrid,
) -> Result<()> {
    checked_projected_point_count(
        grid.core.nx,
        grid.core.ny,
        grid.core.number_of_points,
        "polar stereographic grid",
    )?;

    let mut section = vec![0u8; 65];
    section[..4].copy_from_slice(&65u32.to_be_bytes());
    section[4] = 3;
    section[12..14].copy_from_slice(&20u16.to_be_bytes());
    write_projected_grid_core(&mut section, &grid.core, 55, 64)?;
    section[47..51].copy_from_slice(&encode_grib_i32(
        grid.lat_d,
        "latitude where grid lengths are specified",
    )?);
    section[51..55].copy_from_slice(&encode_grib_i32(grid.lon_v, "projection longitude")?);
    section[63] = grid.projection_center_flag;
    out.extend_from_slice(&section);
    Ok(())
}

fn write_albers_equal_area_grid_section(
    out: &mut Vec<u8>,
    grid: &AlbersEqualAreaGrid,
) -> Result<()> {
    checked_projected_point_count(
        grid.core.nx,
        grid.core.ny,
        grid.core.number_of_points,
        "Albers equal-area grid",
    )?;

    let mut section = vec![0u8; 81];
    section[..4].copy_from_slice(&81u32.to_be_bytes());
    section[4] = 3;
    section[12..14].copy_from_slice(&31u16.to_be_bytes());
    write_projected_grid_core(&mut section, &grid.core, 55, 64)?;
    section[47..51].copy_from_slice(&encode_grib_i32(
        grid.lat_d,
        "latitude where grid lengths are specified",
    )?);
    section[51..55].copy_from_slice(&encode_grib_i32(grid.lon_v, "projection longitude")?);
    section[63] = grid.projection_center_flag;
    section[65..69].copy_from_slice(&encode_grib_i32(grid.latin1, "first Latin latitude")?);
    section[69..73].copy_from_slice(&encode_grib_i32(grid.latin2, "second Latin latitude")?);
    section[73..77].copy_from_slice(&encode_grib_i32(
        grid.lat_southern_pole,
        "latitude of southern pole",
    )?);
    section[77..81].copy_from_slice(&encode_grib_i32(
        grid.lon_southern_pole,
        "longitude of southern pole",
    )?);
    out.extend_from_slice(&section);
    Ok(())
}

fn write_lambert_conformal_grid_section(
    out: &mut Vec<u8>,
    grid: &LambertConformalGrid,
) -> Result<()> {
    checked_projected_point_count(
        grid.core.nx,
        grid.core.ny,
        grid.core.number_of_points,
        "Lambert conformal grid",
    )?;

    let mut section = vec![0u8; 81];
    section[..4].copy_from_slice(&81u32.to_be_bytes());
    section[4] = 3;
    section[12..14].copy_from_slice(&30u16.to_be_bytes());
    write_projected_grid_core(&mut section, &grid.core, 55, 64)?;
    section[47..51].copy_from_slice(&encode_grib_i32(
        grid.lat_d,
        "latitude where grid lengths are specified",
    )?);
    section[51..55].copy_from_slice(&encode_grib_i32(grid.lon_v, "projection longitude")?);
    section[63] = grid.projection_center_flag;
    section[65..69].copy_from_slice(&encode_grib_i32(grid.latin1, "first Latin latitude")?);
    section[69..73].copy_from_slice(&encode_grib_i32(grid.latin2, "second Latin latitude")?);
    section[73..77].copy_from_slice(&encode_grib_i32(
        grid.lat_southern_pole,
        "latitude of southern pole",
    )?);
    section[77..81].copy_from_slice(&encode_grib_i32(
        grid.lon_southern_pole,
        "longitude of southern pole",
    )?);
    out.extend_from_slice(&section);
    Ok(())
}

struct ProjectedGridShapeOfEarth {
    shape_of_earth: u8,
    scale_factor_radius: u8,
    scaled_value_radius: u32,
    scale_factor_major_axis: u8,
    scaled_value_major_axis: u32,
    scale_factor_minor_axis: u8,
    scaled_value_minor_axis: u32,
}

fn write_projected_grid_shape_of_earth(section: &mut [u8], shape: ProjectedGridShapeOfEarth) {
    section[14] = shape.shape_of_earth;
    section[15] = shape.scale_factor_radius;
    section[16..20].copy_from_slice(&shape.scaled_value_radius.to_be_bytes());
    section[20] = shape.scale_factor_major_axis;
    section[21..25].copy_from_slice(&shape.scaled_value_major_axis.to_be_bytes());
    section[25] = shape.scale_factor_minor_axis;
    section[26..30].copy_from_slice(&shape.scaled_value_minor_axis.to_be_bytes());
}

fn write_projected_grid_core(
    section: &mut [u8],
    core: &ProjectedGridCore,
    spacing_offset: usize,
    scanning_mode_offset: usize,
) -> Result<()> {
    section[6..10].copy_from_slice(&core.number_of_points.to_be_bytes());
    write_projected_grid_shape_of_earth(
        section,
        ProjectedGridShapeOfEarth {
            shape_of_earth: core.shape_of_earth,
            scale_factor_radius: core.scale_factor_radius,
            scaled_value_radius: core.scaled_value_radius,
            scale_factor_major_axis: core.scale_factor_major_axis,
            scaled_value_major_axis: core.scaled_value_major_axis,
            scale_factor_minor_axis: core.scale_factor_minor_axis,
            scaled_value_minor_axis: core.scaled_value_minor_axis,
        },
    );
    section[30..34].copy_from_slice(&core.nx.to_be_bytes());
    section[34..38].copy_from_slice(&core.ny.to_be_bytes());
    section[38..42].copy_from_slice(&encode_grib_i32(
        core.lat_first,
        "latitude of first grid point",
    )?);
    section[42..46].copy_from_slice(&encode_grib_i32(
        core.lon_first,
        "longitude of first grid point",
    )?);
    section[46] = core.resolution_and_component_flags;
    section[spacing_offset..spacing_offset + 4].copy_from_slice(&core.dx.to_be_bytes());
    section[spacing_offset + 4..spacing_offset + 8].copy_from_slice(&core.dy.to_be_bytes());
    section[scanning_mode_offset] = core.scanning_mode;
    Ok(())
}

pub(crate) fn write_product_section(out: &mut Vec<u8>, product: &ProductDefinition) -> Result<()> {
    match &product.template {
        ProductDefinitionTemplate::AnalysisOrForecast(template) => {
            write_product_template_prefix(out, product, 0, 34, template)
        }
        ProductDefinitionTemplate::IndividualEnsembleForecast(template) => {
            write_product_template_prefix(out, product, 1, 37, &template.base)?;
            write_ensemble_product_extra(out, template)
        }
        ProductDefinitionTemplate::DerivedForecast(template) => {
            write_product_template_prefix(out, product, 2, 36, &template.base)?;
            write_derived_product_extra(out, template)
        }
        ProductDefinitionTemplate::ProbabilityForecast(template) => {
            validate_probability_type(template.probability)?;
            write_product_template_prefix(out, product, 5, 47, &template.base)?;
            write_probability_product_extra(out, template)
        }
        ProductDefinitionTemplate::PercentileForecast(template) => {
            validate_percentile(template.percentile_value)?;
            write_product_template_prefix(out, product, 6, 35, &template.base)?;
            write_u8_be(out, template.percentile_value)
        }
        ProductDefinitionTemplate::StatisticalProcess(template) => {
            let range_count = checked_time_range_count(template.interval.time_ranges.len())?;
            let section_length = statistical_product_section_len(46, range_count)?;
            write_product_template_prefix(out, product, 8, section_length, &template.base)?;
            write_statistical_interval(out, &template.interval, range_count)
        }
        ProductDefinitionTemplate::ProbabilityStatisticalProcess(template) => {
            validate_probability_type(template.probability.probability)?;
            let range_count = checked_time_range_count(template.interval.time_ranges.len())?;
            let section_length = statistical_product_section_len(59, range_count)?;
            write_product_template_prefix(
                out,
                product,
                9,
                section_length,
                &template.probability.base,
            )?;
            write_probability_product_extra(out, &template.probability)?;
            write_statistical_interval(out, &template.interval, range_count)
        }
        ProductDefinitionTemplate::PercentileStatisticalProcess(template) => {
            validate_percentile(template.percentile.percentile_value)?;
            let range_count = checked_time_range_count(template.interval.time_ranges.len())?;
            let section_length = statistical_product_section_len(47, range_count)?;
            write_product_template_prefix(
                out,
                product,
                10,
                section_length,
                &template.percentile.base,
            )?;
            write_u8_be(out, template.percentile.percentile_value)?;
            write_statistical_interval(out, &template.interval, range_count)
        }
        ProductDefinitionTemplate::EnsembleStatisticalProcess(template) => {
            let range_count = checked_time_range_count(template.interval.time_ranges.len())?;
            let section_length = statistical_product_section_len(49, range_count)?;
            write_product_template_prefix(
                out,
                product,
                11,
                section_length,
                &template.ensemble.base,
            )?;
            write_ensemble_product_extra(out, &template.ensemble)?;
            write_statistical_interval(out, &template.interval, range_count)
        }
        ProductDefinitionTemplate::DerivedStatisticalProcess(template) => {
            let range_count = checked_time_range_count(template.interval.time_ranges.len())?;
            let section_length = statistical_product_section_len(48, range_count)?;
            write_product_template_prefix(
                out,
                product,
                12,
                section_length,
                &template.derived.base,
            )?;
            write_derived_product_extra(out, &template.derived)?;
            write_statistical_interval(out, &template.interval, range_count)
        }
        ProductDefinitionTemplate::SpatialProcess(template) => {
            write_product_template_prefix(out, product, 15, 37, &template.base)?;
            write_u8_be(out, template.statistical_process)?;
            write_u8_be(out, template.spatial_processing)?;
            write_u8_be(out, template.number_of_points_used)
        }
        ProductDefinitionTemplate::Unsupported { number, .. } => {
            Err(Error::UnsupportedProductTemplate(*number))
        }
        template => Err(Error::UnsupportedProductTemplate(template.number())),
    }
}

fn write_product_template_prefix(
    out: &mut Vec<u8>,
    product: &ProductDefinition,
    template_number: u16,
    section_length: u32,
    template: &AnalysisOrForecastTemplate,
) -> Result<()> {
    validate_product_template_prefix(template)?;

    write_u32_be(out, section_length)?;
    write_u8_be(out, 4)?;
    write_u16_be(out, 0)?;
    write_u16_be(out, template_number)?;
    write_u8_be(out, product.parameter_category)?;
    write_u8_be(out, product.parameter_number)?;
    write_u8_be(out, template.type_of_generating_process)?;
    write_u8_be(out, template.background_generating_process_identifier)?;
    write_u8_be(out, template.generating_process_identifier)?;
    write_u16_be(out, template.hours_after_data_cutoff.unwrap_or(u16::MAX))?;
    write_u8_be(out, template.minutes_after_data_cutoff.unwrap_or(u8::MAX))?;
    write_u8_be(out, template.forecast_time_unit)?;
    out.extend_from_slice(&encode_wmo_i32(template.forecast_time).ok_or_else(|| {
        Error::ValueOutOfRange("forecast time does not fit GRIB signed i32".into())
    })?);
    write_surface(out, template.first_surface.as_ref())?;
    write_surface(out, template.second_surface.as_ref())
}

fn validate_product_template_prefix(template: &AnalysisOrForecastTemplate) -> Result<()> {
    if template.hours_after_data_cutoff == Some(u16::MAX) {
        return Err(Error::ValueOutOfRange(
            "hours after data cutoff must be at most 65534".into(),
        ));
    }
    if template
        .minutes_after_data_cutoff
        .is_some_and(|minutes| minutes > 59)
    {
        return Err(Error::ValueOutOfRange(
            "minutes after data cutoff must be at most 59".into(),
        ));
    }
    if encode_wmo_i32(template.forecast_time).is_none() {
        return Err(Error::ValueOutOfRange(
            "forecast time does not fit GRIB signed i32".into(),
        ));
    }
    Ok(())
}

fn write_ensemble_product_extra(
    out: &mut Vec<u8>,
    template: &grib_core::IndividualEnsembleForecastTemplate,
) -> Result<()> {
    write_u8_be(out, template.type_of_ensemble_forecast)?;
    write_u8_be(out, template.perturbation_number)?;
    write_u8_be(out, template.number_of_forecasts_in_ensemble)
}

fn write_derived_product_extra(
    out: &mut Vec<u8>,
    template: &grib_core::DerivedForecastTemplate,
) -> Result<()> {
    write_u8_be(out, template.derived_forecast_type)?;
    write_u8_be(out, template.number_of_forecasts_in_ensemble)
}

fn write_probability_product_extra(
    out: &mut Vec<u8>,
    template: &grib_core::ProbabilityForecastTemplate,
) -> Result<()> {
    write_u8_be(out, template.forecast_probability_number)?;
    write_u8_be(out, template.total_number_of_forecast_probabilities)?;
    write_u8_be(out, template.probability.code())?;
    write_probability_limit(out, template.probability.lower_limit())?;
    write_probability_limit(out, template.probability.upper_limit())
}

fn write_probability_limit(out: &mut Vec<u8>, limit: Option<ProbabilityLimit>) -> Result<()> {
    let Some(limit) = limit else {
        out.extend_from_slice(&[0xff; 5]);
        return Ok(());
    };

    write_u8_be(
        out,
        encode_wmo_i8(limit.scale_factor).ok_or_else(|| {
            Error::ValueOutOfRange(
                "probability-limit scale factor does not fit GRIB signed i8".into(),
            )
        })?,
    )?;
    out.extend_from_slice(&encode_wmo_i32(limit.scaled_value).ok_or_else(|| {
        Error::ValueOutOfRange("probability-limit scaled value does not fit GRIB signed i32".into())
    })?);
    Ok(())
}

fn validate_probability_type(probability: ProbabilityType) -> Result<()> {
    if let ProbabilityType::Other { code, .. } = probability {
        if matches!(code, 0..=10 | 255) {
            return Err(Error::ValueOutOfRange(format!(
                "WMO probability type {code} must use its typed probability variant"
            )));
        }
    }

    for limit in [probability.lower_limit(), probability.upper_limit()]
        .into_iter()
        .flatten()
    {
        if encode_wmo_i8(limit.scale_factor).is_none() {
            return Err(Error::ValueOutOfRange(
                "probability-limit scale factor does not fit GRIB signed i8".into(),
            ));
        }
        if encode_wmo_i32(limit.scaled_value).is_none() {
            return Err(Error::ValueOutOfRange(
                "probability-limit scaled value does not fit GRIB signed i32".into(),
            ));
        }
    }
    Ok(())
}

fn validate_percentile(percentile: u8) -> Result<()> {
    if percentile <= 100 {
        Ok(())
    } else {
        Err(Error::ValueOutOfRange(format!(
            "percentile value {percentile} exceeds 100"
        )))
    }
}

fn write_statistical_interval(
    out: &mut Vec<u8>,
    interval: &StatisticalInterval,
    range_count: u8,
) -> Result<()> {
    write_reference_time(out, interval.end_of_overall_time_interval)?;
    write_u8_be(out, range_count)?;
    write_u32_be(out, interval.number_of_missing_in_statistical_process)?;
    write_statistical_time_ranges(out, &interval.time_ranges)
}

fn write_reference_time(out: &mut Vec<u8>, reference_time: ReferenceTime) -> Result<()> {
    validate_reference_time(reference_time)?;

    write_u16_be(out, reference_time.year)?;
    write_u8_be(out, reference_time.month)?;
    write_u8_be(out, reference_time.day)?;
    write_u8_be(out, reference_time.hour)?;
    write_u8_be(out, reference_time.minute)?;
    write_u8_be(out, reference_time.second)
}

fn validate_reference_time(reference_time: ReferenceTime) -> Result<()> {
    if reference_time.is_valid() {
        return Ok(());
    }

    Err(Error::InvalidSection {
        section: 4,
        reason: format!(
            "invalid reference timestamp {:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            reference_time.year,
            reference_time.month,
            reference_time.day,
            reference_time.hour,
            reference_time.minute,
            reference_time.second
        ),
    })
}

fn checked_time_range_count(range_count: usize) -> Result<u8> {
    u8::try_from(range_count).map_err(|_| {
        Error::ValueOutOfRange(format!(
            "statistical product time-range count ({range_count}) exceeds GRIB2 u8 limit"
        ))
    })
}

fn statistical_product_section_len(base_len: u32, range_count: u8) -> Result<u32> {
    u32::from(range_count)
        .checked_mul(12)
        .and_then(|ranges_len| base_len.checked_add(ranges_len))
        .ok_or(Error::ArithmeticOverflow {
            operation: "computing statistical product section length",
        })
}

fn write_statistical_time_ranges(out: &mut Vec<u8>, ranges: &[StatisticalTimeRange]) -> Result<()> {
    for range in ranges {
        write_u8_be(out, range.type_of_statistical_processing)?;
        write_u8_be(out, range.type_of_time_increment)?;
        write_u8_be(out, range.time_range_unit)?;
        write_u32_be(out, range.time_range_length)?;
        write_u8_be(out, range.time_increment_unit)?;
        write_u32_be(out, range.time_increment)?;
    }
    Ok(())
}

fn write_surface(out: &mut Vec<u8>, surface: Option<&FixedSurface>) -> Result<()> {
    match surface {
        Some(surface) => {
            write_u8_be(out, surface.surface_type)?;
            match surface.value {
                Some(value) => {
                    write_u8_be(
                        out,
                        encode_grib_i8(value.scale_factor, "fixed-surface scale factor")?,
                    )?;
                    out.extend_from_slice(&encode_grib_i32(
                        value.scaled_value,
                        "fixed-surface scaled value",
                    )?);
                }
                None => out.extend_from_slice(&[0xff; 5]),
            }
            Ok(())
        }
        None => {
            write_u8_be(out, 255)?;
            out.extend_from_slice(&[0xff; 5]);
            Ok(())
        }
    }
}

pub(crate) fn write_data_representation_section(
    out: &mut Vec<u8>,
    packed: &PackedField,
) -> Result<()> {
    match &packed.representation {
        DataRepresentation::SimplePacking(params) => {
            write_simple_data_representation_section(out, params)
        }
        DataRepresentation::ComplexPacking(params) => {
            write_complex_data_representation_section(out, params)
        }
        DataRepresentation::IeeePacking(params) => {
            write_ieee_data_representation_section(out, params)
        }
        DataRepresentation::Jpeg2000Packing(params) => {
            write_jpeg2000_data_representation_section(out, params)
        }
        DataRepresentation::PngPacking(params) => {
            write_png_data_representation_section(out, params)
        }
        DataRepresentation::CcsdsPacking(params) => {
            write_ccsds_data_representation_section(out, params)
        }
        representation => Err(Error::UnsupportedDataTemplate(
            representation.template_number(),
        )),
    }
}

fn write_simple_data_representation_section(
    out: &mut Vec<u8>,
    params: &ScaledPackingParams,
) -> Result<()> {
    let encoded_values = checked_u32(params.encoded_values, "encoded value count")?;
    write_u32_be(out, 21)?;
    write_u8_be(out, 5)?;
    write_u32_be(out, encoded_values)?;
    write_u16_be(out, 0)?;
    out.extend_from_slice(&params.reference_value.to_be_bytes());
    out.extend_from_slice(&encode_grib_i16(params.binary_scale, "binary scale")?);
    out.extend_from_slice(&encode_grib_i16(params.decimal_scale, "decimal scale")?);
    write_u8_be(out, params.bits_per_value)?;
    write_u8_be(out, params.original_field_type)
}

fn write_complex_data_representation_section(
    out: &mut Vec<u8>,
    params: &ComplexPackingParams,
) -> Result<()> {
    let encoded_values = checked_u32(params.encoded_values, "encoded value count")?;
    let num_groups = checked_u32(params.num_groups, "complex group count")?;
    let template = if params.spatial_differencing.is_some() {
        3
    } else {
        2
    };
    let section_length = if params.spatial_differencing.is_some() {
        49
    } else {
        47
    };

    write_u32_be(out, section_length)?;
    write_u8_be(out, 5)?;
    write_u32_be(out, encoded_values)?;
    write_u16_be(out, template)?;
    out.extend_from_slice(&params.reference_value.to_be_bytes());
    out.extend_from_slice(&encode_grib_i16(params.binary_scale, "binary scale")?);
    out.extend_from_slice(&encode_grib_i16(params.decimal_scale, "decimal scale")?);
    write_u8_be(out, params.group_reference_bits)?;
    write_u8_be(out, params.original_field_type)?;
    write_u8_be(out, params.group_splitting_method)?;
    write_u8_be(out, params.missing_value_management)?;
    write_u32_be(out, params.primary_missing_substitute)?;
    write_u32_be(out, params.secondary_missing_substitute)?;
    write_u32_be(out, num_groups)?;
    write_u8_be(out, params.group_width_reference)?;
    write_u8_be(out, params.group_width_bits)?;
    write_u32_be(out, params.group_length_reference)?;
    write_u8_be(out, params.group_length_increment)?;
    write_u32_be(out, params.true_length_last_group)?;
    write_u8_be(out, params.scaled_group_length_bits)?;
    if let Some(spatial) = params.spatial_differencing {
        write_u8_be(out, spatial.order)?;
        write_u8_be(out, spatial.descriptor_octets)?;
    }
    Ok(())
}

fn write_ieee_data_representation_section(
    out: &mut Vec<u8>,
    params: &IeeePackingParams,
) -> Result<()> {
    let encoded_values = checked_u32(params.encoded_values, "encoded value count")?;
    write_u32_be(out, 12)?;
    write_u8_be(out, 5)?;
    write_u32_be(out, encoded_values)?;
    write_u16_be(out, 4)?;
    write_u8_be(out, params.precision.code())
}

fn write_jpeg2000_data_representation_section(
    out: &mut Vec<u8>,
    params: &Jpeg2000PackingParams,
) -> Result<()> {
    write_scaled_data_representation_base(out, 23, 40, &params.packing)?;
    write_u8_be(out, params.compression_type)?;
    write_u8_be(out, params.target_compression_ratio)
}

fn write_png_data_representation_section(
    out: &mut Vec<u8>,
    params: &PngPackingParams,
) -> Result<()> {
    write_scaled_data_representation_base(out, 21, 41, &params.packing)
}

fn write_ccsds_data_representation_section(
    out: &mut Vec<u8>,
    params: &CcsdsPackingParams,
) -> Result<()> {
    write_scaled_data_representation_base(out, 25, 42, params.packing())?;
    write_u8_be(out, params.flags().bits())?;
    write_u8_be(out, params.block_size().samples())?;
    write_u16_be(out, params.reference_sample_interval())
}

fn write_scaled_data_representation_base(
    out: &mut Vec<u8>,
    section_length: u32,
    template: u16,
    params: &ScaledPackingParams,
) -> Result<()> {
    let encoded_values = checked_u32(params.encoded_values, "encoded value count")?;
    write_u32_be(out, section_length)?;
    write_u8_be(out, 5)?;
    write_u32_be(out, encoded_values)?;
    write_u16_be(out, template)?;
    out.extend_from_slice(&params.reference_value.to_be_bytes());
    out.extend_from_slice(&encode_grib_i16(params.binary_scale, "binary scale")?);
    out.extend_from_slice(&encode_grib_i16(params.decimal_scale, "decimal scale")?);
    write_u8_be(out, params.bits_per_value)?;
    write_u8_be(out, params.original_field_type)
}

/// Section 6 is mandatory in every GRIB2 field; indicator 255 states that no
/// bitmap applies.
pub(crate) fn write_bitmap_section(out: &mut Vec<u8>, bitmap_payload: Option<&[u8]>) -> Result<()> {
    let payload = bitmap_payload.unwrap_or_default();
    let length = checked_section_length(6usize + payload.len(), 6)?;
    write_u32_be(out, length)?;
    write_u8_be(out, 6)?;
    write_u8_be(out, if bitmap_payload.is_some() { 0 } else { 255 })?;
    out.extend_from_slice(payload);
    Ok(())
}

pub(crate) fn write_data_section(out: &mut Vec<u8>, data_payload: &[u8]) -> Result<()> {
    let length = checked_section_length(5usize + data_payload.len(), 7)?;
    write_u32_be(out, length)?;
    write_u8_be(out, 7)?;
    out.extend_from_slice(data_payload);
    Ok(())
}

fn checked_section_length(length: usize, section: u8) -> Result<u32> {
    u32::try_from(length).map_err(|_| Error::InvalidSection {
        section,
        reason: format!("section length {length} exceeds u32"),
    })
}

pub(crate) fn checked_grid_point_count(grid: &GridDefinition) -> Result<usize> {
    match grid {
        GridDefinition::LatLon(grid) => Ok(checked_latlon_point_count(grid)? as usize),
        GridDefinition::Mercator(grid) => checked_projected_point_count(
            grid.core.nx,
            grid.core.ny,
            grid.core.number_of_points,
            "Mercator grid",
        ),
        GridDefinition::PolarStereographic(grid) => checked_projected_point_count(
            grid.core.nx,
            grid.core.ny,
            grid.core.number_of_points,
            "polar stereographic grid",
        ),
        GridDefinition::LambertConformal(grid) => checked_projected_point_count(
            grid.core.nx,
            grid.core.ny,
            grid.core.number_of_points,
            "Lambert conformal grid",
        ),
        GridDefinition::AlbersEqualArea(grid) => checked_projected_point_count(
            grid.core.nx,
            grid.core.ny,
            grid.core.number_of_points,
            "Albers equal-area grid",
        ),
        _ => Err(Error::UnsupportedGridTemplate(grid.template_number())),
    }
}

fn checked_latlon_point_count(grid: &LatLonGrid) -> Result<u32> {
    let count =
        u64::from(grid.ni)
            .checked_mul(u64::from(grid.nj))
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing latitude-longitude grid point count",
            })?;
    u32::try_from(count)
        .map_err(|_| Error::ValueOutOfRange(format!("grid point count {count} exceeds u32")))
}

fn checked_projected_point_count(
    nx: u32,
    ny: u32,
    number_of_points: u32,
    grid_name: &str,
) -> Result<usize> {
    let expected = u64::from(nx)
        .checked_mul(u64::from(ny))
        .ok_or(Error::ArithmeticOverflow {
            operation: "computing projected grid point count",
        })?;
    let expected = u32::try_from(expected).map_err(|_| {
        Error::ValueOutOfRange(format!("{grid_name} point count {expected} exceeds u32"))
    })?;
    if number_of_points != expected {
        let expected = usize::try_from(expected).map_err(|_| {
            Error::ValueOutOfRange(format!("{grid_name} point count {expected} exceeds usize"))
        })?;
        let actual = usize::try_from(number_of_points).map_err(|_| {
            Error::ValueOutOfRange(format!(
                "{grid_name} number_of_points {number_of_points} exceeds usize"
            ))
        })?;
        return Err(Error::DataLengthMismatch { expected, actual });
    }
    usize::try_from(number_of_points).map_err(|_| {
        Error::ValueOutOfRange(format!(
            "{grid_name} point count {number_of_points} exceeds usize"
        ))
    })
}

pub(crate) fn validate_supported_grid(grid: &GridDefinition) -> Result<()> {
    grid.validate_supported_scan_order()
}

pub(crate) fn validate_supported_product(product: &ProductDefinition) -> Result<()> {
    match &product.template {
        ProductDefinitionTemplate::AnalysisOrForecast(template) => {
            validate_product_template_prefix(template)
        }
        ProductDefinitionTemplate::IndividualEnsembleForecast(template) => {
            validate_product_template_prefix(&template.base)
        }
        ProductDefinitionTemplate::DerivedForecast(template) => {
            validate_product_template_prefix(&template.base)
        }
        ProductDefinitionTemplate::ProbabilityForecast(template) => {
            validate_product_template_prefix(&template.base)?;
            validate_probability_type(template.probability)
        }
        ProductDefinitionTemplate::PercentileForecast(template) => {
            validate_product_template_prefix(&template.base)?;
            validate_percentile(template.percentile_value)
        }
        ProductDefinitionTemplate::StatisticalProcess(template) => {
            validate_product_template_prefix(&template.base)?;
            validate_statistical_interval(&template.interval)
        }
        ProductDefinitionTemplate::ProbabilityStatisticalProcess(template) => {
            validate_product_template_prefix(&template.probability.base)?;
            validate_probability_type(template.probability.probability)?;
            validate_statistical_interval(&template.interval)
        }
        ProductDefinitionTemplate::PercentileStatisticalProcess(template) => {
            validate_product_template_prefix(&template.percentile.base)?;
            validate_percentile(template.percentile.percentile_value)?;
            validate_statistical_interval(&template.interval)
        }
        ProductDefinitionTemplate::EnsembleStatisticalProcess(template) => {
            validate_product_template_prefix(&template.ensemble.base)?;
            validate_statistical_interval(&template.interval)
        }
        ProductDefinitionTemplate::DerivedStatisticalProcess(template) => {
            validate_product_template_prefix(&template.derived.base)?;
            validate_statistical_interval(&template.interval)
        }
        ProductDefinitionTemplate::SpatialProcess(template) => {
            validate_product_template_prefix(&template.base)
        }
        ProductDefinitionTemplate::Unsupported { number, .. } => {
            Err(Error::UnsupportedProductTemplate(*number))
        }
        template => Err(Error::UnsupportedProductTemplate(template.number())),
    }
}

fn validate_statistical_interval(interval: &StatisticalInterval) -> Result<()> {
    checked_time_range_count(interval.time_ranges.len())?;
    validate_reference_time(interval.end_of_overall_time_interval)
}
