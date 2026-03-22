mod common;

use std::path::Path;

use common::{
    build_grib1_bitmap_message, build_grib1_message, build_grib2_message,
    build_grib2_multifield_message, collect_parity_samples, dump_reference, helper_path,
    write_fixture,
};
use grib_reader::GribFile;

#[test]
fn generated_fixtures_match_eccodes_when_configured() {
    let Some(helper) = helper_path() else {
        eprintln!("skipping ecCodes parity test; GRIB_READER_ECCODES_HELPER is not set");
        return;
    };

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
    ];

    for path in fixtures {
        assert_matches_reference(&helper, &path);
    }
}

#[test]
fn corpus_samples_match_eccodes_when_configured() {
    let Some(helper) = helper_path() else {
        eprintln!("skipping ecCodes parity test; GRIB_READER_ECCODES_HELPER is not set");
        return;
    };

    for path in collect_parity_samples() {
        assert_matches_reference(&helper, &path);
    }
}

fn assert_matches_reference(helper: &Path, path: &Path) {
    let rust = GribFile::open(path)
        .unwrap_or_else(|err| panic!("failed opening {} with Rust decoder: {err}", path.display()));
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
