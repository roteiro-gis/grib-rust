# Benchmark Report

Date: 2026-07-23

This report records the current checksum-guarded comparison between
`grib-rust` 0.7.0 and ecCodes, plus the focused measurements used to retain the
Phase 3 reader and writer optimizations.

## System Under Test

- Host: Apple M1, 8 logical CPUs, 16 GiB memory
- Host OS: macOS 26.5.1 (`arm64`)
- Container target: `aarch64-unknown-linux-gnu`
- Rust toolchain: Rust 1.91.0
- Reference libraries: ecCodes 2.47.0 and libaec 1.1.7
- Container limits: 2 CPUs, 4 GiB memory, 2 Cargo build jobs

The reference image builds SHA-256-verified source archives. The uniquely
named benchmark container is removed after the run, and its Linux-native build
artifacts remain container-local on this host.

## Method

The current measurements were generated with:

```sh
./scripts/run-reference-parity.sh
./scripts/run-reference-benchmarks.sh
```

The aggregate decode comparison covers the eight checked-in parity files:
five generated bootstrap fixtures and three real interoperability samples.
Before Criterion records a timing, the harness requires the Rust and ecCodes
implementations to agree on iteration count, logical-message count,
decoded-value count, and finite-value checksum.
The corresponding parity run passed both reader tests and the writer test.

Criterion used its default 3-second warm-up, 100 samples, and 5-second target
measurement time. Slow aggregate operations automatically received a longer
measurement window so Criterion could collect all samples.

## Current Results

### Aggregate parity-corpus decode

| implementation | Criterion interval |
| --- | ---: |
| `grib-rust` | 113.29–113.64 ms |
| ecCodes 2.47.0 | 196.81–197.24 ms |

Using the reported means, `grib-rust` was 1.74x faster on this exact corpus and
host. This is a corpus-specific result, not a claim about every GRIB packing
template or machine.

### Dense simple-packing decode

This focused benchmark decodes 262,144 byte-aligned 8-bit packed values and
allocates the returned `Vec<f64>`.

| workload | Criterion interval |
| --- | ---: |
| `decode/simple-packed-u8` | 122.50–122.78 µs |

### Writer workloads

The writer benchmarks use a 144 by 73 regular latitude/longitude grid.

| workload | Criterion interval |
| --- | ---: |
| simple GRIB2 field | 193.96–194.65 µs |
| bitmap GRIB2 field | 217.85–218.27 µs |
| three-field GRIB2 message | 579.97–580.76 µs |
| simple encode/decode round trip | 303.30–304.36 µs |

## Phase 3 Before/After Evidence

These focused before/after measurements were recorded on the same Apple M1
host on 2026-07-12. They establish that the hot-path changes materially
improved throughput; their absolute times should not be compared directly with
the capped Docker run above.

### Byte-aligned simple decode

| implementation | Criterion interval |
| --- | ---: |
| buffered extraction through the bitmap-aware cursor | 1.752–1.758 ms |
| byte-aligned dense decode directly into the output slice | 117.3–118.0 µs |

Criterion reported a 93.3% improvement (`p < 0.05`). On the then-current full
corpus, the same change reduced aggregate decode from 92.75–94.05 ms to
90.87–91.24 ms, a 2.4% mean improvement (`p < 0.05`). Image codec time
dominated much of that aggregate workload and was unaffected by the change.

### Packed-bit writer loop

| implementation | mean | Criterion interval |
| --- | ---: | ---: |
| bit-at-a-time writer | 197.99 µs | 187.24–220.20 µs |
| byte-chunked writer | 159.84 µs | 151.61–172.37 µs |

Criterion reported a 19.1% mean improvement (`p < 0.05`) for simple GRIB2
encoding.

## Limits

- Results reflect one host and the committed sample corpus.
- Docker constrains resources and reproduces the reference libraries, but host
  CPU and storage still affect timings.
- Corpus additions, compiler changes, or codec updates require a new run before
  these numbers are treated as current.
