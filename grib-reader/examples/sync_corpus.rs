use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[path = "../tests/common/fixtures.rs"]
mod fixtures;

fn main() -> io::Result<()> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let bootstrap_dir = manifest_dir.join("tests/corpus/bootstrap");
    let fuzz_root = manifest_dir.join("fuzz/corpus");
    let open_dir = fuzz_root.join("fuzz_grib_open");
    let tolerant_dir = fuzz_root.join("fuzz_grib_tolerant");
    let sections_dir = fuzz_root.join("fuzz_grib_sections");
    let interop_dir = manifest_dir.join("tests/corpus/interop/samples");

    fs::create_dir_all(&bootstrap_dir)?;
    fs::create_dir_all(&open_dir)?;
    fs::create_dir_all(&tolerant_dir)?;
    fs::create_dir_all(&sections_dir)?;

    let minimal_grib2 = fixtures::build_grib2_message(&[1, 2, 3, 4]);
    let minimal_grib1 = fixtures::build_grib1_message(&[5, 6, 7, 8]);
    let multifield_grib2 = fixtures::build_grib2_multifield_message();
    let bitmap_grib1 = fixtures::build_grib1_bitmap_message();
    let forecast_grib2 = fixtures::build_grib2_message_with_forecast(&[1, 2, 3, 4], 18);
    let complex_grib2 = fixtures::build_grib2_complex_packing_message();
    let complex_missing_grib2 = fixtures::build_grib2_complex_packing_message_with_missing();
    let spatial_grib2 = fixtures::build_grib2_spatial_differencing_message();
    let tolerant_mixed = fixtures::build_bitmap_prefixed_stream();
    let truncated_grib2 = fixtures::build_truncated_grib2_message();

    write_if_changed(&bootstrap_dir.join("minimal.grib2"), &minimal_grib2)?;
    write_if_changed(&bootstrap_dir.join("minimal.grib1"), &minimal_grib1)?;
    write_if_changed(&bootstrap_dir.join("multifield.grib2"), &multifield_grib2)?;
    write_if_changed(&bootstrap_dir.join("bitmap.grib1"), &bitmap_grib1)?;
    write_if_changed(&bootstrap_dir.join("forecast.grib2"), &forecast_grib2)?;
    remove_if_exists(&bootstrap_dir.join("complex.grib2"))?;
    remove_if_exists(&bootstrap_dir.join("complex-missing.grib2"))?;
    remove_if_exists(&bootstrap_dir.join("spatial-differencing.grib2"))?;

    write_if_changed(&open_dir.join("minimal.grib2"), &minimal_grib2)?;
    write_if_changed(&open_dir.join("minimal.grib1"), &minimal_grib1)?;
    write_if_changed(&open_dir.join("multifield.grib2"), &multifield_grib2)?;
    write_if_changed(&open_dir.join("bitmap.grib1"), &bitmap_grib1)?;
    write_if_changed(&open_dir.join("forecast.grib2"), &forecast_grib2)?;
    write_if_changed(&open_dir.join("complex.grib2"), &complex_grib2)?;
    write_if_changed(
        &open_dir.join("complex-missing.grib2"),
        &complex_missing_grib2,
    )?;
    write_if_changed(&open_dir.join("spatial-differencing.grib2"), &spatial_grib2)?;

    write_if_changed(&tolerant_dir.join("minimal.grib2"), &minimal_grib2)?;
    write_if_changed(&tolerant_dir.join("forecast.grib2"), &forecast_grib2)?;
    write_if_changed(&tolerant_dir.join("complex.grib2"), &complex_grib2)?;
    write_if_changed(
        &tolerant_dir.join("complex-missing.grib2"),
        &complex_missing_grib2,
    )?;
    write_if_changed(
        &tolerant_dir.join("spatial-differencing.grib2"),
        &spatial_grib2,
    )?;
    write_if_changed(&tolerant_dir.join("mixed-prefix.bin"), &tolerant_mixed)?;
    write_if_changed(&tolerant_dir.join("truncated.grib2"), &truncated_grib2)?;

    write_if_changed(&sections_dir.join("minimal.grib2"), &minimal_grib2)?;
    write_if_changed(&sections_dir.join("minimal.grib1"), &minimal_grib1)?;
    write_if_changed(&sections_dir.join("multifield.grib2"), &multifield_grib2)?;
    write_if_changed(&sections_dir.join("bitmap.grib1"), &bitmap_grib1)?;
    write_if_changed(&sections_dir.join("forecast.grib2"), &forecast_grib2)?;
    write_if_changed(&sections_dir.join("complex.grib2"), &complex_grib2)?;
    write_if_changed(
        &sections_dir.join("complex-missing.grib2"),
        &complex_missing_grib2,
    )?;
    write_if_changed(
        &sections_dir.join("spatial-differencing.grib2"),
        &spatial_grib2,
    )?;

    let interop_samples = collect_grib_samples(&interop_dir);
    for sample in interop_samples {
        let bytes = fs::read(&sample)?;
        let relative = sample.strip_prefix(&interop_dir).unwrap_or(&sample);
        let file_name = sanitize_relative_path(relative);
        write_if_changed(&open_dir.join(&file_name), &bytes)?;
        write_if_changed(&tolerant_dir.join(&file_name), &bytes)?;
        write_if_changed(&sections_dir.join(&file_name), &bytes)?;
    }

    println!("bootstrap corpus: {}", bootstrap_dir.display());
    println!("fuzz corpus: {}", fuzz_root.display());
    println!(
        "interop samples imported: {}",
        collect_grib_samples(&interop_dir).len()
    );

    Ok(())
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Ok(existing) = fs::read(path) {
        if existing == bytes {
            return Ok(());
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn collect_grib_samples(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_grib_samples_recursive(root, &mut files);
    files.sort();
    files
}

fn collect_grib_samples_recursive(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_grib_samples_recursive(&path, files);
        } else if is_grib_sample(&path) {
            files.push(path);
        }
    }
}

fn is_grib_sample(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("grib" | "grib1" | "grib2" | "grb" | "bin")
    )
}

fn sanitize_relative_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        let text = component.as_os_str().to_string_lossy();
        parts.push(text.replace(['/', '\\', ' '], "_"));
    }
    parts.join("__")
}
