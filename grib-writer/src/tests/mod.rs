use super::{
    Grib1FieldBuilder, Grib1ProductDefinition, Grib2FieldBuilder, GribWriter, PackingStrategy,
    SpatialDifferencingOrder, ValueOrder,
};
use std::process::Command;

use grib_core::binary::decode_ibm_f32;
use grib_core::metadata::ReferenceTime;
#[cfg(feature = "jpeg2000")]
use grib_core::Error;
use grib_core::{
    AlbersEqualAreaGrid, AnalysisOrForecastTemplate, CcsdsBlockSize, CcsdsFlags,
    DataRepresentation, DerivedForecastTemplate, DerivedStatisticalProcessTemplate,
    EnsembleStatisticalProcessTemplate, FixedSurface, GridDefinition, Identification,
    IndividualEnsembleForecastTemplate, LambertConformalGrid, LatLonGrid, MercatorGrid,
    PercentileForecastTemplate, PercentileStatisticalProcessTemplate, PolarStereographicGrid,
    ProbabilityForecastTemplate, ProbabilityLimit, ProbabilityStatisticalProcessTemplate,
    ProbabilityType, ProductDefinition, ProductDefinitionTemplate, ProjectedGridCore,
    SpatialProcessTemplate, StatisticalInterval, StatisticalProcessTemplate, StatisticalTimeRange,
};
use grib_reader::sections::scan_sections;
use grib_reader::{GribFile, PredefinedBitmap};
use serde::Deserialize;

fn identification() -> Identification {
    Identification {
        center_id: 7,
        subcenter_id: 0,
        master_table_version: 35,
        local_table_version: 1,
        significance_of_reference_time: 1,
        reference_year: 2026,
        reference_month: 3,
        reference_day: 20,
        reference_hour: 12,
        reference_minute: 0,
        reference_second: 0,
        production_status: 0,
        processed_data_type: 1,
    }
}

fn grib1_product() -> Grib1ProductDefinition {
    Grib1ProductDefinition {
        table_version: 2,
        center_id: 7,
        generating_process_id: 255,
        grid_id: 0,
        has_grid_definition: true,
        has_bitmap: false,
        parameter_number: 11,
        level_type: 100,
        level_value: 850,
        reference_time: ReferenceTime {
            year: 2026,
            month: 3,
            day: 20,
            hour: 12,
            minute: 0,
            second: 0,
        },
        forecast_time_unit: 1,
        p1: 6,
        p2: 0,
        time_range_indicator: 0,
        average_count: 0,
        missing_count: 0,
        century: 21,
        subcenter_id: 0,
        decimal_scale: 0,
    }
}

fn grid() -> GridDefinition {
    grid_with_shape_and_scanning_mode(2, 2, 0)
}

fn grid_with_scanning_mode(scanning_mode: u8) -> GridDefinition {
    grid_with_shape_and_scanning_mode(3, 2, scanning_mode)
}

fn grid_with_shape_and_scanning_mode(ni: u32, nj: u32, scanning_mode: u8) -> GridDefinition {
    let lon_first = -120_000_000;
    let lat_first = 50_000_000;
    let di = 1_000_000;
    let dj = 1_000_000;
    let i_step = if scanning_mode & 0b1000_0000 == 0 {
        di as i32
    } else {
        -(di as i32)
    };
    let j_step = if scanning_mode & 0b0100_0000 != 0 {
        dj as i32
    } else {
        -(dj as i32)
    };

    GridDefinition::LatLon(LatLonGrid {
        ni,
        nj,
        lat_first,
        lon_first,
        lat_last: lat_first + (nj.saturating_sub(1) as i32) * j_step,
        lon_last: lon_first + (ni.saturating_sub(1) as i32) * i_step,
        di,
        dj,
        scanning_mode,
    })
}

fn projected_core(
    nx: u32,
    ny: u32,
    lat_first: i32,
    lon_first: i32,
    dx: u32,
    dy: u32,
    scanning_mode: u8,
) -> ProjectedGridCore {
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
        lat_first,
        lon_first,
        resolution_and_component_flags: 0x08,
        dx,
        dy,
        scanning_mode,
    }
}

fn polar_grid(scanning_mode: u8) -> GridDefinition {
    GridDefinition::PolarStereographic(PolarStereographicGrid {
        core: projected_core(
            3,
            2,
            41_612_949,
            185_117_126,
            3_000_000,
            3_000_000,
            scanning_mode,
        ),
        lat_d: 60_000_000,
        lon_v: 225_000_000,
        projection_center_flag: 0,
    })
}

fn mercator_grid(scanning_mode: u8) -> GridDefinition {
    GridDefinition::Mercator(MercatorGrid {
        core: projected_core(
            3,
            2,
            -20_000_000,
            -100_000_000,
            1_000_000,
            2_000_000,
            scanning_mode,
        ),
        lat_d: 0,
        lat_last: 20_000_000,
        lon_last: -98_000_000,
        orientation_of_grid: 0,
    })
}

fn lambert_grid(scanning_mode: u8) -> GridDefinition {
    GridDefinition::LambertConformal(LambertConformalGrid {
        core: ProjectedGridCore {
            shape_of_earth: 1,
            scaled_value_radius: 6_371_200,
            ..projected_core(
                3,
                2,
                12_190_000,
                226_541_000,
                2_539_703,
                2_539_703,
                scanning_mode,
            )
        },
        lat_d: 25_000_000,
        lon_v: 265_000_000,
        projection_center_flag: 0,
        latin1: 25_000_000,
        latin2: 25_000_000,
        lat_southern_pole: -90_000_000,
        lon_southern_pole: 0,
    })
}

fn albers_grid(scanning_mode: u8) -> GridDefinition {
    GridDefinition::AlbersEqualArea(AlbersEqualAreaGrid {
        core: projected_core(
            3,
            2,
            23_000_000,
            240_000_000,
            4_000_000,
            5_000_000,
            scanning_mode,
        ),
        lat_d: 25_000_000,
        lon_v: 265_000_000,
        projection_center_flag: 0,
        latin1: 29_500_000,
        latin2: 45_500_000,
        lat_southern_pole: -90_000_000,
        lon_southern_pole: 0,
    })
}

fn product(parameter_category: u8, parameter_number: u8) -> ProductDefinition {
    ProductDefinition {
        parameter_category,
        parameter_number,
        template: ProductDefinitionTemplate::AnalysisOrForecast(analysis_or_forecast_template()),
    }
}

fn analysis_or_forecast_template() -> AnalysisOrForecastTemplate {
    AnalysisOrForecastTemplate {
        type_of_generating_process: 2,
        background_generating_process_identifier: 0,
        generating_process_identifier: 0,
        hours_after_data_cutoff: Some(0),
        minutes_after_data_cutoff: Some(0),
        forecast_time_unit: 1,
        forecast_time: 6,
        first_surface: Some(FixedSurface::with_value(103, 0, 850)),
        second_surface: None,
    }
}

fn statistical_time_range() -> StatisticalTimeRange {
    StatisticalTimeRange {
        type_of_statistical_processing: 1,
        type_of_time_increment: 2,
        time_range_unit: 1,
        time_range_length: 6,
        time_increment_unit: 255,
        time_increment: 0,
    }
}

fn interval_end_time() -> ReferenceTime {
    ReferenceTime {
        year: 2026,
        month: 3,
        day: 20,
        hour: 18,
        minute: 0,
        second: 0,
    }
}

fn statistical_interval() -> StatisticalInterval {
    StatisticalInterval {
        end_of_overall_time_interval: interval_end_time(),
        number_of_missing_in_statistical_process: 0,
        time_ranges: vec![statistical_time_range()],
    }
}

