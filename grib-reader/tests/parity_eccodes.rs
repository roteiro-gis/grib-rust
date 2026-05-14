mod common;

use std::path::Path;

use common::{
    build_grib1_bitmap_message, build_grib1_message, build_grib2_complex_packing_message,
    build_grib2_complex_packing_message_with_missing, build_grib2_lambert_message,
    build_grib2_message, build_grib2_multifield_message, build_grib2_polar_stereographic_message,
    build_grib2_spatial_differencing_message, collect_parity_samples, dump_reference, helper_path,
    write_fixture,
};
use grib_reader::{DataRepresentation, GribFile};

#[test]
#[ignore = "requires GRIB_READER_ECCODES_HELPER"]
fn generated_fixtures_match_eccodes_when_configured() {
    let helper = helper_path().unwrap_or_else(|| {
        panic!("GRIB_READER_ECCODES_HELPER must be set to run ecCodes parity tests")
    });

    let dir = tempfile::tempdir().unwrap();
    let fixtures = [
        write_fixture(
            dir.path(),
            "sample.grib1",
            &build_grib1_message(&[5, 6, 7, 8]),
        ),
        write_fixture(dir.path(), "bitmap.grib1", &build_grib1_bitmap_message()),
        write_fixture(
            dir.path(),
            "sample.grib2",
            &build_grib2_message(&[1, 2, 3, 4]),
        ),
        write_fixture(
            dir.path(),
            "multifield.grib2",
            &build_grib2_multifield_message(),
        ),
        write_fixture(
            dir.path(),
            "complex.grib2",
            &build_grib2_complex_packing_message(),
        ),
        write_fixture(
            dir.path(),
            "complex-missing.grib2",
            &build_grib2_complex_packing_message_with_missing(),
        ),
        write_fixture(
            dir.path(),
            "spatial-differencing.grib2",
            &build_grib2_spatial_differencing_message(),
        ),
        write_fixture(dir.path(), "lambert.grib2", &build_grib2_lambert_message()),
        write_fixture(
            dir.path(),
            "polar-stereographic.grib2",
            &build_grib2_polar_stereographic_message(),
        ),
    ];

    for path in fixtures {
        assert_matches_reference(&helper, &path);
    }
}

#[test]
#[ignore = "requires GRIB_READER_ECCODES_HELPER"]
fn corpus_samples_match_eccodes_when_configured() {
    let helper = helper_path().unwrap_or_else(|| {
        panic!("GRIB_READER_ECCODES_HELPER must be set to run ecCodes parity tests")
    });

    for path in collect_parity_samples() {
        assert_matches_reference(&helper, &path);
    }
}

fn assert_matches_reference(helper: &Path, path: &Path) {
    let rust = GribFile::open(path)
        .unwrap_or_else(|err| panic!("failed opening {} with Rust decoder: {err}", path.display()));
    if sample_requires_disabled_codec(&rust) {
        return;
    }
    let reference = dump_reference(helper, path);

    assert_eq!(
        rust.message_count(),
        reference.messages.len(),
        "message count mismatch for {}",
        path.display()
    );

    for (index, expected) in reference.messages.iter().enumerate() {
        let message = rust.message(index).unwrap();
        let decoded = message.read_data_as_f64().unwrap();
        let actual = decoded.iter().copied().collect::<Vec<_>>();

        assert_eq!(
            message.edition(),
            expected.edition,
            "edition mismatch for {} field {}",
            path.display(),
            index
        );
        if !has_local_use_parameter(&message) {
            assert_eq!(
                message.parameter_description(),
                expected.name,
                "parameter description mismatch for {} field {}",
                path.display(),
                index
            );
        }
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
                    let diff = (actual - expected).abs();
                    assert!(
                        diff <= 1e-12,
                        "value mismatch for {} field {} value {}: rust={} eccodes={} diff={}",
                        path.display(),
                        index,
                        value_index,
                        actual,
                        expected,
                        diff
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

fn sample_requires_disabled_codec(file: &GribFile) -> bool {
    for index in 0..file.message_count() {
        let message = file.message(index).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::Jpeg2000Packing(_) if !cfg!(feature = "jpeg2000") => return true,
            DataRepresentation::PngPacking(_) if !cfg!(feature = "png") => return true,
            _ => {}
        }
    }
    false
}

fn has_local_use_parameter(message: &grib_reader::Message<'_>) -> bool {
    let Some(product) = message.product_definition() else {
        return false;
    };
    // Local table resolution is intentionally not modeled yet, so ecCodes may
    // know a center-specific name that the generic WMO lookup must not claim.
    is_local_use_code(product.parameter_category) || is_local_use_code(product.parameter_number)
}

fn is_local_use_code(code: u8) -> bool {
    (192..=254).contains(&code)
}
