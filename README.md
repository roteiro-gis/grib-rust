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
}

let data = file.message(0)?.read_data_as_f64()?;
println!("shape: {:?}", data.shape());

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
- WMO parameter table lookups (Code Table 4.2)
- Typed metadata access for reference time, parameter identity, product metadata, grid geometry, and lat/lon coordinates
- `OpenOptions` for strict or tolerant scanning
- Bitmap application with missing values surfaced as `NaN`
- Parallel field decoding via Rayon
- Output: `ndarray::ArrayD<f64>`
- Memory-mapped I/O or owned byte buffers

## Not Yet Supported

- Non-lat/lon grid templates
- Complex packing and more advanced multi-packing templates
- JPEG2000 and PNG-packed GRIB2 fields
- GRIB1 predefined bitmaps

Unsupported cases fail explicitly with typed errors.

## Feature flags

| Flag | Default | Description |
|---|---|---|
| `rayon` | yes | Parallel field decoding |

## Testing

```sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## License

MIT OR Apache-2.0
