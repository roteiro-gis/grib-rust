# grib-rust

Pure-Rust, read-only GRIB decoder for weather and climate data. No C libraries, no build scripts, and no unsafe beyond `memmap2`.

## Crates

| Crate | Description |
|---|---|
| `grib-reader` | GRIB1/GRIB2 field scanner, section parser, metadata extraction, and data unpacking |

## Usage

```rust
use grib_reader::GribFile;

let file = GribFile::open("gfs.grib2")?;
println!("messages: {}", file.message_count());

for msg in file.messages() {
    println!(
        "  {} {:?} reference={}Z",
        msg.parameter_name(),
        msg.grid_shape(),
        msg.reference_time().hour
    );
}

// Read a single field as an f64 ndarray
let data = file.message(0)?.read_data_as_f64()?;
println!("shape: {:?}", data.shape());

// Tolerant scan mode for byte streams containing junk or malformed candidates.
let tolerant = GribFile::from_bytes_with_options(
    std::fs::read("mixed.bin")?,
    grib_reader::OpenOptions { strict: false },
)?;
println!("recoverable messages: {}", tolerant.message_count());
```

## Features

- GRIB1 and GRIB2 message scanning with `"GRIB"`/`"7777"` boundary detection
- Logical field indexing for multi-field GRIB2 messages
- Section parsing for edition-specific metadata, grid, bitmap, and packed data sections
- Regular latitude/longitude grids for GRIB1 and GRIB2
- Simple packing for GRIB1 and GRIB2
- WMO parameter table lookups (Code Table 4.2)
- Typed metadata access for reference time, parameter identity, product metadata, and grid geometry
- `OpenOptions` for strict or tolerant scanning
- Bitmap application with missing values surfaced as `NaN`
- Parallel field decoding via Rayon
- Output: `ndarray::ArrayD<f64>`
- Memory-mapped I/O or owned byte buffers (WASM-compatible)
- Integration tests, corruption tests, and a `cargo-fuzz` target scaffold

## Current Scope

- Supported end-to-end today: GRIB1 and GRIB2 regular lat/lon grids with simple packing
- Unsupported today: non-lat/lon grid templates, complex packing, JPEG2000, PNG, GRIB1 predefined bitmaps, and more advanced multi-packing templates
- Unsupported cases fail explicitly with typed errors; the crate does not silently fabricate metadata or values

## Feature flags

| Flag | Default | Description |
|---|---|---|
| `rayon` | yes | Parallel field decoding |

## Testing

```sh
cargo test
```

## License

MIT OR Apache-2.0
