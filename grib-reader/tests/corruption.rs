mod common;

use common::{
    build_grib1_message, build_grib2_complex_packing_message, build_grib2_message,
    build_grib2_spatial_differencing_message,
};
use grib_reader::{Error, GribFile};

fn expect_err(bytes: Vec<u8>) -> Error {
    match GribFile::from_bytes(bytes) {
        Ok(_) => panic!("expected error, got Ok"),
        Err(err) => err,
    }
}

#[test]
fn rejects_truncated_grib2_message() {
    let message = build_grib2_message(&[1, 2, 3, 4]);
    let err = expect_err(message[..message.len() - 2].to_vec());
    assert!(matches!(err, Error::Truncated { .. }));
}

#[test]
fn rejects_invalid_end_marker() {
    let mut message = build_grib2_message(&[1, 2, 3, 4]);
    let len = message.len();
    message[len - 4..].copy_from_slice(b"0000");
    let err = expect_err(message);
    assert!(matches!(err, Error::InvalidMessage(_)));
}

#[test]
fn rejects_invalid_grib2_section_order() {
    let mut message = build_grib2_message(&[1, 2, 3, 4]);
    // Change section 5 to section 7 to create an invalid order: 1,3,4,7,7.
    let replacement_index = 16 + 21 + 72 + 4;
    message[replacement_index] = 7;
    let err = expect_err(message);
    assert!(matches!(err, Error::InvalidSectionOrder(_)));
}

#[test]
fn rejects_grib1_without_grid_definition() {
    let mut message = build_grib1_message(&[1, 2, 3, 4]);
    // Clear the GDS-present bit in the PDS flag octet.
    message[8 + 7] = 0;
    let err = expect_err(message);
    assert!(matches!(err, Error::InvalidMessage(_)));
}

#[test]
fn rejects_impossibly_short_reported_message_length() {
    let mut message = build_grib2_message(&[1, 2, 3, 4]);
    message[8..16].copy_from_slice(&8u64.to_be_bytes());
    let err = expect_err(message);
    assert!(matches!(err, Error::InvalidMessage(_)));
}

#[test]
fn rejects_grib1_section_length_past_end_of_message() {
    let mut message = build_grib1_message(&[1, 2, 3, 4]);
    let gds_offset = 8 + 28;
    message[gds_offset..gds_offset + 3].copy_from_slice(&[0, 1, 0]);
    let err = expect_err(message);
    assert!(matches!(err, Error::Truncated { .. }));
}

#[test]
fn rejects_grib2_encoded_value_count_mismatch_without_bitmap() {
    let mut message = build_grib2_message(&[1, 2, 3, 4]);
    let section5_offset = 16 + 21 + 72 + 34;
    message[section5_offset + 5..section5_offset + 9].copy_from_slice(&3u32.to_be_bytes());

    let opened = GribFile::from_bytes(message).unwrap();
    let err = opened.message(0).unwrap().read_data_as_f64().unwrap_err();
    assert!(matches!(
        err,
        Error::DataLengthMismatch {
            expected: 4,
            actual: 3,
        }
    ));
}

#[test]
fn rejects_internal_end_marker_reached_via_bad_section_length() {
    let mut message = build_grib2_message(&[55, 55, 55, 55]);
    let section7_offset = 16 + 21 + 72 + 34 + 21;
    message[section7_offset..section7_offset + 4].copy_from_slice(&5u32.to_be_bytes());

    let err = expect_err(message);
    assert!(matches!(err, Error::InvalidMessage(_)));
}

#[test]
fn rejects_row_by_row_complex_packing() {
    let mut message = build_grib2_complex_packing_message();
    let section5_offset = 16 + 21 + 72 + 34;
    message[section5_offset + 21] = 0;

    let err = expect_err(message);
    assert!(matches!(err, Error::UnsupportedGroupSplittingMethod(0)));
}

#[test]
fn rejects_invalid_spatial_differencing_order() {
    let mut message = build_grib2_spatial_differencing_message();
    let section5_offset = 16 + 21 + 72 + 34;
    message[section5_offset + 47] = 3;

    let err = expect_err(message);
    assert!(matches!(err, Error::UnsupportedSpatialDifferencingOrder(3)));
}