fn write_message(fields: impl IntoIterator<Item = super::Grib2Field>) -> Vec<u8> {
    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes)
        .write_grib2_message(fields)
        .unwrap();
    bytes
}

fn write_grib1_message(field: super::Grib1Field) -> Vec<u8> {
    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes)
        .write_grib1_message(field)
        .unwrap();
    bytes
}

fn section_numbers(bytes: &[u8]) -> Vec<u8> {
    scan_sections(bytes)
        .unwrap()
        .iter()
        .map(|section| section.number)
        .collect()
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn section_payload(bytes: &[u8], section_number: u8) -> &[u8] {
    let section = scan_sections(bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == section_number)
        .unwrap();
    &bytes[section.offset + 5..section.offset + section.length]
}

fn simple_field(values: &[f64], parameter_category: u8, parameter_number: u8) -> super::Grib2Field {
    Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(parameter_category, parameter_number))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(values)
        .build()
        .unwrap()
}

fn field_with_product_template(template: ProductDefinitionTemplate) -> super::Grib2Field {
    Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(ProductDefinition {
            parameter_category: 0,
            parameter_number: 0,
            template,
        })
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap()
}

fn grib1_simple_field(values: &[f64]) -> super::Grib1Field {
    Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(grid())
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(values)
        .build()
        .unwrap()
}

#[test]
fn reports_missing_builder_fields_with_typed_errors() {
    assert!(matches!(
        Grib1FieldBuilder::new().build().unwrap_err(),
        grib_core::Error::MissingRequiredField {
            field: "GRIB1 product definition",
        }
    ));
    assert!(matches!(
        Grib2FieldBuilder::new().build().unwrap_err(),
        grib_core::Error::MissingRequiredField {
            field: "GRIB2 identification",
        }
    ));
}

#[test]
fn writes_simple_grib1_field_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0];
    let bytes = write_grib1_message(grib1_simple_field(&values));

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(file.message_count(), 1);
    assert_eq!(message.edition(), 1);
    assert_eq!(message.parameter_name(), "TMP");
    assert_eq!(message.grid_shape(), (2, 2));
    assert_eq!(message.forecast_time(), Some(6));
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_grib1_bitmap_from_nan_values() {
    let values = [5.0, f64::NAN, 7.0, 8.0];
    let bytes = write_grib1_message(grib1_simple_field(&values));
    let bitmap_offset = 8 + 28 + 32;
    assert_eq!(&bytes[bitmap_offset + 4..bitmap_offset + 6], &[0, 0]);

    let file = GribFile::from_bytes(bytes).unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 5.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 7.0);
    assert_eq!(decoded[3], 8.0);
}

#[test]
fn writes_grib1_bitmap_from_explicit_mask() {
    let field = Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(grid())
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[5.0, 999.0, 7.0, 8.0])
        .bitmap(&[true, false, true, true])
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_grib1_message(field)).unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 5.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 7.0);
    assert_eq!(decoded[3], 8.0);
}

#[test]
fn writes_grib1_predefined_bitmap_reference() {
    let bitmap = [true, false, true, false];
    let field = Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(grid())
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[9.0, 999.0, 7.0, 999.0])
        .predefined_bitmap(300, &bitmap)
        .build()
        .unwrap();
    let bytes = write_grib1_message(field);

    let bitmap_offset = 8 + 28 + 32;
    assert_eq!(
        &bytes[bitmap_offset..bitmap_offset + 6],
        &[0, 0, 6, 0, 1, 44]
    );

    let err = match GribFile::from_bytes(bytes.clone()) {
        Ok(_) => panic!("expected unsupported predefined bitmap"),
        Err(err) => err,
    };
    assert!(matches!(
        err,
        grib_core::Error::UnsupportedBitmapIndicator(300)
    ));

    let bitmap_payload = [0b1010_0000];
    let predefined = [PredefinedBitmap {
        center_id: 7,
        subcenter_id: Some(0),
        table_reference: 300,
        bitmap: &bitmap_payload,
    }];
    let file = GribFile::builder()
        .grib1_predefined_bitmaps(&predefined)
        .from_bytes(bytes)
        .unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 9.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 7.0);
    assert!(decoded[3].is_nan());
}

#[test]
fn rejects_zero_grib1_predefined_bitmap_reference() {
    let err = Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(grid())
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 999.0, 3.0, 999.0])
        .predefined_bitmap(0, &[true, false, true, false])
        .build()
        .unwrap_err();

    assert!(
        matches!(err, grib_core::Error::ValueOutOfRange(message) if message.contains("must be nonzero"))
    );
}

#[test]
fn writes_grib1_ibm_float_reference_value() {
    let bytes = write_grib1_message(grib1_simple_field(&[10.0, 11.0, 12.0, 13.0]));
    let bds_offset = 8 + 28 + 32;
    let reference = decode_ibm_f32(bytes[bds_offset + 6..bds_offset + 10].try_into().unwrap());
    assert_eq!(reference, 10.0);

    let file = GribFile::from_bytes(bytes).unwrap();
    assert_eq!(
        file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
        vec![10.0, 11.0, 12.0, 13.0]
    );
}

#[test]
fn rejects_grib1_u24_length_overflow() {
    let err = super::grib1::checked_grib1_u24_length(grib_core::binary::U24_MAX as usize + 1, 0)
        .unwrap_err();
    assert!(matches!(
        err,
        grib_core::Error::InvalidSection { section: 0, .. }
    ));
}

#[test]
fn rejects_unsupported_grib1_binary_data_flags() {
    let err = super::grib1::validate_grib1_binary_data_flags(0b0001).unwrap_err();
    assert!(matches!(
        err,
        grib_core::Error::UnsupportedDataTemplate(1007)
    ));
}

#[test]
fn rejects_grib1_grid_dimensions_beyond_u16() {
    let err = Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(GridDefinition::LatLon(LatLonGrid {
            ni: 65_536,
            nj: 1,
            lat_first: 0,
            lon_first: 0,
            lat_last: 0,
            lon_last: 0,
            di: 1_000,
            dj: 1_000,
            scanning_mode: 0,
        }))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0])
        .build()
        .unwrap_err();
    assert!(
        matches!(err, grib_core::Error::ValueOutOfRange(message) if message.contains("Ni exceeds"))
    );
}

#[test]
fn writes_simple_grib2_field_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0];
    let field = simple_field(&values, 0, 0);

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.parameter_name(), "TMP");
    assert_eq!(message.grid_shape(), (2, 2));
    assert_eq!(message.forecast_time(), Some(6));
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_signed_forecast_offset_and_process_metadata() {
    let mut template = analysis_or_forecast_template();
    template.background_generating_process_identifier = 7;
    template.generating_process_identifier = 42;
    template.hours_after_data_cutoff = Some(12);
    template.minutes_after_data_cutoff = Some(30);
    template.forecast_time = -6;
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(ProductDefinition {
            parameter_category: 0,
            parameter_number: 0,
            template: ProductDefinitionTemplate::AnalysisOrForecast(template),
        })
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.forecast_time(), Some(-6));
    let product = message.product_definition().unwrap();
    assert_eq!(product.generating_process_identifier(), Some(42));
    let ProductDefinitionTemplate::AnalysisOrForecast(template) = &product.template else {
        panic!("expected template 4.0");
    };
    assert_eq!(template.background_generating_process_identifier, 7);
    assert_eq!(template.hours_after_data_cutoff, Some(12));
    assert_eq!(template.minutes_after_data_cutoff, Some(30));
}

