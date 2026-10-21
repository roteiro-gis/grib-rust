use std::fs;
use std::path::{Path, PathBuf};

use grib_reader::{DataRepresentation, GribFile, GridDefinition, ParameterTableSource};

#[test]
fn bootstrap_corpus_decodes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/bootstrap");
    let files = collect_sample_files(&root);
    assert!(
        !files.is_empty(),
        "bootstrap corpus is empty; run `cargo run -p grib-reader --example sync_corpus`"
    );

    for path in files {
        assert_sample_decodes(&path);
    }
}

#[test]
fn interop_corpus_decodes_when_present() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/interop/samples");
    let files = collect_sample_files(&root);
    for path in files {
        assert_sample_decodes(&path);
    }
}

#[test]
#[cfg(feature = "jpeg2000")]
fn eccc_jpeg2000_interop_sample_decodes_values_and_axes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/interop/samples/eccc-gem-global-latlon-albedo-jpeg2000.grib2");
    let bytes =
        fs::read(&path).unwrap_or_else(|err| panic!("failed reading {}: {err}", path.display()));
    let file = GribFile::from_bytes(bytes)
        .unwrap_or_else(|err| panic!("failed opening {}: {err}", path.display()));
    let message = file.message(0).unwrap();

    assert!(matches!(
        message.metadata().data_representation,
        DataRepresentation::Jpeg2000Packing(_)
    ));
    assert_eq!(message.grid_shape(), (2400, 1201));
    match message.grid_definition() {
        GridDefinition::LatLon(grid) => {
            assert_eq!(grid.ni, 2400);
            assert_eq!(grid.nj, 1201);
            assert_eq!(grid.lat_first, -90_000_000);
            assert_eq!(grid.lon_first, 180_000_000);
            assert_eq!(grid.lat_last, 90_000_000);
            assert_eq!(grid.lon_last, 179_850_000);
            assert_eq!(grid.di, 150_000);
            assert_eq!(grid.dj, 150_000);
            assert_eq!(grid.scanning_mode, 64);
        }
        other => panic!("expected latitude/longitude grid, got {other:?}"),
    }

    let latitudes = message.latitudes().unwrap().unwrap();
    assert_eq!(latitudes.len(), 1201);
    assert_eq!(latitudes[0], -90.0);
    assert!((latitudes[1] - -89.85).abs() < 1e-12);
    assert_eq!(latitudes[1200], 90.0);

    let longitudes = message.longitudes().unwrap().unwrap();
    assert_eq!(longitudes.len(), 2400);
    assert_eq!(longitudes[0], 180.0);
    assert!((longitudes[1] - 180.15).abs() < 1e-12);
    assert!((longitudes[2399] - 539.85).abs() < 1e-12);

    let values = message.read_flat_data_as_f64().unwrap();
    assert_eq!(values.len(), 2_882_400);
    assert_eq!(
        [
            values[0],
            values[values.len() / 4],
            values[values.len() / 2],
            values[values.len() * 3 / 4],
            values[values.len() - 1],
        ],
        [82.0, 0.0, 0.0, 30.0, 82.0]
    );
}

#[test]
fn hrrr_lambert_interop_sample_has_expected_grid() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/interop/samples/noaa-hrrr-conus-lambert-refc.grib2");
    let bytes =
        fs::read(&path).unwrap_or_else(|err| panic!("failed reading {}: {err}", path.display()));
    let file = GribFile::from_bytes(bytes)
        .unwrap_or_else(|err| panic!("failed opening {}: {err}", path.display()));
    let message = file.message(0).unwrap();

    assert_eq!(message.parameter_name(), "REFC");
    assert_eq!(
        message.parameter_description(),
        "Maximum/Composite radar reflectivity"
    );
    assert_eq!(
        message.parameter().source,
        ParameterTableSource::Local {
            center_id: 7,
            subcenter_id: 0,
            local_table_version: 1,
        }
    );
    assert_eq!(message.grid_shape(), (1799, 1059));
    match message.grid_definition() {
        GridDefinition::LambertConformal(grid) => {
            assert_eq!(grid.core.number_of_points, 1_905_141);
            assert_eq!(grid.core.nx, 1799);
            assert_eq!(grid.core.ny, 1059);
            assert_eq!(grid.core.scanning_mode, 64);
            assert_eq!(grid.core.dx, 3_000_000);
            assert_eq!(grid.core.dy, 3_000_000);
        }
        other => panic!("expected Lambert conformal grid, got {other:?}"),
    }
    assert_eq!(message.latitudes().unwrap(), None);
    assert_eq!(message.longitudes().unwrap(), None);
    let x = message.projected_x_coordinates().unwrap().unwrap();
    assert_eq!(x.len(), 1799);
    assert_eq!([x[0], x[1], x[1798]], [0.0, 3_000.0, 5_394_000.0]);
    let y = message.projected_y_coordinates().unwrap().unwrap();
    assert_eq!(y.len(), 1059);
    assert_eq!([y[0], y[1], y[1058]], [0.0, 3_000.0, 3_174_000.0]);

    assert_eq!(
        message.read_flat_data_as_f64().unwrap().len(),
        message.grid_definition().num_points()
    );
}

