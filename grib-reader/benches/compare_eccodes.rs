#[path = "../tests/common/mod.rs"]
mod common;

use std::time::Duration;

use common::{
    benchmark_reference, benchmark_rust, collect_parity_samples, helper_path, ReferenceBenchmark,
    RustBenchmark,
};
use criterion::{criterion_group, criterion_main, Criterion};

fn compare_against_eccodes(c: &mut Criterion) {
    let files = collect_parity_samples();
    assert!(
        !files.is_empty(),
        "benchmark corpus is empty; expected bootstrap fixtures at minimum"
    );

    let mut group = c.benchmark_group("decode");
    group.bench_function("grib-rust", |b| {
        b.iter_custom(|iters| benchmark_rust(&files, iters as usize).elapsed)
    });
    if let Some(helper) = helper_path() {
        let rust_validation = benchmark_rust(&files, 1);
        let reference_validation = benchmark_reference(&helper, &files, 1);
        assert_benchmark_coverage(&rust_validation, &reference_validation);

        group.bench_function("eccodes", |b| {
            b.iter_custom(|iters| {
                let reference = benchmark_reference(&helper, &files, iters as usize);
                duration_from_nanos(reference.elapsed_ns)
            })
        });
    } else {
        eprintln!("ecCodes benchmark disabled; GRIB_READER_ECCODES_HELPER is not set");
    }
    group.finish();
}

fn assert_benchmark_coverage(rust: &RustBenchmark, reference: &ReferenceBenchmark) {
    assert_eq!(rust.iterations, reference.iterations, "iteration mismatch");
    assert_eq!(rust.messages, reference.messages, "message count mismatch");
    assert_eq!(
        rust.values, reference.values,
        "decoded value count mismatch"
    );

    let checksum_delta = (rust.checksum - reference.checksum).abs();
    let tolerance = rust.checksum.abs().max(reference.checksum.abs()).max(1.0) * 1e-12;
    assert!(
        checksum_delta <= tolerance,
        "checksum mismatch: rust={} eccodes={} delta={} tolerance={}",
        rust.checksum,
        reference.checksum,
        checksum_delta,
        tolerance
    );
}

fn duration_from_nanos(nanos: u64) -> Duration {
    Duration::from_nanos(nanos)
}

criterion_group!(benches, compare_against_eccodes);
criterion_main!(benches);
