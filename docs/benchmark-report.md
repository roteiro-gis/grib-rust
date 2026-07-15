# Benchmark Report

Date: 2026-03-21

This report summarizes a Dockerized parity and comparison benchmark run for
`grib-rust` against ecCodes. It captures one historical parity run and the
aggregate decode performance of the corpus that existed for that run.

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
- The recorded timing below was taken before the bootstrap corpus expansion and
  reflects a 4-file checked-in GRIB sample set.
- The current checked-in bootstrap corpus now contains 8 generated samples and
  should be re-benchmarked before treating any throughput number here as
  current or representative.
- The comparison bench validates message counts, decoded value counts, and
  checksum parity before timing.
- Criterion measures aggregate elapsed time for repeated decode of the full
  sample set used in the run.

## Historical Results

### Packed-bit writer hot loop (2026-07-12)

The following measurements use the same optimized Criterion command before and
after replacing bit-at-a-time writes with byte-chunked writes:

```sh
cargo bench -p grib-writer --bench encode -- simple_grib2 \
  --sample-size 50 --warm-up-time 1 --measurement-time 2 --noplot
```

| implementation | mean | Criterion interval |
| --- | ---: | ---: |
| byte-by-byte reader and bit-at-a-time writer | 197.99 µs | 187.24–220.20 µs |
| 64-bit buffered reader and byte-chunked writer | 159.84 µs | 151.61–172.37 µs |

Criterion reported a 19.1% mean improvement (`p < 0.05`). This isolates the
simple GRIB2 encode path on this host; it is not a claim about every packing
template or machine.

### Parity

- `generated_fixtures_match_eccodes_when_configured`: passed
- `corpus_samples_match_eccodes_when_configured`: passed

### Historical Summary

| workload | grib-rust | ecCodes | result |
| --- | ---: | ---: | --- |
| aggregate corpus decode | 1.62 ms | 4.24 ms | `grib-rust` 2.61x faster |

## Interpretation

- On the 4-file corpus used in this run, `grib-rust` was ahead of ecCodes in
  aggregate decode time on this host.
- The benchmark is checksum-guarded and corpus-driven, so it is a stronger
  signal than a pure microbenchmark with no correctness check.
- Because that corpus was still small, this result is best read as a statement
  about that sample set and control path, not every GRIB workload.

## Limits

- This report reflects one machine.
- Any current performance claim should be regenerated after corpus changes.
- Docker improves reproducibility here, but the results remain host-specific.