#[test]
fn hrrr_alaska_polar_interop_sample_has_expected_grid() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/interop/samples/noaa-hrrr-alaska-polar-refc.grib2");
    let bytes =
        fs::read(&path).unwrap_or_else(|err| panic!("failed reading {}: {err}", path.display()));
    let file = GribFile::from_bytes(bytes)
        .unwrap_or_else(|err| panic!("failed opening {}: {err}", path.display()));
    let message = file.message(0).unwrap();

    assert_eq!(message.parameter_name(), "REFC");
    assert_eq!(
        message.parameter_description(),
        "Maximum/Composite radar reflectivity"
    );
    assert_eq!(
        message.parameter().source,
        ParameterTableSource::Local {
            center_id: 7,
            subcenter_id: 0,
            local_table_version: 1,
        }
    );
    assert_eq!(message.grid_shape(), (1299, 919));
    match message.grid_definition() {
        GridDefinition::PolarStereographic(grid) => {
            assert_eq!(grid.core.number_of_points, 1_193_781);
            assert_eq!(grid.core.nx, 1299);
            assert_eq!(grid.core.ny, 919);
            assert_eq!(grid.core.scanning_mode, 64);
            assert_eq!(grid.core.lat_first, 41_612_949);
            assert_eq!(grid.core.lon_first, 185_117_126);
            assert_eq!(grid.lat_d, 60_000_000);
            assert_eq!(grid.lon_v, 225_000_000);
            assert_eq!(grid.core.dx, 3_000_000);
            assert_eq!(grid.core.dy, 3_000_000);
        }
        other => panic!("expected polar stereographic grid, got {other:?}"),
    }
    assert_eq!(message.latitudes().unwrap(), None);
    assert_eq!(message.longitudes().unwrap(), None);
    let x = message.projected_x_coordinates().unwrap().unwrap();
    assert_eq!(x.len(), 1299);
    assert_eq!([x[0], x[1], x[1298]], [0.0, 3_000.0, 3_894_000.0]);
    let y = message.projected_y_coordinates().unwrap().unwrap();
    assert_eq!(y.len(), 919);
    assert_eq!([y[0], y[1], y[918]], [0.0, 3_000.0, 2_754_000.0]);

    assert_eq!(
        message.read_flat_data_as_f64().unwrap().len(),
        message.grid_definition().num_points()
    );
}

fn assert_sample_decodes(path: &Path) {
    let bytes =
        fs::read(path).unwrap_or_else(|err| panic!("failed reading {}: {err}", path.display()));
    let file = GribFile::from_bytes(bytes)
        .unwrap_or_else(|err| panic!("failed opening {}: {err}", path.display()));
    if sample_requires_disabled_codec(&file) {
        return;
    }
    assert!(
        file.message_count() > 0,
        "sample {} produced zero logical fields",
        path.display()
    );
    file.read_all_data_as_f64()
        .unwrap_or_else(|err| panic!("failed decoding {}: {err}", path.display()));
}

fn collect_sample_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_sample_files_recursive(root, &mut files);
    files.sort();
    files
}

fn collect_sample_files_recursive(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sample_files_recursive(&path, files);
        } else if is_grib_sample(&path) {
            files.push(path);
        }
    }
}

fn is_grib_sample(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("grib" | "grib1" | "grib2" | "grb" | "bin")
    )
}

fn sample_requires_disabled_codec(file: &GribFile) -> bool {
    for index in 0..file.message_count() {
        let message = file.message(index).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::Jpeg2000Packing(_) if !cfg!(feature = "jpeg2000") => return true,
            DataRepresentation::PngPacking(_) if !cfg!(feature = "png") => return true,
            DataRepresentation::CcsdsPacking(_) if !cfg!(feature = "ccsds") => return true,
            _ => {}
        }
    }
    false
}
