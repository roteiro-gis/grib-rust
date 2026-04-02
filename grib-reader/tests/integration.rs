mod common;

use std::io::Write;

use common::{
    build_grib1_message, build_grib1_message_with_bitmap, build_grib2_complex_packing_message,
    build_grib2_complex_packing_message_with_missing, build_grib2_message,
    build_grib2_message_with_forecast, build_grib2_multifield_message,
    build_grib2_spatial_differencing_message,
};
use grib_reader::{GribFile, OpenOptions};

#[test]
fn open_grib2_from_file_and_decode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.grib2");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&build_grib2_message(&[1, 2, 3, 4])).unwrap();

    let opened = GribFile::open(&path).unwrap();
    assert_eq!(opened.edition(), 2);
    assert_eq!(opened.message_count(), 1);
    let field = opened.message(0).unwrap();
    assert_eq!(field.parameter_name(), "TMP");
    assert_eq!(field.reference_time().year, 2026);
    assert_eq!(
        field.read_flat_data_as_f64().unwrap(),
        vec![1.0, 2.0, 3.0, 4.0]
    );
    assert_eq!(
        field
            .read_data_as_f64()
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![1.0, 2.0, 3.0, 4.0]
    );
}

#[test]
fn open_grib1_from_file_and_decode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.grib1");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&build_grib1_message(&[5, 6, 7, 8])).unwrap();

    let opened = GribFile::open(&path).unwrap();
    assert_eq!(opened.edition(), 1);
    assert_eq!(opened.message_count(), 1);
    let field = opened.message(0).unwrap();
    assert_eq!(field.parameter_name(), "TMP");
    assert_eq!(field.center_id(), 7);
    assert!(field.grib1_product_definition().is_some());
    assert_eq!(
        field
            .read_data_as_f64()
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![5.0, 6.0, 7.0, 8.0]
    );
}

#[test]
fn computes_valid_time_from_forecast_lead() {
    let opened =
        GribFile::from_bytes(build_grib2_message_with_forecast(&[1, 2, 3, 4], 18)).unwrap();
    let field = opened.message(0).unwrap();
    let valid = field.valid_time().unwrap();

    assert_eq!(field.forecast_time_unit(), Some(1));
    assert_eq!(field.forecast_time(), Some(18));
    assert_eq!(valid.year, 2026);
    assert_eq!(valid.month, 3);
    assert_eq!(valid.day, 21);
    assert_eq!(valid.hour, 6);
    assert_eq!(valid.minute, 0);
    assert_eq!(valid.second, 0);
}

#[test]
fn iterates_multifield_grib2_message() {
    let opened = GribFile::from_bytes(build_grib2_multifield_message()).unwrap();
    let names = opened
        .messages()
        .map(|message| message.parameter_name())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["TMP", "POT"]);
}

#[test]
fn tolerant_open_skips_malformed_candidates() {
    let mut bytes = b"junkGRIB\x00\x00\x00\x02not-a-real-message".to_vec();
    bytes.extend_from_slice(&build_grib2_message(&[9, 8, 7, 6]));

    let opened = GribFile::from_bytes_with_options(bytes, OpenOptions { strict: false }).unwrap();

    assert_eq!(opened.message_count(), 1);
    assert_eq!(
        opened
            .message(0)
            .unwrap()
            .read_data_as_f64()
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![9.0, 8.0, 7.0, 6.0]
    );
}

#[test]
fn open_grib1_bitmap_field_ignores_padding_bits() {
    let opened = GribFile::from_bytes(build_grib1_message_with_bitmap(
        &[9, 7],
        3,
        1,
        Some(&[0b1011_1111]),
    ))
    .unwrap();

    let decoded = opened
        .message(0)
        .unwrap()
        .read_data_as_f64()
        .unwrap()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0], 9.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 7.0);
}

#[test]
fn open_grib2_complex_packing_field_and_decode() {
    let opened = GribFile::from_bytes(build_grib2_complex_packing_message()).unwrap();
    let decoded = opened
        .message(0)
        .unwrap()
        .read_data_as_f64()
        .unwrap()
        .iter()
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(decoded, vec![3.0, 4.0, 5.0, 9.0]);
}

#[test]
fn open_grib2_complex_packing_field_with_missing_values() {
    let opened = GribFile::from_bytes(build_grib2_complex_packing_message_with_missing()).unwrap();
    let decoded = opened
        .message(0)
        .unwrap()
        .read_data_as_f64()
        .unwrap()
        .iter()
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(decoded[0], 7.0);
    assert!(decoded[1].is_nan());
    assert_eq!(decoded[2], 9.0);
    assert!(decoded[3].is_nan());
}

#[test]
fn open_grib2_spatial_differencing_field_and_decode() {
    let opened = GribFile::from_bytes(build_grib2_spatial_differencing_message()).unwrap();
    let decoded = opened
        .message(0)
        .unwrap()
        .read_data_as_f64()
        .unwrap()
        .iter()
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(decoded, vec![10.0, 12.0, 15.0, 19.0]);
}