#[test]
fn rejects_unrepresentable_cutoff_metadata() {
    let mut template = analysis_or_forecast_template();
    template.hours_after_data_cutoff = Some(u16::MAX);
    let mut product = product(0, 0);
    product.template = ProductDefinitionTemplate::AnalysisOrForecast(template);
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product)
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();
    assert!(matches!(err, grib_core::Error::ValueOutOfRange(_)));
}

#[test]
fn writes_individual_ensemble_product_template_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(ProductDefinition {
            parameter_category: 0,
            parameter_number: 0,
            template: ProductDefinitionTemplate::IndividualEnsembleForecast(
                IndividualEnsembleForecastTemplate {
                    base: analysis_or_forecast_template(),
                    type_of_ensemble_forecast: 1,
                    perturbation_number: 2,
                    number_of_forecasts_in_ensemble: 20,
                },
            ),
        })
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 37);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    assert_eq!(product.template_number(), 1);
    match &product.template {
        ProductDefinitionTemplate::IndividualEnsembleForecast(template) => {
            assert_eq!(template.type_of_ensemble_forecast, 1);
            assert_eq!(template.perturbation_number, 2);
            assert_eq!(template.number_of_forecasts_in_ensemble, 20);
            assert_eq!(template.base.forecast_time, 6);
        }
        other => panic!("expected template 4.1, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_derived_forecast_product_template_readable_by_reader() {
    let field = field_with_product_template(ProductDefinitionTemplate::DerivedForecast(
        DerivedForecastTemplate {
            base: analysis_or_forecast_template(),
            derived_forecast_type: 4,
            number_of_forecasts_in_ensemble: 50,
        },
    ));

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 36);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    let ProductDefinitionTemplate::DerivedForecast(template) = &product.template else {
        panic!("expected template 4.2");
    };
    assert_eq!(template.derived_forecast_type, 4);
    assert_eq!(template.number_of_forecasts_in_ensemble, 50);
    assert_eq!(
        message.read_flat_data_as_f64().unwrap(),
        [1.0, 2.0, 3.0, 4.0]
    );
}

#[test]
fn writes_probability_forecast_product_template_readable_by_reader() {
    let probability = ProbabilityType::BetweenLimits {
        lower: ProbabilityLimit {
            scale_factor: 1,
            scaled_value: -125,
        },
        upper: ProbabilityLimit {
            scale_factor: 1,
            scaled_value: 250,
        },
    };
    let field = field_with_product_template(ProductDefinitionTemplate::ProbabilityForecast(
        ProbabilityForecastTemplate {
            base: analysis_or_forecast_template(),
            forecast_probability_number: 2,
            total_number_of_forecast_probabilities: 10,
            probability,
        },
    ));

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 47);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    let ProductDefinitionTemplate::ProbabilityForecast(template) = &product.template else {
        panic!("expected template 4.5");
    };
    assert_eq!(template.forecast_probability_number, 2);
    assert_eq!(template.total_number_of_forecast_probabilities, 10);
    assert_eq!(template.probability, probability);
    assert_eq!(
        message.read_flat_data_as_f64().unwrap(),
        [1.0, 2.0, 3.0, 4.0]
    );
}

#[test]
fn writes_percentile_forecast_product_template_readable_by_reader() {
    let field = field_with_product_template(ProductDefinitionTemplate::PercentileForecast(
        PercentileForecastTemplate {
            base: analysis_or_forecast_template(),
            percentile_value: 90,
        },
    ));

    let bytes = write_message([field]);
    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    let ProductDefinitionTemplate::PercentileForecast(template) = &product.template else {
        panic!("expected template 4.6");
    };
    assert_eq!(template.percentile_value, 90);
    assert_eq!(
        message.read_flat_data_as_f64().unwrap(),
        [1.0, 2.0, 3.0, 4.0]
    );
}

#[test]
fn writes_spatial_process_product_template_readable_by_reader() {
    let field = field_with_product_template(ProductDefinitionTemplate::SpatialProcess(
        SpatialProcessTemplate {
            base: analysis_or_forecast_template(),
            statistical_process: 1,
            spatial_processing: 2,
            number_of_points_used: 4,
        },
    ));

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 37);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    let ProductDefinitionTemplate::SpatialProcess(template) = &product.template else {
        panic!("expected template 4.15");
    };
    assert_eq!(template.statistical_process, 1);
    assert_eq!(template.spatial_processing, 2);
    assert_eq!(template.number_of_points_used, 4);
    assert_eq!(
        message.read_flat_data_as_f64().unwrap(),
        [1.0, 2.0, 3.0, 4.0]
    );
}

#[test]
fn rejects_noncanonical_probability_types_and_invalid_percentiles() {
    for template in [
        ProductDefinitionTemplate::ProbabilityForecast(ProbabilityForecastTemplate {
            base: analysis_or_forecast_template(),
            forecast_probability_number: 1,
            total_number_of_forecast_probabilities: 10,
            probability: ProbabilityType::Other {
                code: 0,
                lower: None,
                upper: None,
            },
        }),
        ProductDefinitionTemplate::ProbabilityForecast(ProbabilityForecastTemplate {
            base: analysis_or_forecast_template(),
            forecast_probability_number: 1,
            total_number_of_forecast_probabilities: 10,
            probability: ProbabilityType::BelowLowerLimit(ProbabilityLimit {
                scale_factor: 128,
                scaled_value: 10,
            }),
        }),
        ProductDefinitionTemplate::PercentileForecast(PercentileForecastTemplate {
            base: analysis_or_forecast_template(),
            percentile_value: 101,
        }),
    ] {
        let err = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(ProductDefinition {
                parameter_category: 0,
                parameter_number: 0,
                template,
            })
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[1.0, 2.0, 3.0, 4.0])
            .build()
            .unwrap_err();
        assert!(matches!(err, grib_core::Error::ValueOutOfRange(_)));
    }
}

#[test]
fn writes_statistical_product_template_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0];
    let mut base = analysis_or_forecast_template();
    base.forecast_time = 1;
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(ProductDefinition {
            parameter_category: 0,
            parameter_number: 1,
            template: ProductDefinitionTemplate::StatisticalProcess(StatisticalProcessTemplate {
                base,
                interval: statistical_interval(),
            }),
        })
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 58);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.valid_time(), Some(interval_end_time()));
    let product = message.product_definition().unwrap();
    assert_eq!(product.template_number(), 8);
    match &product.template {
        ProductDefinitionTemplate::StatisticalProcess(template) => {
            assert_eq!(template.base.forecast_time, 1);
            assert_eq!(template.interval, statistical_interval());
        }
        other => panic!("expected template 4.8, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_ensemble_statistical_product_template_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(ProductDefinition {
            parameter_category: 0,
            parameter_number: 2,
            template: ProductDefinitionTemplate::EnsembleStatisticalProcess(
                EnsembleStatisticalProcessTemplate {
                    ensemble: IndividualEnsembleForecastTemplate {
                        base: analysis_or_forecast_template(),
                        type_of_ensemble_forecast: 1,
                        perturbation_number: 3,
                        number_of_forecasts_in_ensemble: 30,
                    },
                    interval: statistical_interval(),
                },
            ),
        })
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 61);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.valid_time(), Some(interval_end_time()));
    let product = message.product_definition().unwrap();
    assert_eq!(product.template_number(), 11);
    match &product.template {
        ProductDefinitionTemplate::EnsembleStatisticalProcess(template) => {
            assert_eq!(template.ensemble.type_of_ensemble_forecast, 1);
            assert_eq!(template.ensemble.perturbation_number, 3);
            assert_eq!(template.ensemble.number_of_forecasts_in_ensemble, 30);
            assert_eq!(template.interval, statistical_interval());
        }
        other => panic!("expected template 4.11, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_probability_statistical_product_template_readable_by_reader() {
    let probability = ProbabilityType::AboveLowerLimit(ProbabilityLimit {
        scale_factor: 1,
        scaled_value: 125,
    });
    let field =
        field_with_product_template(ProductDefinitionTemplate::ProbabilityStatisticalProcess(
            ProbabilityStatisticalProcessTemplate {
                probability: ProbabilityForecastTemplate {
                    base: analysis_or_forecast_template(),
                    forecast_probability_number: 1,
                    total_number_of_forecast_probabilities: 10,
                    probability,
                },
                interval: statistical_interval(),
            },
        ));

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 71);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    let ProductDefinitionTemplate::ProbabilityStatisticalProcess(template) = &product.template
    else {
        panic!("expected template 4.9");
    };
    assert_eq!(template.probability.probability, probability);
    assert_eq!(template.interval, statistical_interval());
    assert_eq!(message.valid_time(), Some(interval_end_time()));
}

