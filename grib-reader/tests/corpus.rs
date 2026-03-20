use std::fs;
use std::path::{Path, PathBuf};

use grib_reader::GribFile;

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

fn assert_sample_decodes(path: &Path) {
    let bytes =
        fs::read(path).unwrap_or_else(|err| panic!("failed reading {}: {err}", path.display()));
    let file = GribFile::from_bytes(bytes)
        .unwrap_or_else(|err| panic!("failed opening {}: {err}", path.display()));
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
