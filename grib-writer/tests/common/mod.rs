#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use grib_core::metadata::ReferenceTime;
use grib_core::{
    AnalysisOrForecastTemplate, FixedSurface, GridDefinition, Identification, LatLonGrid,
    ProductDefinition, ProductDefinitionTemplate,
};
use grib_reader::GribFile;
use grib_writer::{
    Grib1Field, Grib1FieldBuilder, Grib1ProductDefinition, Grib2Field, Grib2FieldBuilder,
    GribWriter, PackingStrategy,
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

pub fn writer_reference_samples() -> Vec<(&'static str, Vec<u8>)> {
    let decimal = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(2, 2, 0))
        .product(product(0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 1 })
        .values(&[1.2, 2.3, 3.4, 4.5])
        .build()
        .unwrap();

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
        template: ProductDefinitionTemplate::AnalysisOrForecast(AnalysisOrForecastTemplate {
            generating_process: 2,
            forecast_time_unit: 1,
            forecast_time: 6,
            first_surface: Some(FixedSurface {
                surface_type: 103,
                scale_factor: 0,
                scaled_value: 850,
            }),
            second_surface: None,
        }),
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