#[test]
fn writes_percentile_statistical_product_template_readable_by_reader() {
    let field =
        field_with_product_template(ProductDefinitionTemplate::PercentileStatisticalProcess(
            PercentileStatisticalProcessTemplate {
                percentile: PercentileForecastTemplate {
                    base: analysis_or_forecast_template(),
                    percentile_value: 75,
                },
                interval: statistical_interval(),
            },
        ));

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 59);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    let ProductDefinitionTemplate::PercentileStatisticalProcess(template) = &product.template
    else {
        panic!("expected template 4.10");
    };
    assert_eq!(template.percentile.percentile_value, 75);
    assert_eq!(template.interval, statistical_interval());
    assert_eq!(message.valid_time(), Some(interval_end_time()));
}

#[test]
fn writes_derived_statistical_product_template_readable_by_reader() {
    let field = field_with_product_template(ProductDefinitionTemplate::DerivedStatisticalProcess(
        DerivedStatisticalProcessTemplate {
            derived: DerivedForecastTemplate {
                base: analysis_or_forecast_template(),
                derived_forecast_type: 1,
                number_of_forecasts_in_ensemble: 20,
            },
            interval: statistical_interval(),
        },
    ));

    let bytes = write_message([field]);
    let product_section = scan_sections(&bytes)
        .unwrap()
        .into_iter()
        .find(|section| section.number == 4)
        .unwrap();
    assert_eq!(product_section.length, 60);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    let product = message.product_definition().unwrap();
    let ProductDefinitionTemplate::DerivedStatisticalProcess(template) = &product.template else {
        panic!("expected template 4.12");
    };
    assert_eq!(template.derived.derived_forecast_type, 1);
    assert_eq!(template.derived.number_of_forecasts_in_ensemble, 20);
    assert_eq!(template.interval, statistical_interval());
    assert_eq!(message.valid_time(), Some(interval_end_time()));
}

#[test]
fn rejects_too_many_statistical_time_ranges() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(ProductDefinition {
            parameter_category: 0,
            parameter_number: 1,
            template: ProductDefinitionTemplate::StatisticalProcess(StatisticalProcessTemplate {
                base: analysis_or_forecast_template(),
                interval: StatisticalInterval {
                    time_ranges: vec![statistical_time_range(); 256],
                    ..statistical_interval()
                },
            }),
        })
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();

    assert!(
        matches!(err, grib_core::Error::Other(message) if message.contains("time-range count"))
    );
}

#[test]
fn rejects_invalid_statistical_product_end_time() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(ProductDefinition {
            parameter_category: 0,
            parameter_number: 1,
            template: ProductDefinitionTemplate::StatisticalProcess(StatisticalProcessTemplate {
                base: analysis_or_forecast_template(),
                interval: StatisticalInterval {
                    end_of_overall_time_interval: ReferenceTime {
                        year: 2026,
                        month: 13,
                        day: 20,
                        hour: 18,
                        minute: 0,
                        second: 0,
                    },
                    ..statistical_interval()
                },
            }),
        })
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();

    assert!(
        matches!(err, grib_core::Error::InvalidSection { section: 4, reason } if reason.contains("invalid reference timestamp"))
    );
}

