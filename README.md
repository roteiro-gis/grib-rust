# grib-rust

Pure-Rust, read-only GRIB decoder for weather and climate data. No C libraries, no build scripts, and no unsafe beyond `memmap2`.

## Crates

| Crate | Description |
|---|---|
| `grib-reader` | GRIB1/GRIB2 field scanning, metadata parsing, and packed data decoding |

## Usage

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
let flat = field.read_flat_data_as_f64()?;
println!("decoded values: {}", flat.len());

let data = field.read_data_as_f64()?;
println!("ndarray shape: {:?}", data.shape());

let tolerant = GribFile::from_bytes_with_options(
    std::fs::read("mixed.bin")?,
    grib_reader::OpenOptions { strict: false },
)?;
println!("recoverable messages: {}", tolerant.message_count());
```

## Supported Now

- GRIB1 and GRIB2 message scanning with `"GRIB"`/`"7777"` boundary detection
- Logical field indexing for multi-field GRIB2 messages
- Regular latitude/longitude grids for GRIB1 and GRIB2
- Simple packing for GRIB1 and GRIB2
- GRIB2 complex packing with general group splitting, including spatial differencing
- WMO parameter table lookups (Code Table 4.2)
- Typed metadata access for reference time, parameter identity, product metadata, grid geometry, and lat/lon coordinates
- Forecast valid-time helpers from reference time + lead time
- `OpenOptions` for strict or tolerant scanning
- Bitmap application with missing values surfaced as `NaN`
- Parallel field decoding via Rayon
- Output: flat `Vec<f64>` or `ndarray::ArrayD<f64>`
- Memory-mapped I/O or owned byte buffers

## Not Yet Supported

- Non-lat/lon grid templates
- Row-by-row complex packing and more advanced multi-packing templates
- JPEG2000 and PNG-packed GRIB2 fields
- GRIB1 predefined bitmaps

Unsupported cases fail explicitly with typed errors.

## Feature flags

| Flag | Default | Description |
|---|---|---|
| `rayon` | yes | Parallel field decoding |

## Testing

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo run -p grib-reader --example sync_corpus
git diff --exit-code
cargo test --all-features
cargo test --no-default-features
cargo check --manifest-path grib-reader/fuzz/Cargo.toml --bins
cargo clippy --manifest-path grib-reader/fuzz/Cargo.toml --bins -- -D warnings
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

cargo package -p grib-reader
cargo publish -p grib-reader

git tag v<version>
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
- For reference comparisons and current benchmark results against ecCodes, see
  [docs/benchmark-report.md](docs/benchmark-report.md).

## License

MIT OR Apache-2.0
