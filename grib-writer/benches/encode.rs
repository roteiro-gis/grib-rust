#[path = "../tests/common/mod.rs"]
mod common;

use common::{bitmap_values, dense_values, grib2_field, latlon_grid, write_grib2_message};
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use grib_reader::GribFile;

fn writer_encode_benchmarks(c: &mut Criterion) {
    let ni = 144;
    let nj = 73;
    let dense = dense_values(ni, nj);
    let bitmap = bitmap_values(ni, nj);

    let mut group = c.benchmark_group("encode");
    group.bench_function("simple_grib2", |b| {
        b.iter(|| {
            let field = grib2_field(latlon_grid(ni, nj, 0), black_box(&dense), 0, 0, 0);
            let bytes = write_grib2_message([field]);
            black_box(bytes);
        })
    });
    group.bench_function("bitmap_grib2", |b| {
        b.iter(|| {
            let field = grib2_field(latlon_grid(ni, nj, 0), black_box(&bitmap), 0, 0, 0);
            let bytes = write_grib2_message([field]);
            black_box(bytes);
        })
    });
    group.bench_function("multifield_grib2", |b| {
        b.iter(|| {
            let first = grib2_field(latlon_grid(ni, nj, 0), black_box(&dense), 0, 0, 0);
            let second = grib2_field(latlon_grid(ni, nj, 0), black_box(&bitmap), 0, 2, 0);
            let third = grib2_field(latlon_grid(ni, nj, 0), black_box(&dense), 0, 4, 1);
            let bytes = write_grib2_message([first, second, third]);
            black_box(bytes);
        })
    });
    group.bench_function("encode_decode_roundtrip", |b| {
        b.iter_batched(
            || dense.clone(),
            |values| {
                let field = grib2_field(latlon_grid(ni, nj, 0), &values, 0, 0, 0);
                let bytes = write_grib2_message([field]);
                let file = GribFile::from_bytes(bytes).unwrap();
                let decoded = file.read_all_data_as_f64().unwrap();
                black_box(decoded);
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(benches, writer_encode_benchmarks);
criterion_main!(benches);