#[test]
fn writes_polar_stereographic_grib2_field_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(polar_grid(0))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.grid_shape(), (3, 2));
    assert_eq!(
        message.projected_x_coordinates().unwrap().unwrap(),
        vec![0.0, 3_000.0, 6_000.0]
    );
    assert_eq!(
        message.projected_y_coordinates().unwrap().unwrap(),
        vec![-0.0, -3_000.0]
    );
    match message.grid_definition() {
        GridDefinition::PolarStereographic(grid) => {
            assert_eq!(grid.core.number_of_points, 6);
            assert_eq!(grid.core.shape_of_earth, 6);
            assert_eq!(grid.core.nx, 3);
            assert_eq!(grid.core.ny, 2);
            assert_eq!(grid.core.lat_first, 41_612_949);
            assert_eq!(grid.core.lon_first, 185_117_126);
            assert_eq!(grid.lat_d, 60_000_000);
            assert_eq!(grid.lon_v, 225_000_000);
            assert_eq!(grid.core.dx, 3_000_000);
            assert_eq!(grid.core.dy, 3_000_000);
        }
        other => panic!("expected polar stereographic grid, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_mercator_grib2_field_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(mercator_grid(0))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.grid_shape(), (3, 2));
    assert_eq!(
        message.projected_x_coordinates().unwrap().unwrap(),
        vec![0.0, 1_000.0, 2_000.0]
    );
    assert_eq!(
        message.projected_y_coordinates().unwrap().unwrap(),
        vec![-0.0, -2_000.0]
    );
    match message.grid_definition() {
        GridDefinition::Mercator(grid) => {
            assert_eq!(grid.core.number_of_points, 6);
            assert_eq!(grid.core.shape_of_earth, 6);
            assert_eq!(grid.core.nx, 3);
            assert_eq!(grid.core.ny, 2);
            assert_eq!(grid.core.lat_first, -20_000_000);
            assert_eq!(grid.core.lon_first, -100_000_000);
            assert_eq!(grid.lat_d, 0);
            assert_eq!(grid.lat_last, 20_000_000);
            assert_eq!(grid.lon_last, -98_000_000);
            assert_eq!(grid.core.dx, 1_000_000);
            assert_eq!(grid.core.dy, 2_000_000);
        }
        other => panic!("expected Mercator grid, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_lambert_conformal_grib2_field_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(lambert_grid(0))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.grid_shape(), (3, 2));
    assert_eq!(
        message.projected_x_coordinates().unwrap().unwrap(),
        vec![0.0, 2_539.703, 5_079.406]
    );
    assert_eq!(
        message.projected_y_coordinates().unwrap().unwrap(),
        vec![-0.0, -2_539.703]
    );
    match message.grid_definition() {
        GridDefinition::LambertConformal(grid) => {
            assert_eq!(grid.core.number_of_points, 6);
            assert_eq!(grid.core.shape_of_earth, 1);
            assert_eq!(grid.core.scaled_value_radius, 6_371_200);
            assert_eq!(grid.core.nx, 3);
            assert_eq!(grid.core.ny, 2);
            assert_eq!(grid.core.lat_first, 12_190_000);
            assert_eq!(grid.core.lon_first, 226_541_000);
            assert_eq!(grid.lat_d, 25_000_000);
            assert_eq!(grid.lon_v, 265_000_000);
            assert_eq!(grid.core.dx, 2_539_703);
            assert_eq!(grid.core.dy, 2_539_703);
            assert_eq!(grid.latin1, 25_000_000);
            assert_eq!(grid.latin2, 25_000_000);
        }
        other => panic!("expected Lambert conformal grid, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_albers_equal_area_grib2_field_readable_by_reader() {
    let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(albers_grid(0))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    assert_eq!(message.grid_shape(), (3, 2));
    assert_eq!(
        message.projected_x_coordinates().unwrap().unwrap(),
        vec![0.0, 4_000.0, 8_000.0]
    );
    assert_eq!(
        message.projected_y_coordinates().unwrap().unwrap(),
        vec![-0.0, -5_000.0]
    );
    match message.grid_definition() {
        GridDefinition::AlbersEqualArea(grid) => {
            assert_eq!(grid.core.number_of_points, 6);
            assert_eq!(grid.core.shape_of_earth, 6);
            assert_eq!(grid.core.nx, 3);
            assert_eq!(grid.core.ny, 2);
            assert_eq!(grid.core.lat_first, 23_000_000);
            assert_eq!(grid.core.lon_first, 240_000_000);
            assert_eq!(grid.lat_d, 25_000_000);
            assert_eq!(grid.lon_v, 265_000_000);
            assert_eq!(grid.core.dx, 4_000_000);
            assert_eq!(grid.core.dy, 5_000_000);
            assert_eq!(grid.latin1, 29_500_000);
            assert_eq!(grid.latin2, 45_500_000);
        }
        other => panic!("expected Albers equal-area grid, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn roundtrips_projected_grid_logical_order_for_alternating_scan_rows() {
    let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(polar_grid(0b0001_0000))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    assert_eq!(
        file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
        values
    );
}

#[test]
fn rejects_projected_grid_point_count_mismatch_before_writing() {
    let mut grid = match polar_grid(0) {
        GridDefinition::PolarStereographic(grid) => grid,
        other => panic!("expected polar stereographic grid, got {other:?}"),
    };
    grid.core.number_of_points = 5;
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(GridDefinition::PolarStereographic(grid))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0, 5.0])
        .build()
        .unwrap_err();

    assert!(
        matches!(err, grib_core::Error::Other(message) if message.contains("number_of_points"))
    );
}

#[cfg(not(feature = "png"))]
#[test]
fn png_packing_requires_png_feature() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::PngAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();

    assert!(matches!(err, grib_core::Error::UnsupportedDataTemplate(41)));
}

#[cfg(not(feature = "jpeg2000"))]
#[test]
fn jpeg2000_packing_requires_jpeg2000_feature() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::Jpeg2000Auto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();

    assert!(matches!(err, grib_core::Error::UnsupportedDataTemplate(40)));
}

#[cfg(not(feature = "ccsds"))]
#[test]
fn ccsds_packing_requires_ccsds_feature() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::CcsdsAuto {
            decimal_scale: 0,
            flags: CcsdsFlags::DEFAULT,
            block_size: CcsdsBlockSize::THIRTY_TWO,
            reference_sample_interval: 128,
        })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();

    assert!(matches!(err, grib_core::Error::UnsupportedDataTemplate(42)));
}

#[cfg(feature = "png")]
#[test]
fn writes_png_grib2_field() {
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::PngAuto { decimal_scale: 0 })
        .values(&[12.0, 14.0, 16.0, 18.0])
        .build()
        .unwrap();

    match field.data_representation() {
        DataRepresentation::PngPacking(params) => {
            assert_eq!(params.packing.encoded_values, 4);
            assert_eq!(params.packing.reference_value, 12.0);
            assert_eq!(params.packing.bits_per_value, 4);
        }
        other => panic!("expected PNG packing, got {other:?}"),
    }

    let bytes = write_message([field]);
    let file = GribFile::from_bytes(bytes.clone()).unwrap();
    let message = file.message(0).unwrap();
    assert!(matches!(
        &message.metadata().data_representation,
        DataRepresentation::PngPacking(_)
    ));
    assert_eq!(
        message.read_flat_data_as_f64().unwrap(),
        vec![12.0, 14.0, 16.0, 18.0]
    );

    let payload = section_payload(&bytes, 7);
    let decoder = png::Decoder::new(std::io::Cursor::new(payload));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut decoded).unwrap();
    assert_eq!(info.width, 2);
    assert_eq!(info.height, 2);
    assert_eq!(info.color_type, png::ColorType::Grayscale);
    assert_eq!(info.bit_depth, png::BitDepth::Four);
    assert_eq!(&decoded[..info.buffer_size()], &[0x02, 0x46]);
}

#[cfg(feature = "png")]
#[test]
fn writes_png_projected_grib2_field() {
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(polar_grid(0))
        .product(product(0, 0))
        .packing(PackingStrategy::PngAuto { decimal_scale: 0 })
        .values(&[12.0, 14.0, 16.0, 18.0, 20.0, 22.0])
        .build()
        .unwrap();

    let bytes = write_message([field]);
    let file = GribFile::from_bytes(bytes.clone()).unwrap();
    let message = file.message(0).unwrap();
    assert!(matches!(
        &message.metadata().data_representation,
        DataRepresentation::PngPacking(_)
    ));
    assert!(matches!(
        message.grid_definition(),
        GridDefinition::PolarStereographic(_)
    ));
    assert_eq!(
        message.read_flat_data_as_f64().unwrap(),
        vec![12.0, 14.0, 16.0, 18.0, 20.0, 22.0]
    );

    let payload = section_payload(&bytes, 7);
    let decoder = png::Decoder::new(std::io::Cursor::new(payload));
    let reader = decoder.read_info().unwrap();
    let info = reader.info();
    assert_eq!(info.width, 3);
    assert_eq!(info.height, 2);
}

#[cfg(feature = "jpeg2000")]
#[test]
fn writes_jpeg2000_grib2_field() {
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::Jpeg2000Auto { decimal_scale: 0 })
        .values(&[12.0, 13.0, 14.0, 15.0])
        .build()
        .unwrap();

    match field.data_representation() {
        DataRepresentation::Jpeg2000Packing(params) => {
            assert_eq!(params.packing.encoded_values, 4);
            assert_eq!(params.packing.reference_value, 12.0);
            assert_eq!(params.packing.bits_per_value, 2);
            assert_eq!(params.compression_type, 0);
            assert_eq!(params.target_compression_ratio, 0);
        }
        other => panic!("expected JPEG2000 packing, got {other:?}"),
    }

    let bytes = write_message([field]);
    let file = GribFile::from_bytes(bytes.clone()).unwrap();
    let message = file.message(0).unwrap();
    assert!(matches!(
        &message.metadata().data_representation,
        DataRepresentation::Jpeg2000Packing(_)
    ));
    assert_eq!(
        message.read_flat_data_as_f64().unwrap(),
        vec![12.0, 13.0, 14.0, 15.0]
    );

    let payload = section_payload(&bytes, 7);
    assert!(payload.starts_with(&[0xff, 0x4f, 0xff, 0x51]));
}

