mod common;

use common::{assert_matches_reference, helper_path, writer_reference_samples};

#[test]
#[ignore = "requires GRIB_READER_ECCODES_HELPER"]
fn writer_generated_fixtures_match_eccodes_when_configured() {
    let helper = helper_path().unwrap_or_else(|| {
        panic!("GRIB_READER_ECCODES_HELPER must be set to run ecCodes parity tests")
    });
    let dir = tempfile::tempdir().unwrap();

    for (name, bytes) in writer_reference_samples() {
        let path = dir.path().join(name);
        std::fs::write(&path, &bytes)
            .unwrap_or_else(|err| panic!("failed writing {}: {err}", path.display()));
        assert_matches_reference(&helper, &path, &bytes);
    }
}
