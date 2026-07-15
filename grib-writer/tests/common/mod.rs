#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use grib_core::metadata::ReferenceTime;
use grib_core::{
    AnalysisOrForecastTemplate, DerivedForecastTemplate, DerivedStatisticalProcessTemplate,
    FixedSurface, GridDefinition, Identification, LatLonGrid, PercentileForecastTemplate,
    PercentileStatisticalProcessTemplate, ProbabilityForecastTemplate, ProbabilityLimit,
    ProbabilityStatisticalProcessTemplate, ProbabilityType, ProductDefinition,
    ProductDefinitionTemplate, StatisticalInterval, StatisticalTimeRange,
};
use grib_reader::GribFile;
use grib_writer::{
    Grib1Field, Grib1FieldBuilder, Grib1ProductDefinition, Grib2Field, Grib2FieldBuilder,
    GribWriter, PackingStrategy, SpatialDifferencingOrder,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ReferenceDump {
    pub messages: Vec<ReferenceMessage>,
}

#[derive(Debug, Deserialize)]
pub struct ReferenceMessage {
    pub edition: u8,
    pub name: String,
    pub reference_time: ReferenceTimeDump,
    pub ni: usize,
    pub nj: usize,
    pub product_definition_template_number: Option<i64>,
    pub derived_forecast: Option<i64>,
    pub number_of_forecasts_in_ensemble: Option<i64>,
    pub forecast_probability_number: Option<i64>,
    pub total_number_of_forecast_probabilities: Option<i64>,
    pub probability_type: Option<i64>,
    pub scale_factor_of_lower_limit: Option<i64>,
    pub scaled_value_of_lower_limit: Option<i64>,
    pub scale_factor_of_upper_limit: Option<i64>,
    pub scaled_value_of_upper_limit: Option<i64>,
    pub percentile_value: Option<i64>,
    pub interval_end_year: Option<i64>,
    pub interval_end_month: Option<i64>,
    pub interval_end_day: Option<i64>,
    pub interval_end_hour: Option<i64>,
    pub interval_end_minute: Option<i64>,
    pub interval_end_second: Option<i64>,
    pub number_of_time_ranges: Option<i64>,
    pub number_missing_in_statistical_process: Option<i64>,
    pub type_of_statistical_processing: Option<i64>,
    pub type_of_time_increment: Option<i64>,
    pub time_range_unit: Option<i64>,
    pub time_range_length: Option<i64>,
    pub time_increment_unit: Option<i64>,
    pub time_increment: Option<i64>,
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
pub struct ReferenceTimeDump {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

pub fn helper_path() -> Option<PathBuf> {
    let path = std::env::var_os("GRIB_READER_ECCODES_HELPER")?;
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
}

pub fn dump_reference(helper: &Path, path: &Path) -> ReferenceDump {
    let output = Command::new(helper)
        .arg("dump")
        .arg(path)
        .output()
        .unwrap_or_else(|err| panic!("failed to run {}: {err}", helper.display()));
    assert!(
        output.status.success(),
        "reference dump failed for {}:\nstdout:\n{}\nstderr:\n{}",
        path.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "failed parsing reference dump for {}: {err}",
            path.display()
        )
    })
}

pub fn assert_matches_reference(helper: &Path, path: &Path, bytes: &[u8]) {
    let rust = GribFile::from_bytes(bytes.to_vec()).unwrap_or_else(|err| {
        panic!(
            "failed opening writer fixture {} with Rust decoder: {err}",
            path.display()
        )
    });
    let reference = dump_reference(helper, path);

    assert_eq!(
        rust.message_count(),
        reference.messages.len(),
        "message count mismatch for {}",
        path.display()
    );

    for (index, expected) in reference.messages.iter().enumerate() {
        let message = rust.message(index).unwrap();
        let actual = message.read_flat_data_as_f64().unwrap();

        assert_eq!(
            message.edition(),
            expected.edition,
            "edition mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.parameter_description(),
            expected.name,
            "parameter description mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.reference_time().year,
            expected.reference_time.year,
            "reference year mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.reference_time().month,
            expected.reference_time.month,
            "reference month mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.reference_time().day,
            expected.reference_time.day,
            "reference day mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.reference_time().hour,
            expected.reference_time.hour,
            "reference hour mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.reference_time().minute,
            expected.reference_time.minute,
            "reference minute mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.reference_time().second,
            expected.reference_time.second,
            "reference second mismatch for {} field {}",
            path.display(),
            index
        );
        assert_eq!(
            message.grid_shape(),
            (expected.ni, expected.nj),
            "grid shape mismatch for {} field {}",
            path.display(),
            index
        );
        assert_product_metadata(&message, expected, path, index);
        assert_eq!(
            actual.len(),
            expected.values.len(),
            "value count mismatch for {} field {}",
            path.display(),
            index
        );

        for (value_index, (actual, expected)) in actual.iter().zip(&expected.values).enumerate() {
            match expected {
                Some(expected) => {
                    let tolerance = expected.abs().max(actual.abs()).max(1.0) * 1e-6;
                    let diff = (actual - expected).abs();
                    assert!(
                        diff <= tolerance,
                        "value mismatch for {} field {} value {}: rust={} eccodes={} diff={} tolerance={}",
                        path.display(),
                        index,
                        value_index,
                        actual,
                        expected,
                        diff,
                        tolerance
                    );
                }
                None => {
                    assert!(
                        actual.is_nan(),
                        "missing-value mismatch for {} field {} value {}: rust={} eccodes=null",
                        path.display(),
                        index,
                        value_index,
                        actual
                    );
                }
            }
        }
    }
}

fn assert_product_metadata(
    message: &grib_reader::Message<'_>,
    expected: &ReferenceMessage,
    path: &Path,
    field_index: usize,
) {
    if message.edition() != 2 {
        return;
    }

    let product = message.product_definition().unwrap_or_else(|| {
        panic!(
            "missing product metadata for {} field {}",
            path.display(),
            field_index
        )
    });
    assert_eq!(
        expected.product_definition_template_number,
        Some(i64::from(product.template_number())),
        "product template mismatch for {} field {}",
        path.display(),
        field_index
    );

    match &product.template {
        ProductDefinitionTemplate::DerivedForecast(template) => {
            assert_derived_product_metadata(template, expected)
        }
        ProductDefinitionTemplate::ProbabilityForecast(template) => {
            assert_probability_product_metadata(template, expected)
        }
        ProductDefinitionTemplate::PercentileForecast(template) => {
            assert_percentile_product_metadata(template, expected)
        }
        ProductDefinitionTemplate::StatisticalProcess(template) => {
            assert_statistical_interval_metadata(&template.interval, expected)
        }
        ProductDefinitionTemplate::ProbabilityStatisticalProcess(template) => {
            assert_probability_product_metadata(&template.probability, expected);
            assert_statistical_interval_metadata(&template.interval, expected);
        }
        ProductDefinitionTemplate::PercentileStatisticalProcess(template) => {
            assert_percentile_product_metadata(&template.percentile, expected);
            assert_statistical_interval_metadata(&template.interval, expected);
        }
        ProductDefinitionTemplate::EnsembleStatisticalProcess(template) => {
            assert_statistical_interval_metadata(&template.interval, expected)
        }
        ProductDefinitionTemplate::DerivedStatisticalProcess(template) => {
            assert_derived_product_metadata(&template.derived, expected);
            assert_statistical_interval_metadata(&template.interval, expected);
        }
        _ => {}
    }
}

fn assert_derived_product_metadata(actual: &DerivedForecastTemplate, expected: &ReferenceMessage) {
    assert_eq!(
        expected.derived_forecast,
        Some(i64::from(actual.derived_forecast_type))
    );
    assert_eq!(
        expected.number_of_forecasts_in_ensemble,
        Some(i64::from(actual.number_of_forecasts_in_ensemble))
    );
}

fn assert_probability_product_metadata(
    actual: &ProbabilityForecastTemplate,
    expected: &ReferenceMessage,
) {
    assert_eq!(
        expected.forecast_probability_number,
        Some(i64::from(actual.forecast_probability_number))
    );
    assert_eq!(
        expected.total_number_of_forecast_probabilities,
        Some(i64::from(actual.total_number_of_forecast_probabilities))
    );
    assert_eq!(
        expected.probability_type,
        Some(i64::from(actual.probability.code()))
    );
    assert_probability_limit_metadata(
        actual.probability.lower_limit(),
        expected.scale_factor_of_lower_limit,
        expected.scaled_value_of_lower_limit,
    );
    assert_probability_limit_metadata(
        actual.probability.upper_limit(),
        expected.scale_factor_of_upper_limit,
        expected.scaled_value_of_upper_limit,
    );
}

fn assert_percentile_product_metadata(
    actual: &PercentileForecastTemplate,
    expected: &ReferenceMessage,
) {
    assert_eq!(
        expected.percentile_value,
        Some(i64::from(actual.percentile_value))
    );
}

fn assert_statistical_interval_metadata(actual: &StatisticalInterval, expected: &ReferenceMessage) {
    let end = actual.end_of_overall_time_interval;
    assert_eq!(expected.interval_end_year, Some(i64::from(end.year)));
    assert_eq!(expected.interval_end_month, Some(i64::from(end.month)));
    assert_eq!(expected.interval_end_day, Some(i64::from(end.day)));
    assert_eq!(expected.interval_end_hour, Some(i64::from(end.hour)));
    assert_eq!(expected.interval_end_minute, Some(i64::from(end.minute)));
    assert_eq!(expected.interval_end_second, Some(i64::from(end.second)));
    assert_eq!(
        expected.number_of_time_ranges,
        Some(i64::try_from(actual.time_ranges.len()).unwrap())
    );
    assert_eq!(
        expected.number_missing_in_statistical_process,
        Some(i64::from(actual.number_of_missing_in_statistical_process))
    );

    if let [range] = actual.time_ranges.as_slice() {
        assert_eq!(
            expected.type_of_statistical_processing,
            Some(i64::from(range.type_of_statistical_processing))
        );
        assert_eq!(
            expected.type_of_time_increment,
            Some(i64::from(range.type_of_time_increment))
        );
        assert_eq!(
            expected.time_range_unit,
            Some(i64::from(range.time_range_unit))
        );
        assert_eq!(
            expected.time_range_length,
            Some(i64::from(range.time_range_length))
        );
        assert_eq!(
            expected.time_increment_unit,
            Some(i64::from(range.time_increment_unit))
        );
        assert_eq!(
            expected.time_increment,
            Some(i64::from(range.time_increment))
        );
    }
}

fn assert_probability_limit_metadata(
    actual: Option<ProbabilityLimit>,
    expected_scale_factor: Option<i64>,
    expected_scaled_value: Option<i64>,
) {
    assert_eq!(
        actual.map(|limit| i64::from(limit.scale_factor)),
        expected_scale_factor
    );
    assert_eq!(
        actual.map(|limit| i64::from(limit.scaled_value)),
        expected_scaled_value
    );
}

pub fn writer_reference_samples() -> Vec<(&'static str, Vec<u8>)> {
    let decimal = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(2, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 1 })
        .values(&[1.2, 2.3, 3.4, 4.5])
        .build()
        .unwrap();
    let complex_values = (0..70)
        .map(|index| f64::from((index * 37) % 113) / 10.0 - 5.0)
        .collect::<Vec<_>>();
    let complex = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(35, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 1,
            spatial_differencing: None,
        })
        .values(&complex_values)
        .build()
        .unwrap();
    let spatial_first_values = (0..70)
        .map(|index| f64::from((index * index + 7 * index) % 149) - 50.0)
        .collect::<Vec<_>>();
    let spatial_first = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(35, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: Some(SpatialDifferencingOrder::First),
        })
        .values(&spatial_first_values)
        .build()
        .unwrap();
    let spatial_second_values = (0..70)
        .map(|index| {
            let index = f64::from(index);
            index * index - 12.0 * index + 25.0
        })
        .collect::<Vec<_>>();
    let spatial_second = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(35, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::ComplexAuto {
            decimal_scale: 0,
            spatial_differencing: Some(SpatialDifferencingOrder::Second),
        })
        .values(&spatial_second_values)
        .build()
        .unwrap();
    let mut signed_forecast_product = product(0, 0);
    let ProductDefinitionTemplate::AnalysisOrForecast(template) =
        &mut signed_forecast_product.template
    else {
        unreachable!("product helper always returns template 4.0");
    };
    template.background_generating_process_identifier = 7;
    template.generating_process_identifier = 42;
    template.hours_after_data_cutoff = Some(12);
    template.minutes_after_data_cutoff = Some(30);
    template.forecast_time = -6;
    let signed_forecast = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(2, 2, 0))
        .product(signed_forecast_product)
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1.0, 2.0, 3.0, 4.0])
        .build()
        .unwrap();
    let product_field = |template| {
        Grib2FieldBuilder::new()
            .identification(identification())
            .grid(latlon_grid(2, 2, 0))
            .product(ProductDefinition {
                parameter_category: 0,
                parameter_number: 0,
                template,
            })
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[1.0, 2.0, 3.0, 4.0])
            .build()
            .unwrap()
    };
    let derived = product_field(ProductDefinitionTemplate::DerivedForecast(
        DerivedForecastTemplate {
            base: analysis_or_forecast_template(),
            derived_forecast_type: 4,
            number_of_forecasts_in_ensemble: 50,
        },
    ));
    let probability_type = ProbabilityType::BelowLowerLimit(ProbabilityLimit {
        scale_factor: 1,
        scaled_value: 2732,
    });
    let probability = product_field(ProductDefinitionTemplate::ProbabilityForecast(
        ProbabilityForecastTemplate {
            base: analysis_or_forecast_template(),
            forecast_probability_number: 1,
            total_number_of_forecast_probabilities: 10,
            probability: probability_type,
        },
    ));
    let percentile = product_field(ProductDefinitionTemplate::PercentileForecast(
        PercentileForecastTemplate {
            base: analysis_or_forecast_template(),
            percentile_value: 90,
        },
    ));
    let probability_interval =
        product_field(ProductDefinitionTemplate::ProbabilityStatisticalProcess(
            ProbabilityStatisticalProcessTemplate {
                probability: ProbabilityForecastTemplate {
                    base: analysis_or_forecast_template(),
                    forecast_probability_number: 1,
                    total_number_of_forecast_probabilities: 10,
                    probability: probability_type,
                },
                interval: statistical_interval(),
            },
        ));
    let percentile_interval =
        product_field(ProductDefinitionTemplate::PercentileStatisticalProcess(
            PercentileStatisticalProcessTemplate {
                percentile: PercentileForecastTemplate {
                    base: analysis_or_forecast_template(),
                    percentile_value: 90,
                },
                interval: statistical_interval(),
            },
        ));
    let derived_interval = product_field(ProductDefinitionTemplate::DerivedStatisticalProcess(
        DerivedStatisticalProcessTemplate {
            derived: DerivedForecastTemplate {
                base: analysis_or_forecast_template(),
                derived_forecast_type: 4,
                number_of_forecasts_in_ensemble: 50,
            },
            interval: statistical_interval(),
        },
    ));

    vec![
        (
            "writer-simple.grib2",
            write_grib2_message([simple_grib2_field(&[1.0, 2.0, 3.0, 4.0], 0, 0)]),
        ),
        (
            "writer-bitmap.grib2",
            write_grib2_message([simple_grib2_field(&[5.0, f64::NAN, 7.0, 8.0], 0, 0)]),
        ),
        ("writer-decimal.grib2", write_grib2_message([decimal])),
        (
            "writer-signed-forecast.grib2",
            write_grib2_message([signed_forecast]),
        ),
        ("writer-derived.grib2", write_grib2_message([derived])),
        (
            "writer-probability.grib2",
            write_grib2_message([probability]),
        ),
        ("writer-percentile.grib2", write_grib2_message([percentile])),
        (
            "writer-probability-interval.grib2",
            write_grib2_message([probability_interval]),
        ),
        (
            "writer-percentile-interval.grib2",
            write_grib2_message([percentile_interval]),
        ),
        (
            "writer-derived-interval.grib2",
            write_grib2_message([derived_interval]),
        ),
        ("writer-complex.grib2", write_grib2_message([complex])),
        (
            "writer-complex-spatial-first.grib2",
            write_grib2_message([spatial_first]),
        ),
        (
            "writer-complex-spatial-second.grib2",
            write_grib2_message([spatial_second]),
        ),
        (
            "writer-multifield.grib2",
            write_grib2_message([
                simple_grib2_field(&[1.0, 2.0, 3.0, 4.0], 0, 0),
                simple_grib2_field(&[5.0, f64::NAN, 7.0, 8.0], 0, 2),
            ]),
        ),
        (
            "writer-simple.grib1",
            write_grib1_message(simple_grib1_field(&[5.0, 6.0, 7.0, 8.0])),
        ),
        (
            "writer-bitmap.grib1",
            write_grib1_message(simple_grib1_field(&[5.0, f64::NAN, 7.0, 8.0])),
        ),
    ]
}