#[cfg(feature = "jpeg2000")]
#[test]
fn writes_lossless_jpeg2000_fields_through_the_maximum_supported_precision() {
    for bits_per_value in [1u8, 8, 16, 20, 22, 24, 25] {
        let maximum = ((1u64 << bits_per_value) - 1) as f64;
        let values = (0..64)
            .map(|index| {
                if (index + index / 8) % 2 == 0 {
                    0.0
                } else {
                    maximum
                }
            })
            .collect::<Vec<_>>();
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_shape_and_scanning_mode(8, 8, 0))
            .product(product(0, 0))
            .packing(PackingStrategy::Jpeg2000Auto { decimal_scale: 0 })
            .values(&values)
            .build()
            .unwrap();

        let DataRepresentation::Jpeg2000Packing(params) = field.data_representation() else {
            panic!("expected JPEG2000 packing");
        };
        assert_eq!(params.packing.bits_per_value, bits_per_value);

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        assert_eq!(
            decoded, values,
            "precision failure at {bits_per_value} bits"
        );
    }
}

#[cfg(feature = "jpeg2000")]
#[test]
fn rejects_jpeg2000_values_that_need_more_than_25_bits() {
    let error = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_shape_and_scanning_mode(2, 1, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::Jpeg2000Auto { decimal_scale: 0 })
        .values(&[0.0, (1u64 << 25) as f64])
        .build()
        .unwrap_err();

    assert!(matches!(error, Error::UnsupportedPackingWidth(26)));
}

#[cfg(feature = "ccsds")]
#[test]
fn writes_signed_preprocessed_ccsds_field_readable_by_reader() {
    let values = [0.0, 127.0, 128.0, 255.0];
    let flags = CcsdsFlags::DEFAULT | CcsdsFlags::SIGNED;
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::CcsdsAuto {
            decimal_scale: 0,
            flags,
            block_size: CcsdsBlockSize::THIRTY_TWO,
            reference_sample_interval: 128,
        })
        .values(&values)
        .build()
        .unwrap();

    match field.data_representation() {
        DataRepresentation::CcsdsPacking(params) => {
            assert_eq!(params.packing().encoded_values, values.len());
            assert_eq!(params.packing().bits_per_value, 8);
            assert_eq!(params.flags(), flags);
        }
        other => panic!("expected CCSDS packing, got {other:?}"),
    }

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    assert_eq!(
        file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
        values
    );
}

#[cfg(feature = "ccsds")]
#[test]
fn writes_non_byte_signed_and_layout_independent_ccsds_streams() {
    let cases = [
        (
            vec![0.0, 31.0, 1.0, 30.0],
            CcsdsFlags::DEFAULT | CcsdsFlags::SIGNED,
            6,
        ),
        (
            vec![0.0, 16_777_215.0, 1.0, 0xaaaaaa as f64],
            CcsdsFlags::DEFAULT | CcsdsFlags::SIGNED,
            25,
        ),
        (
            vec![0.0, 1_048_575.0, 0x54321 as f64, 1.0],
            CcsdsFlags::NONE,
            20,
        ),
    ];

    for (values, flags, expected_bits) in cases {
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::CcsdsAuto {
                decimal_scale: 0,
                flags,
                block_size: CcsdsBlockSize::THIRTY_TWO,
                reference_sample_interval: 128,
            })
            .values(&values)
            .build()
            .unwrap();

        let DataRepresentation::CcsdsPacking(params) = field.data_representation() else {
            panic!("expected CCSDS packing");
        };
        assert_eq!(params.packing().bits_per_value, expected_bits);
        assert_eq!(params.flags(), flags);

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        assert_eq!(
            file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
            values
        );
    }
}

#[cfg(feature = "ccsds")]
#[test]
fn writes_constant_ccsds_reference_without_applying_scale_factors() {
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::CcsdsAuto {
            decimal_scale: 1,
            flags: CcsdsFlags::DEFAULT,
            block_size: CcsdsBlockSize::THIRTY_TWO,
            reference_sample_interval: 128,
        })
        .values(&[42.0; 4])
        .build()
        .unwrap();

    match field.data_representation() {
        DataRepresentation::CcsdsPacking(params) => {
            assert_eq!(params.packing().reference_value, 42.0);
            assert_eq!(params.packing().decimal_scale, 1);
            assert_eq!(params.packing().bits_per_value, 0);
        }
        other => panic!("expected CCSDS packing, got {other:?}"),
    }

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    assert_eq!(
        file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
        vec![42.0; 4]
    );
}

#[cfg(feature = "ccsds")]
#[test]
fn writes_ccsds_bitmap_and_all_missing_fields() {
    let build = |values: &[f64]| {
        Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::CcsdsAuto {
                decimal_scale: 0,
                flags: CcsdsFlags::DEFAULT,
                block_size: CcsdsBlockSize::THIRTY_TWO,
                reference_sample_interval: 128,
            })
            .values(values)
            .build()
            .unwrap()
    };
    let partial = build(&[5.0, f64::NAN, 7.0, 8.0]);
    let all_missing = build(&[f64::NAN; 4]);

    let DataRepresentation::CcsdsPacking(partial_params) = partial.data_representation() else {
        panic!("expected CCSDS packing");
    };
    assert_eq!(partial_params.packing().encoded_values, 3);
    let DataRepresentation::CcsdsPacking(missing_params) = all_missing.data_representation() else {
        panic!("expected CCSDS packing");
    };
    assert_eq!(missing_params.packing().encoded_values, 0);
    assert_eq!(missing_params.packing().bits_per_value, 0);

    let file = GribFile::from_bytes(write_message([partial, all_missing])).unwrap();
    let partial_values = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(partial_values[0], 5.0);
    assert!(partial_values[1].is_nan());
    assert_eq!(&partial_values[2..], &[7.0, 8.0]);
    assert!(file
        .message(1)
        .unwrap()
        .read_flat_data_as_f64()
        .unwrap()
        .iter()
        .all(|value| value.is_nan()));
}

#[cfg(feature = "ccsds")]
#[test]
fn writes_restricted_and_unenforced_ccsds_streams() {
    let cases = [
        (
            vec![0.0, 1.0, 15.0, 7.0],
            grid(),
            CcsdsFlags::DEFAULT | CcsdsFlags::RESTRICTED,
            CcsdsBlockSize::SIXTEEN,
        ),
        (
            (0..12).map(f64::from).collect::<Vec<_>>(),
            grid_with_shape_and_scanning_mode(12, 1, 0),
            CcsdsFlags::DEFAULT | CcsdsFlags::NOT_ENFORCE,
            CcsdsBlockSize::new(12).unwrap(),
        ),
    ];

    for (values, grid, flags, block_size) in cases {
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid)
            .product(product(0, 0))
            .packing(PackingStrategy::CcsdsAuto {
                decimal_scale: 0,
                flags,
                block_size,
                reference_sample_interval: 128,
            })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        assert_eq!(
            file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
            values
        );
    }
}

#[cfg(feature = "ccsds")]
#[test]
fn writes_incompressible_32_bit_ccsds_stream() {
    let values = [
        0.0,
        u32::MAX as f64,
        0x5555_5555u32 as f64,
        0xaaaa_aaaau32 as f64,
    ];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::CcsdsAuto {
            decimal_scale: 0,
            flags: CcsdsFlags::MSB,
            block_size: CcsdsBlockSize::EIGHT,
            reference_sample_interval: 1,
        })
        .values(&values)
        .build()
        .unwrap();

    let DataRepresentation::CcsdsPacking(params) = field.data_representation() else {
        panic!("expected CCSDS packing");
    };
    assert_eq!(params.packing().bits_per_value, 32);

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    assert_eq!(
        file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
        values
    );
}

