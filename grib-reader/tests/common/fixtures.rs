#![allow(dead_code)]

use grib_core::grib1::ProductDefinition as Grib1ProductDefinition;
use grib_core::{
    AnalysisOrForecastTemplate, FixedSurface, GridDefinition, Identification, LatLonGrid,
    ProductDefinition, ProductDefinitionTemplate, ReferenceTime,
};
use grib_writer::{Grib1FieldBuilder, Grib2FieldBuilder, GribWriter, PackingStrategy};

pub fn build_grib2_message(values: &[u8]) -> Vec<u8> {
    build_grib2_message_with_forecast(values, 0)
}

pub fn build_grib2_message_with_forecast(values: &[u8], forecast_time: u32) -> Vec<u8> {
    let field = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(2, 2))
        .product(grib2_product_with_forecast(0, 0, forecast_time))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(values)
        .build()
        .unwrap();
    write_grib2_message([field])
}

pub fn build_grib2_multifield_message() -> Vec<u8> {
    let first = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(2, 2))
        .product(grib2_product_with_forecast(0, 0, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[1u8, 2, 3, 4])
        .build()
        .unwrap();
    let second = Grib2FieldBuilder::new()
        .identification(identification())
        .grid(latlon_grid(2, 2))
        .product(grib2_product_with_forecast(0, 2, 0))
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
        .values(&[5u8, 6, 7, 8])
        .build()
        .unwrap();
    write_grib2_message([first, second])
}

pub fn build_bitmap_prefixed_stream() -> Vec<u8> {
    let mut bytes = b"junkGRIB\x00\x00\x00\x02not-a-real-message".to_vec();
    bytes.extend_from_slice(&build_grib2_message(&[9, 8, 7, 6]));
    bytes
}

pub fn build_truncated_grib2_message() -> Vec<u8> {
    let message = build_grib2_message(&[1, 2, 3, 4]);
    message[..message.len() - 2].to_vec()
}

pub fn build_grib1_bitmap_message() -> Vec<u8> {
    build_grib1_message_with_bitmap(&[9, 7], 3, 1, Some(&[0b1011_1111]))
}

pub fn build_grib1_message(values: &[u8]) -> Vec<u8> {
    build_grib1_message_with_bitmap(values, 2, 2, None)
}

pub fn build_grib1_message_with_bitmap(
    values: &[u8],
    ni: u16,
    nj: u16,
    bitmap_payload: Option<&[u8]>,
) -> Vec<u8> {
    let points = usize::from(ni) * usize::from(nj);
    let grid = latlon_grid(u32::from(ni), u32::from(nj));
    let mut builder = Grib1FieldBuilder::new()
        .product(grib1_product())
        .grid(grid)
        .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 });

    let logical_values = match bitmap_payload {
        Some(payload) => {
            let bitmap = bitmap_from_payload(payload, points);
            let mut present = values.iter().copied();
            let logical = bitmap
                .iter()
                .map(|is_present| {
                    if *is_present {
                        f64::from(present.next().expect("not enough present values"))
                    } else {
                        f64::NAN
                    }
                })
                .collect::<Vec<_>>();
            assert!(
                present.next().is_none(),
                "too many present values for bitmap fixture"
            );
            builder = builder.bitmap(&bitmap);
            logical
        }
        None => {
            assert_eq!(values.len(), points);
            values.iter().copied().map(f64::from).collect()
        }
    };

    let field = builder.values(&logical_values).build().unwrap();
    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes)
        .write_grib1_message(field)
        .unwrap();
    bytes
}

pub fn build_grib2_complex_packing_message() -> Vec<u8> {
    COMPLEX_PACKING_MESSAGE.to_vec()
}

pub fn build_grib2_complex_packing_message_with_missing() -> Vec<u8> {
    COMPLEX_PACKING_WITH_MISSING_MESSAGE.to_vec()
}

pub fn build_grib2_spatial_differencing_message() -> Vec<u8> {
    SPATIAL_DIFFERENCING_MESSAGE.to_vec()
}

fn write_grib2_message(fields: impl IntoIterator<Item = grib_writer::Grib2Field>) -> Vec<u8> {
    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes)
        .write_grib2_message(fields)
        .unwrap();
    bytes
}

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
        p1: 0,
        p2: 0,
        time_range_indicator: 0,
        average_count: 0,
        missing_count: 0,
        century: 21,
        subcenter_id: 0,
        decimal_scale: 0,
    }
}

