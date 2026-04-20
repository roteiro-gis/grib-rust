# Fuzzing

Seed corpora live in `corpus/` and are generated from the crate's canonical fixtures plus any real-world interoperability samples under `../tests/corpus/interop/samples/`.

Regenerate the corpus:

```sh
cargo run -p grib-reader --example sync_corpus
```

Run fuzzers:

```sh
cargo fuzz run fuzz_grib_open
cargo fuzz run fuzz_grib_tolerant
cargo fuzz run fuzz_grib_sections
cargo fuzz run fuzz_grib_writer_inputs
```

Targets:

- `fuzz_grib_open`: strict end-to-end open and decode
- `fuzz_grib_tolerant`: tolerant scanning across malformed prefixes and mixed streams
- `fuzz_grib_sections`: direct section-indexing pressure on GRIB1 and GRIB2 parser entry points
- `fuzz_grib_writer_inputs`: generated writer inputs must either emit readable GRIB or return typed writer errors