#[cfg(feature = "ccsds")]
#[test]
fn rejects_ccsds_padding_and_widths_above_32_bits() {
    let padding_error = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::CcsdsAuto {
            decimal_scale: 0,
            flags: CcsdsFlags::DEFAULT | CcsdsFlags::PAD_RSI,
            block_size: CcsdsBlockSize::THIRTY_TWO,
            reference_sample_interval: 128,
        })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();
    assert!(matches!(
        padding_error,
        grib_core::Error::ValueOutOfRange(_)
    ));

    let width_error = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::CcsdsAuto {
            decimal_scale: 0,
            flags: CcsdsFlags::DEFAULT,
            block_size: CcsdsBlockSize::THIRTY_TWO,
            reference_sample_interval: 128,
        })
        .values(&[0.0, 4_294_967_296.0, 1.0, 2.0])
        .build()
        .unwrap_err();
    assert!(matches!(
        width_error,
        grib_core::Error::UnsupportedPackingWidth(33)
    ));
}

#[test]
fn writes_complex_grib2_field_readable_by_reader() {
    let values = (0..70)
        .map(|index| f64::from((index * 37) % 113) - 50.0)
        .collect::<Vec<_>>();
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_shape_and_scanning_mode(35, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: None,
        })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    match &message.metadata().data_representation {
        DataRepresentation::ComplexPacking(params) => {
            assert_eq!(params.num_groups, 3);
            assert_eq!(params.group_splitting_method, 1);
            assert_eq!(params.missing_value_management, 0);
            assert_eq!(params.group_length_reference, 32);
            assert_eq!(params.true_length_last_group, 6);
            assert_eq!(params.spatial_differencing, None);
        }
        other => panic!("expected complex packing, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_complex_grib2_decimal_scaled_values() {
    let values = [1.24, 2.34, -3.46, 4.56];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 1,
            spatial_differencing: None,
        })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    assert!(matches!(
        &message.metadata().data_representation,
        DataRepresentation::ComplexPacking(_)
    ));
    let decoded = message.read_flat_data_as_f64().unwrap();
    for (actual, expected) in decoded.iter().zip(values) {
        assert!((actual - expected).abs() <= 0.05);
    }
}

#[test]
fn writes_complex_grib2_bitmap_from_nan_values() {
    let values = [1.0, f64::NAN, 3.0, 4.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: None,
        })
        .values(&values)
        .build()
        .unwrap();

    let bytes = write_message([field]);
    assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 6, 7, 8]);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    match &message.metadata().data_representation {
        DataRepresentation::ComplexPacking(params) => assert_eq!(params.encoded_values, 3),
        other => panic!("expected complex packing, got {other:?}"),
    }
    let decoded = message.read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 1.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 3.0);
    assert_eq!(decoded[3], 4.0);
}

#[test]
fn writes_all_missing_complex_grib2_bitmap_field() {
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: None,
        })
        .values(&[f64::NAN; 4])
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    match &message.metadata().data_representation {
        DataRepresentation::ComplexPacking(params) => {
            assert_eq!(params.encoded_values, 0);
            assert_eq!(params.num_groups, 1);
            assert_eq!(params.true_length_last_group, 0);
        }
        other => panic!("expected complex packing, got {other:?}"),
    }
    let decoded = message.read_flat_data_as_f64().unwrap();
    assert!(decoded.iter().all(|value| value.is_nan()));
}

#[test]
fn writes_first_order_spatial_differencing_grib2_field() {
    let values = (0..70)
        .map(|index| f64::from((index * index + 7 * index) % 149) - 50.0)
        .collect::<Vec<_>>();
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_shape_and_scanning_mode(35, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: Some(SpatialDifferencingOrder::First),
        })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    match &message.metadata().data_representation {
        DataRepresentation::ComplexPacking(params) => {
            let spatial = params.spatial_differencing.unwrap();
            assert_eq!(spatial.order, 1);
            assert!(spatial.descriptor_octets >= 1);
            assert_eq!(params.num_groups, 3);
        }
        other => panic!("expected complex packing, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_second_order_spatial_differencing_grib2_field() {
    let values = (0..70)
        .map(|index| {
            let index = f64::from(index);
            index * index - 12.0 * index + 25.0
        })
        .collect::<Vec<_>>();
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_shape_and_scanning_mode(35, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: Some(SpatialDifferencingOrder::Second),
        })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    match &message.metadata().data_representation {
        DataRepresentation::ComplexPacking(params) => {
            let spatial = params.spatial_differencing.unwrap();
            assert_eq!(spatial.order, 2);
            assert!(spatial.descriptor_octets >= 1);
            assert_eq!(params.num_groups, 3);
        }
        other => panic!("expected complex packing, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
}

#[test]
fn writes_spatial_differencing_with_bitmap_missing_values() {
    let values = [1.0, f64::NAN, 4.0, 9.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: Some(SpatialDifferencingOrder::First),
        })
        .values(&values)
        .build()
        .unwrap();

    let bytes = write_message([field]);
    assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 6, 7, 8]);

    let file = GribFile::from_bytes(bytes).unwrap();
    let message = file.message(0).unwrap();
    match &message.metadata().data_representation {
        DataRepresentation::ComplexPacking(params) => {
            assert_eq!(params.encoded_values, 3);
            assert_eq!(params.spatial_differencing.unwrap().order, 1);
        }
        other => panic!("expected complex packing, got {other:?}"),
    }
    let decoded = message.read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 1.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 4.0);
    assert_eq!(decoded[3], 9.0);
}

#[test]
fn rejects_spatial_differencing_without_enough_present_values() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: Some(SpatialDifferencingOrder::Second),
        })
        .values(&[1.0, f64::NAN, f64::NAN, f64::NAN])
        .build()
        .unwrap_err();

    assert!(matches!(
        err,
        grib_core::Error::DataLengthMismatch {
            expected: 2,
            actual: 1
        }
    ));
}

#[test]
fn rejects_complex_packing_for_grib1() {
    let err = Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(grid())
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: None,
        })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap_err();

    assert!(matches!(
        err,
        grib_core::Error::UnsupportedPackingForEdition {
            edition: 1,
            packing: "complex",
        }
    ));
}

#[test]
fn writes_constant_field_with_zero_width_simple_packing() {
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[42.0, 42.0, 42.0, 42.0])
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let message = file.message(0).unwrap();
    match &message.metadata().data_representation {
        DataRepresentation::SimplePacking(params) => assert_eq!(params.bits_per_value, 0),
        other => panic!("expected simple packing, got {other:?}"),
    }
    assert_eq!(message.read_flat_data_as_f64().unwrap(), vec![42.0; 4]);
}

#[test]
fn writes_decimal_scaled_values_within_quantization_tolerance() {
    let values = [1.2, 2.3, 3.4, 4.5];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 1 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    for (actual, expected) in decoded.iter().zip(values) {
        assert!((actual - expected).abs() <= 0.05);
    }
}

#[test]
fn writes_bitmap_from_nan_values() {
    let values = [1.0, f64::NAN, 3.0, 4.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 1.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 3.0);
    assert_eq!(decoded[3], 4.0);
}

#[test]
fn writes_bitmap_from_explicit_mask() {
    let values = [1.0, 999.0, 3.0, 4.0];
    let bitmap = [true, false, true, true];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .bitmap(&bitmap)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 1.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 3.0);
    assert_eq!(decoded[3], 4.0);
}

#[test]
fn writes_all_missing_bitmap_field() {
    let values = [f64::NAN; 4];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert!(decoded.iter().all(|value| value.is_nan()));
}

