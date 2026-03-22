# Benchmark Report

Date: 2026-03-21

This report summarizes the current Dockerized parity and comparison benchmark
suite for `grib-rust` against ecCodes. It captures the current parity status
and the aggregate decode performance of the current benchmark corpus.

## System Under Test

- Machine: Apple M1
- CPU topology: 8 logical CPUs
- Memory: 16 GiB
- OS: macOS 13.0
- Architecture: `arm64`
- Rust toolchain: `rustc 1.92.0`
- Reference environment: Docker image with Rust and `libeccodes-dev`

These measurements reflect this machine. ecCodes ran in Docker, but the
timings still reflect the same host CPU and storage stack.

## Scope

- Dockerized parity tests against ecCodes for:
  - generated GRIB1/GRIB2 fixtures
  - the checked-in GRIB parity sample corpus
- Criterion comparison bench over the full parity sample set collected by
  `collect_parity_samples()`

## Methodology

Commands used for this report:

```sh
./scripts/run-reference-parity.sh
./scripts/run-reference-benchmarks.sh
```

Notes:

- The parity run passed both ecCodes integration tests.
- The current parity corpus contains 4 checked-in GRIB sample files across the
  bootstrap and interoperability sample directories.
- The comparison bench validates message counts, decoded value counts, and
  checksum parity before timing.
- Criterion measures aggregate elapsed time for repeated decode of the full current sample set.

## Current Results

### Parity

- `generated_fixtures_match_eccodes_when_configured`: passed
- `corpus_samples_match_eccodes_when_configured`: passed

### Summary

| workload | grib-rust | ecCodes | result |
| --- | ---: | ---: | --- |
| aggregate corpus decode | 1.62 ms | 4.24 ms | `grib-rust` 2.61x faster |

## Interpretation

- On the current 4-file corpus, `grib-rust` is ahead of ecCodes in aggregate
  decode time on this host.
- The benchmark is checksum-guarded and corpus-driven, so it is a stronger
  signal than a pure microbenchmark with no correctness check.
- Because the corpus is still small, this result is best read as a statement
  about the current control path and sample shape, not every GRIB workload.

## Limits

- This report reflects one machine.
- The benchmark corpus is small and should be expanded if broader throughput
  claims are needed.
- Docker improves reproducibility here, but the results remain host-specific.
