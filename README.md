# grib-rust

Pure-Rust, read-only GRIB decoder for weather and climate data. No C libraries, no build scripts, and no unsafe beyond `memmap2`.

## Crates

| Crate | Description |
|---|---|
| `grib-reader` | GRIB2 field scanner, section parser, metadata extraction, and data unpacking |

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
        msg.identification().reference_hour
    );
}

// Read a single field as an f64 ndarray
let data = file.message(0)?.read_data_as_f64()?;
println!("shape: {:?}", data.shape());
```

## Features

- GRIB2 message scanning with `"GRIB"`/`"7777"` boundary detection
- Logical field indexing for multi-field GRIB2 messages
- Section parsing for Indicator, Identification, Grid Definition, Product Definition, Data Representation, Bitmap, and Data sections
- Grid template 3.0: regular latitude/longitude
- Data representation template 5.0: simple packing
- WMO parameter table lookups (Code Table 4.2)
- Typed metadata access for reference time, product definition, and grid geometry
- Bitmap application with missing values surfaced as `NaN`
- Parallel field decoding via Rayon
- Output: `ndarray::ArrayD<f64>`
- Memory-mapped I/O or owned byte buffers (WASM-compatible)

## Current Scope

- Supported end-to-end today: GRIB2 regular lat/lon grids with simple packing
- Unsupported today: GRIB1 field decoding, non-lat/lon grid templates, complex packing, JPEG2000, PNG, and bitmap reuse indicators other than `0`/`255`
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