#[test]
fn writes_single_grib2_message_with_multiple_fields() {
    let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
    let second = simple_field(&[5.0, 6.0, 7.0, 8.0], 0, 2);

    let bytes = write_message([first, second]);
    assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 7, 4, 5, 7, 8]);

    let file = GribFile::from_bytes(bytes).unwrap();
    assert_eq!(file.message_count(), 2);
    assert_eq!(file.message(0).unwrap().parameter_name(), "TMP");
    assert_eq!(file.message(1).unwrap().parameter_name(), "POT");
    assert_eq!(file.message(0).unwrap().grid_shape(), (2, 2));
    assert_eq!(file.message(1).unwrap().grid_shape(), (2, 2));
    assert_eq!(
        file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
        vec![1.0, 2.0, 3.0, 4.0]
    );
    assert_eq!(
        file.message(1).unwrap().read_flat_data_as_f64().unwrap(),
        vec![5.0, 6.0, 7.0, 8.0]
    );
}

#[test]
fn emits_new_grid_section_only_when_grid_changes() {
    let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
    let second = simple_field(&[5.0, 6.0, 7.0, 8.0], 0, 2);
    let third = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_shape_and_scanning_mode(3, 2, 0))
        .product(product(0, 4))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[9.0, 10.0, 11.0, 12.0, 13.0, 14.0])
        .build()
        .unwrap();

    let bytes = write_message([first, second, third]);
    assert_eq!(
        section_numbers(&bytes),
        vec![1, 3, 4, 5, 7, 4, 5, 7, 3, 4, 5, 7, 8]
    );

    let file = GribFile::from_bytes(bytes).unwrap();
    assert_eq!(file.message_count(), 3);
    assert_eq!(file.message(0).unwrap().parameter_name(), "TMP");
    assert_eq!(file.message(1).unwrap().parameter_name(), "POT");
    assert_eq!(file.message(2).unwrap().parameter_name(), "TMAX");
    assert_eq!(file.message(0).unwrap().grid_shape(), (2, 2));
    assert_eq!(file.message(1).unwrap().grid_shape(), (2, 2));
    assert_eq!(file.message(2).unwrap().grid_shape(), (3, 2));
    assert_eq!(
        file.message(2).unwrap().read_flat_data_as_f64().unwrap(),
        vec![9.0, 10.0, 11.0, 12.0, 13.0, 14.0]
    );
}

#[test]
fn writes_reused_grid_multifield_message_with_bitmap() {
    let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
    let second = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 2))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[5.0, f64::NAN, 7.0, 8.0])
        .build()
        .unwrap();

    let bytes = write_message([first, second]);
    assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 7, 4, 5, 6, 7, 8]);

    let file = GribFile::from_bytes(bytes).unwrap();
    assert_eq!(file.message_count(), 2);
    let decoded = file.message(1).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[0], 5.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 7.0);
    assert_eq!(decoded[3], 8.0);
}

#[test]
fn roundtrips_logical_row_major_order_for_supported_scan_modes() {
    let logical = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    for scanning_mode in [
        0b0000_0000,
        0b1000_0000,
        0b0100_0000,
        0b1100_0000,
        0b0001_0000,
        0b1001_0000,
    ] {
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_scanning_mode(scanning_mode))
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&logical)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        assert_eq!(
            file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
            logical,
            "scanning mode {scanning_mode:08b}"
        );
    }
}

#[test]
fn accepts_grib_scan_order_fast_path() {
    let scan_order = [1.0, 2.0, 3.0, 6.0, 5.0, 4.0];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_scanning_mode(0b0001_0000))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&scan_order)
        .value_order(ValueOrder::GribScanOrder)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    assert_eq!(
        file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
    );
}

#[test]
fn reorders_explicit_bitmap_with_logical_values() {
    let values = [1.0, 2.0, 3.0, 4.0, 999.0, 6.0];
    let bitmap = [true, true, true, true, false, true];
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_scanning_mode(0b0001_0000))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&values)
        .bitmap(&bitmap)
        .build()
        .unwrap();

    let file = GribFile::from_bytes(write_message([field])).unwrap();
    let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
    assert_eq!(decoded[..4], [1.0, 2.0, 3.0, 4.0]);
    assert!(decoded[4].is_nan());
    assert_eq!(decoded[5], 6.0);
}

#[test]
fn rejects_unsupported_scan_mode_before_writing() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid_with_scanning_mode(0b0010_0000))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .build()
        .unwrap_err();

    assert!(matches!(
        err,
        grib_core::Error::UnsupportedScanningMode(0b0010_0000)
    ));
}

#[test]
fn rejects_value_count_mismatch_before_writing() {
    let err = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0])
        .build()
        .unwrap_err();
    assert!(matches!(
        err,
        grib_core::Error::DataLengthMismatch {
            expected: 4,
            actual: 3
        }
    ));
}

#[derive(Debug, Deserialize)]
struct ReferenceDump {
    messages: Vec<ReferenceMessage>,
}

#[derive(Debug, Deserialize)]
struct ReferenceMessage {
    edition: u8,
    name: String,
    values: Vec<Option<f64>>,
}

#[test]
#[ignore = "requires GRIB_READER_ECCODES_HELPER"]
fn generated_grib1_fixture_matches_eccodes_when_configured() {
    let helper = std::env::var_os("GRIB_READER_ECCODES_HELPER")
        .expect("GRIB_READER_ECCODES_HELPER must be set");
    let bytes = write_grib1_message(grib1_simple_field(&[5.0, f64::NAN, 7.0, 8.0]));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("writer-generated.grib1");
    std::fs::write(&path, &bytes).unwrap();

    let output = Command::new(helper)
        .arg("dump")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ecCodes helper failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: ReferenceDump = serde_json::from_slice(&output.stdout).unwrap();
    let rust = GribFile::from_bytes(bytes).unwrap();

    assert_eq!(reference.messages.len(), 1);
    assert_eq!(rust.message_count(), reference.messages.len());
    let message = rust.message(0).unwrap();
    let actual = message.read_flat_data_as_f64().unwrap();
    let expected = &reference.messages[0];
    assert_eq!(message.edition(), expected.edition);
    assert_eq!(message.parameter_description(), expected.name);
    assert_eq!(actual.len(), expected.values.len());
    for (actual, expected) in actual.iter().zip(&expected.values) {
        match expected {
            Some(expected) => assert!((actual - expected).abs() <= 1e-6),
            None => assert!(actual.is_nan()),
        }
    }
}

#[test]
#[ignore = "requires GRIB_READER_ECCODES_HELPER"]
fn generated_grib2_fixture_matches_eccodes_when_configured() {
    let helper = std::env::var_os("GRIB_READER_ECCODES_HELPER")
        .expect("GRIB_READER_ECCODES_HELPER must be set");
    let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
    let second = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid())
        .product(product(0, 2))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[5.0, f64::NAN, 7.0, 8.0])
        .build()
        .unwrap();
    let bytes = write_message([first, second]);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("writer-generated.grib2");
    std::fs::write(&path, &bytes).unwrap();

    let output = Command::new(helper)
        .arg("dump")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ecCodes helper failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: ReferenceDump = serde_json::from_slice(&output.stdout).unwrap();
    let rust = GribFile::from_bytes(bytes).unwrap();

    assert_eq!(reference.messages.len(), 2);
    assert_eq!(rust.message_count(), reference.messages.len());
    for (index, expected) in reference.messages.iter().enumerate() {
        let message = rust.message(index).unwrap();
        let actual = message.read_flat_data_as_f64().unwrap();
        assert_eq!(message.edition(), expected.edition);
        assert_eq!(message.parameter_description(), expected.name);
        assert_eq!(actual.len(), expected.values.len());
        for (actual, expected) in actual.iter().zip(&expected.values) {
            match expected {
                Some(expected) => assert!((actual - expected).abs() <= 1e-6),
                None => assert!(actual.is_nan()),
            }
        }
    }
}
