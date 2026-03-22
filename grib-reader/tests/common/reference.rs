#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use grib_reader::GribFile;
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

#[derive(Debug, Deserialize)]
pub struct ReferenceBenchmark {
    pub iterations: usize,
    pub elapsed_ns: u64,
    pub messages: usize,
    pub values: usize,
    pub checksum: f64,
}

#[derive(Debug)]
pub struct RustBenchmark {
    pub iterations: usize,
    pub elapsed: Duration,
    pub messages: usize,
    pub values: usize,
    pub checksum: f64,
}

pub fn helper_path() -> Option<PathBuf> {
    let path = env::var_os("GRIB_READER_ECCODES_HELPER")?;
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

pub fn benchmark_reference(
    helper: &Path,
    paths: &[PathBuf],
    iterations: usize,
) -> ReferenceBenchmark {
    let mut command = Command::new(helper);
    command.arg("benchmark").arg(iterations.to_string());
    for path in paths {
        command.arg(path);
    }

    let output = command
        .output()
        .unwrap_or_else(|err| panic!("failed to run {}: {err}", helper.display()));
    assert!(
        output.status.success(),
        "reference benchmark failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("failed parsing reference benchmark output: {err}"))
}

pub fn benchmark_rust(paths: &[PathBuf], iterations: usize) -> RustBenchmark {
    let start = Instant::now();
    let mut messages = 0usize;
    let mut values = 0usize;
    let mut checksum = 0.0f64;

    for _ in 0..iterations {
        for path in paths {
            let file = GribFile::open(path)
                .unwrap_or_else(|err| panic!("failed opening {}: {err}", path.display()));
            messages += file.message_count();
            for message in file.messages() {
                let data = message
                    .read_data_as_f64()
                    .unwrap_or_else(|err| panic!("failed decoding {}: {err}", path.display()));
                values += data.len();
                checksum += finite_sum(data.iter().copied());
            }
        }
    }

    RustBenchmark {
        iterations,
        elapsed: start.elapsed(),
        messages,
        values,
        checksum,
    }
}

pub fn collect_parity_samples() -> Vec<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    files.extend(collect_sample_files(
        &manifest_dir.join("tests/corpus/bootstrap"),
    ));
    files.extend(collect_sample_files(
        &manifest_dir.join("tests/corpus/interop/samples"),
    ));
    files
}

pub fn write_fixture(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes)
        .unwrap_or_else(|err| panic!("failed writing {}: {err}", path.display()));
    path
}

pub fn finite_sum<I>(values: I) -> f64
where
    I: IntoIterator<Item = f64>,
{
    values
        .into_iter()
        .filter(|value| !value.is_nan())
        .sum::<f64>()
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