fn grib2_product_with_forecast(
    parameter_category: u8,
    parameter_number: u8,
    forecast_time: u32,
) -> ProductDefinition {
    ProductDefinition {
        parameter_category,
        parameter_number,
        template: ProductDefinitionTemplate::AnalysisOrForecast(AnalysisOrForecastTemplate {
            generating_process: 2,
            forecast_time_unit: 1,
            forecast_time,
            first_surface: Some(FixedSurface {
                surface_type: 103,
                scale_factor: 0,
                scaled_value: 850,
            }),
            second_surface: None,
        }),
    }
}

fn latlon_grid(ni: u32, nj: u32) -> GridDefinition {
    let lat_first = 50_000_000;
    let lon_first = -120_000_000;
    GridDefinition::LatLon(LatLonGrid {
        ni,
        nj,
        lat_first,
        lon_first,
        lat_last: lat_first - (nj.saturating_sub(1) as i32) * 1_000_000,
        lon_last: lon_first + (ni.saturating_sub(1) as i32) * 1_000_000,
        di: 1_000_000,
        dj: 1_000_000,
        scanning_mode: 0,
    })
}

fn bitmap_from_payload(payload: &[u8], points: usize) -> Vec<bool> {
    (0..points)
        .map(|index| {
            let byte = payload
                .get(index / 8)
                .copied()
                .expect("bitmap payload too short");
            byte & (1 << (7 - (index % 8))) != 0
        })
        .collect()
}

// Complex packing templates remain fixed reader fixtures until grib-writer
// supports templates 5.2/5.3; do not reintroduce test-only encoders here.
const COMPLEX_PACKING_MESSAGE: &[u8] = &[
    71, 82, 73, 66, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 202, 0, 0, 0, 21, 1, 0, 7, 0, 0, 35, 1, 1, 7,
    234, 3, 20, 12, 0, 0, 0, 1, 0, 0, 0, 72, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 2, 250, 240, 128, 135,
    39, 14, 0, 0, 2, 235, 174, 64, 135, 23, 203, 192, 0, 15, 66, 64, 0, 15, 66, 64, 0, 0, 0, 0, 34,
    4, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 103, 0, 0, 0, 3, 82, 255, 0, 0, 0, 0, 0,
    0, 0, 0, 47, 5, 0, 0, 0, 4, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 1, 0, 255, 255, 255, 255, 255,
    255, 255, 255, 0, 0, 0, 2, 0, 2, 0, 0, 0, 2, 1, 0, 0, 0, 2, 0, 0, 0, 0, 8, 7, 116, 112, 68, 55,
    55, 55, 55,
];

const COMPLEX_PACKING_WITH_MISSING_MESSAGE: &[u8] = &[
    71, 82, 73, 66, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 202, 0, 0, 0, 21, 1, 0, 7, 0, 0, 35, 1, 1, 7,
    234, 3, 20, 12, 0, 0, 0, 1, 0, 0, 0, 72, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 2, 250, 240, 128, 135,
    39, 14, 0, 0, 2, 235, 174, 64, 135, 23, 203, 192, 0, 15, 66, 64, 0, 15, 66, 64, 0, 0, 0, 0, 34,
    4, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 103, 0, 0, 0, 3, 82, 255, 0, 0, 0, 0, 0,
    0, 0, 0, 47, 5, 0, 0, 0, 4, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 1, 1, 255, 255, 255, 255, 255,
    255, 255, 255, 0, 0, 0, 2, 0, 2, 0, 0, 0, 2, 1, 0, 0, 0, 2, 0, 0, 0, 0, 8, 7, 121, 144, 52, 55,
    55, 55, 55,
];

const SPATIAL_DIFFERENCING_MESSAGE: &[u8] = &[
    71, 82, 73, 66, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 208, 0, 0, 0, 21, 1, 0, 7, 0, 0, 35, 1, 1, 7,
    234, 3, 20, 12, 0, 0, 0, 1, 0, 0, 0, 72, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 2, 250, 240, 128, 135,
    39, 14, 0, 0, 2, 235, 174, 64, 135, 23, 203, 192, 0, 15, 66, 64, 0, 15, 66, 64, 0, 0, 0, 0, 34,
    4, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 103, 0, 0, 0, 3, 82, 255, 0, 0, 0, 0, 0,
    0, 0, 0, 49, 5, 0, 0, 0, 4, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 255, 255, 255, 255, 255,
    255, 255, 255, 0, 0, 0, 2, 0, 1, 0, 0, 0, 2, 1, 0, 0, 0, 2, 0, 1, 2, 0, 0, 0, 12, 7, 0, 10, 0,
    2, 16, 64, 64, 55, 55, 55, 55,
];