pub fn simple_grib2_field(
    values: &[f64],
    parameter_category: u8,
    parameter_number: u8,
) -> Grib2Field {
    grib2_field(
        latlon_grid(2, 2, 0),
        values,
        parameter_category,
        parameter_number,
        0,
    )
}

pub fn grib2_field(
    grid: GridDefinition,
    values: &[f64],
    parameter_category: u8,
    parameter_number: u8,
    decimal_scale: i16,
) -> Grib2Field {
    Grib2FieldBuilder::new()
        .identification(identification())
        .grid(grid)
        .product(product(parameter_category, parameter_number))
        .packing(PackingStrategy::SimpleAuto { decimal_scale })
        .values(values)
        .build()
        .unwrap()
}

pub fn simple_grib1_field(values: &[f64]) -> Grib1Field {
    Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(latlon_grid(2, 2, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(values)
        .build()
        .unwrap()
}

pub fn write_grib2_message(fields: impl IntoIterator<Item = Grib2Field>) -> Vec<u8> {
    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes)
        .write_grib2_message(fields)
        .unwrap();
    bytes
}

pub fn write_grib1_message(field: Grib1Field) -> Vec<u8> {
    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes)
        .write_grib1_message(field)
        .unwrap();
    bytes
}

pub fn identification() -> Identification {
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

pub fn grib1_product() -> Grib1ProductDefinition {
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

pub fn product(parameter_category: u8, parameter_number: u8) -> ProductDefinition {
    ProductDefinition {
        parameter_category,
        parameter_number,
        template: ProductDefinitionTemplate::AnalysisOrForecast(analysis_or_forecast_template()),
    }
}

pub fn analysis_or_forecast_template() -> AnalysisOrForecastTemplate {
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

pub fn statistical_interval() -> StatisticalInterval {
    StatisticalInterval {
        end_of_overall_time_interval: ReferenceTime {
            year: 2026,
            month: 3,
            day: 20,
            hour: 18,
            minute: 0,
            second: 0,
        },
        number_of_missing_in_statistical_process: 0,
        time_ranges: vec![StatisticalTimeRange {
            type_of_statistical_processing: 1,
            type_of_time_increment: 2,
            time_range_unit: 1,
            time_range_length: 6,
            time_increment_unit: 255,
            time_increment: 0,
        }],
    }
}

pub fn latlon_grid(ni: u32, nj: u32, scanning_mode: u8) -> GridDefinition {
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

pub fn dense_values(ni: u32, nj: u32) -> Vec<f64> {
    (0..ni * nj)
        .map(|index| f64::from((index * 37) % 1000) / 10.0)
        .collect()
}

pub fn bitmap_values(ni: u32, nj: u32) -> Vec<f64> {
    dense_values(ni, nj)
        .into_iter()
        .enumerate()
        .map(|(index, value)| if index % 11 == 0 { f64::NAN } else { value })
        .collect()
}
