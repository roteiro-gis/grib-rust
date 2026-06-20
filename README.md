# grib-rust

[![grib-core crates.io](https://img.shields.io/crates/v/grib-core.svg)](https://crates.io/crates/grib-core)
[![grib-core docs.rs](https://docs.rs/grib-core/badge.svg)](https://docs.rs/grib-core)
[![grib-reader crates.io](https://img.shields.io/crates/v/grib-reader.svg)](https://crates.io/crates/grib-reader)
[![grib-reader docs.rs](https://docs.rs/grib-reader/badge.svg)](https://docs.rs/grib-reader)
[![grib-writer crates.io](https://img.shields.io/crates/v/grib-writer.svg)](https://crates.io/crates/grib-writer)
[![grib-writer docs.rs](https://docs.rs/grib-writer/badge.svg)](https://docs.rs/grib-writer)

Rust GRIB reader, writer, and shared core primitives for weather and climate data. The default build has no C libraries, no build scripts, and no unsafe in crate code beyond `memmap2`; optional GRIB2 image-packing codecs are behind feature flags.

## Crates

| Crate | Description |
|---|---|
| `grib-core` | Shared GRIB data model, code tables, binary primitives, bit I/O, and validation helpers |
| `grib-reader` | GRIB1/GRIB2 file opening, message scanning, metadata parsing, and packed data decoding |
| `grib-writer` | GRIB1/GRIB2 field builders, simple/complex packing, bitmap handling, and message serialization |

## Reader Usage

```rust
use grib_reader::GribFile;

let file = GribFile::open("gfs.grib2")?;
println!("messages: {}", file.message_count());

for msg in file.messages() {
    println!(
        "  {} {:?} {:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        msg.parameter_name(),
        msg.grid_shape(),
        msg.reference_time().year,
        msg.reference_time().month,
        msg.reference_time().day,
        msg.reference_time().hour,
        msg.reference_time().minute,
        msg.reference_time().second,
    );

    if let Some(valid) = msg.valid_time() {
        println!(
            "  valid {:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            valid.year,
            valid.month,
            valid.day,
            valid.hour,
            valid.minute,
            valid.second,
        );
    }
}

let field = file.message(0)?;
let flat = field.read_flat_data_as_f32()?;
println!("decoded values: {}", flat.len());

let mut reused = vec![0.0_f64; field.grid_shape().0 * field.grid_shape().1];
field.decode_into(&mut reused)?;

let data = field.read_data_as_f64()?;
println!("ndarray shape: {:?}", data.shape());

let tolerant = GribFile::from_bytes_with_options(
    std::fs::read("mixed.bin")?,
    grib_reader::OpenOptions {
        strict: false,
        ..grib_reader::OpenOptions::default()
    },
)?;
println!("recoverable messages: {}", tolerant.message_count());
```

`OpenOptions` also bounds decoded-field and coordinate-axis allocations by
default. Tune `max_decoded_points` and `max_axis_points`, or use the
`without_*_limit` helpers, when intentionally reading unusually large grids.

Custom GRIB2 local parameter tables can be authored as CSV and supplied as an
overlay. The reader checks WMO Code Table 4.2 first for standard parameters,
then checks caller entries and the built-in local registry for local-use
category or parameter numbers.

```rust
use grib_reader::{GribFile, LocalParameterTable, OpenOptions};

let table = LocalParameterTable::from_csv_str(
    "center_id,subcenter_id,local_table_version,discipline,category,number,short_name,description\n\
     42,0,1,0,16,196,LREFC,Local composite reflectivity\n",
)?;
let local_parameters = table.entries();

let file = GribFile::from_bytes_with_local_parameters(
    std::fs::read("local-product.grib2")?,
    OpenOptions::default(),
    &local_parameters,
)?;
```

## Writer Usage

```rust
use grib_core::{
    AnalysisOrForecastTemplate, FixedSurface, GridDefinition, Identification, LatLonGrid,
    ProductDefinition, ProductDefinitionTemplate,
};
use grib_writer::{Grib2FieldBuilder, GribWriter, PackingStrategy};

let grid = GridDefinition::LatLon(LatLonGrid {
    ni: 2,
    nj: 2,
    lat_first: 50_000_000,
    lon_first: -120_000_000,
    lat_last: 49_000_000,
    lon_last: -119_000_000,
    di: 1_000_000,
    dj: 1_000_000,
    scanning_mode: 0,
});
let id = Identification {
    center_id: 7,
    subcenter_id: 0,
    master_table_version: 35,
    local_table_version: 1,
    significance_of_reference_time: 1,
    reference_year: 2026,
    reference_month: 3,
    reference_day: 20,
    reference_hour: 12,
    reference_minute: 0,
    reference_second: 0,
    production_status: 0,
    processed_data_type: 1,
};
let product = ProductDefinition {
    parameter_category: 0,
    parameter_number: 0,
    template: ProductDefinitionTemplate::AnalysisOrForecast(AnalysisOrForecastTemplate {
        generating_process: 2,
        forecast_time_unit: 1,
        forecast_time: 6,
        first_surface: Some(FixedSurface {
            surface_type: 103,
            scale_factor: 0,
            scaled_value: 850,
        }),
        second_surface: None,
    }),
};

let values = [1.0, 2.0, f64::NAN, 4.0];
let field = Grib2FieldBuilder::new()
    .identification(id)
    .grid(grid)
    .product(product)
    .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
    .values(&values)
    .build()?;

let mut bytes = Vec::new();
GribWriter::new(&mut bytes).write_grib2_message([field])?;
```

## Supported Now

- GRIB1 and GRIB2 message scanning with `"GRIB"`/`"7777"` boundary detection
- Logical field indexing for multi-field GRIB2 messages
- Regular latitude/longitude grids for GRIB1 and GRIB2
- Reader GRIB2 Mercator grid template 3.10, polar stereographic grid template
  3.20, Lambert conformal grid template 3.30, and Albers equal-area grid
  template 3.31 metadata, projected coordinate offsets, and flat data decode
- Reader simple packing for GRIB1 and GRIB2
- Reader GRIB1 predefined bitmaps via caller-supplied center-defined bitmap tables
- GRIB2 complex packing with general group splitting, including spatial differencing
- Feature-gated reader GRIB2 JPEG2000 template 5.40 and PNG template 5.41 packed data decode
- WMO parameter table lookups (Code Table 4.2) plus center/subcenter/local-table-aware GRIB2 local parameter entries and CSV authoring helpers
- Typed metadata access for reference time, parameter identity, product metadata, grid geometry, and lat/lon coordinates
- Forecast valid-time helpers for supported fixed-width GRIB1/GRIB2 time units
- `OpenOptions` for strict or tolerant scanning
- Bitmap application with missing values surfaced as `NaN`
- Parallel field decoding via Rayon
- Output: caller-owned `&mut [f32]`/`&mut [f64]`, flat `Vec<f32>`/`Vec<f64>`, or `ndarray::ArrayD<f32>`/`ArrayD<f64>`
- Memory-mapped I/O or owned byte buffers
- Writer GRIB2 regular lat/lon fields with product template 4.0, simple packing template 5.0, complex packing template 5.2, and spatial differencing template 5.3
- Writer GRIB2 Mercator grid template 3.10, polar stereographic grid template
  3.20, Lambert conformal grid template 3.30, and Albers equal-area grid
  template 3.31 fields
- Feature-gated writer GRIB2 JPEG2000 template 5.40 and PNG template 5.41 packed data encode
- Writer GRIB2 bitmap section generation from explicit masks or `NaN` values
- Writer single-message multi-field GRIB2 output with reused grid sections
- Writer GRIB1 regular lat/lon fields with simple packing and optional explicit
  or predefined bitmap section

## Not Yet Supported

- Remaining GRIB2 grid templates beyond 3.0, 3.10, 3.20, 3.30, and 3.31
- Writer GRIB2 row-by-row complex packing

Unsupported cases fail explicitly with typed errors.
Calendar-dependent forecast units such as months and years are exposed through
raw metadata but currently return `None` from `valid_time()`.

## API Compatibility

The workspace is pre-1.0. `GridDefinition` is intentionally `#[non_exhaustive]`
because GRIB grid templates are open-ended: WMO can add templates and producers
can use center-specific local templates. Downstream code should prefer
`GridDefinition` query helpers for common behavior or include a wildcard arm
when matching specific grid families.

## Feature flags

| Flag | Default | Description |
|---|---|---|
| `rayon` | yes | Parallel field decoding |
| `jpeg2000` | no | GRIB2 template 5.40 JPEG2000 packed-data decode in `grib-reader` and encode in `grib-writer` |
| `png` | no | GRIB2 template 5.41 PNG packed-data decode in `grib-reader` and encode in `grib-writer` |
| `codecs` | no | Enables both `jpeg2000` and `png` |

## Testing

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo run -p grib-reader --example sync_corpus
git diff --exit-code
cargo test --workspace --all-features
cargo test -p grib-reader --no-default-features
cargo check --manifest-path grib-reader/fuzz/Cargo.toml --bins
cargo clippy --manifest-path grib-reader/fuzz/Cargo.toml --bins -- -D warnings
cargo package --workspace --locked
```

Reference compatibility checks are intentionally outside default PR CI:

```sh
./scripts/run-reference-parity.sh
```

## Release Checklist

```sh
git switch main
git pull --ff-only
git merge <release-branch>

cargo package --workspace --locked
cargo publish -p grib-core --dry-run --locked
cargo publish -p grib-core --locked
cargo publish -p grib-reader --dry-run --locked
cargo publish -p grib-reader --locked
cargo publish -p grib-writer --dry-run --locked
cargo publish -p grib-writer --locked

git tag -a v<version> -m "v<version>"
git push origin main
git push origin v<version>
```

## Corpus And Fuzzing

- Bootstrap corpus samples live in `grib-reader/tests/corpus/bootstrap/`
- Real interoperability samples belong in `grib-reader/tests/corpus/interop/samples/`
- Regenerate the bootstrap and fuzz seed corpora with `cargo run -p grib-reader --example sync_corpus`
- Fuzzer entry points and usage notes live in `grib-reader/fuzz/README.md`

## Reference Checks

- `./scripts/run-reference-parity.sh` runs the Dockerized ecCodes parity suite.
- `grib-writer` has a versioned dev-dependency on `grib-reader` for local
  validation tests and benchmarks, so dry-run and publish it after
  `grib-reader` v0.5.0 is visible in the crates.io index.
- For reference comparisons and current benchmark results against ecCodes, see
  [docs/benchmark-report.md](docs/benchmark-report.md). Re-run the benchmark
  scripts after corpus changes before using those numbers as current throughput
  claims.

## License

MIT OR Apache-2.0
