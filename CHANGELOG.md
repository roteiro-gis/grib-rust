# Changelog

## 0.7.0 - Unreleased

- replace the reader's constructor matrix with `GribFile::builder()`, including
  memory-mapped file, owned-byte, and arbitrary `Read` inputs
- remove the ambiguous file-level edition accessor; editions are reported per
  logical field
- add explicit north-up decode methods while preserving encoded scan directions
  in the existing reader-order methods
- expose GRIB2 local-use sections and resolve bitmap indicator 254 within
  multi-field messages
- centralize allocation failures and resource-limit checks in `grib-core`,
  mark the public error enum non-exhaustive, and remove duplicate signed-integer
  aliases
- report unsigned 24-bit range violations and invalid local parameter tables
  through dedicated error variants instead of the catch-all string error
- report image-codec failures, malformed image layouts, packing arithmetic,
  and decoded grid-shape failures through structured reader error variants
- report missing writer fields, unsupported edition/packing combinations, and
  GRIB1 representation limits through dedicated error variants
- report GRIB2 section coordinate, scale, count, and length failures through
  typed range, overflow, and data-length errors
- report packing-value, image-codec, grouping, and spatial-differencing
  failures through typed errors and remove the catch-all error variant
- remove the unused reader `flate2` dependency
- replace byte-at-a-time bit reads and bit-at-a-time writes with buffered
  64-bit reads and byte-chunked writes
- decode byte-aligned simple packing directly into dense output slices and
  expand bitmap gaps a byte at a time
- locate GRIB messages with substring search instead of testing every byte
  offset
- decode rotated latitude/longitude and regular Gaussian grids while rejecting
  quasi-regular geometry with a typed error
- preserve GRIB2 generating-process and data-cutoff metadata and decode signed
  forecast offsets without unsigned wraparound
- decode derived-ensemble, probability, and percentile forecasts with typed
  probability thresholds for product templates 4.2, 4.5, and 4.6
- write product templates 4.2, 4.5, and 4.6 and compare their Section 4
  metadata with ecCodes in the parity suite
- decode interval probability, percentile, and derived-ensemble templates 4.9,
  4.10, and 4.12 through one shared statistical-interval model
- write interval product templates 4.9, 4.10, and 4.12 and verify their
  timestamps, time ranges, thresholds, and ensemble metadata against ecCodes
- decode and write spatially processed product template 4.15 and verify its
  process type, spatial method, and source-point count against ecCodes
- decode and write IEEE floating-point template 5.4 at binary32, binary64, and
  binary128 precision, including bitmap handling and ecCodes parity
- decode run-length template 5.200 with scaled level tables, embedded missing
  values, bounded run expansion, and ecCodes parity
- resolve the ECMWF GRIB1 large-message convention from its 120-octet block
  count and Section 4 padding length
- decode standard GRIB1 row-by-row, constant-width, and variable-width
  second-order grid-point packing, including primary and secondary bitmaps
- write and decode GRIB2 complex packing with one group per scan line,
  including bitmap-reduced and empty rows
- replace duplicate simple- and image-packing parameter structs with one
  `ScaledPackingParams` model shared by every scaled representation
- decode CCSDS/AEC data representation template 5.42 behind the `ccsds`
  feature with validated option masks, block sizes, reference sample intervals,
  and one shared safe interface to bundled libaec 1.1.7
- write CCSDS/AEC template 5.42 with automatic bit-width selection, bitmap and
  constant-field handling, configurable codec parameters, and ecCodes parity
  for default, restricted, signed, 32-bit, and nonstandard-block streams
- encode JPEG2000 payloads directly into memory through an isolated OpenJPEG
  interface, remove writer runtime temporary-file I/O, and reject component
  precision above the backend's 25-bit lossless numeric limit
- document `NaN` as the default decode result for embedded complex-packing
  missing values and expose active primary and secondary substitute metadata
- build the reference image from SHA-256-verified ecCodes 2.47.0 and libaec
  1.1.7 source archives, limit native builds to two compile jobs, and run
  uniquely named parity and benchmark containers with two CPUs, four GiB of
  memory, two Cargo build jobs, and isolated native artifacts on non-Linux
  hosts
- compare numeric GRIB2 parameter identities in parity tests so ecCodes label
  wording changes do not masquerade as format incompatibilities
- raise the workspace MSRV to Rust 1.87 for the coordinated breaking release
- test the MSRV, macOS, Windows, all four fuzz targets, and an 85% line-coverage
  floor in CI
- reject known advisories, unapproved licenses and sources, wildcard
  dependencies, and duplicate major versions through committed dependency
  policy
- add projected-grid reader fixtures, assert coordinates and decoded values on
  real interoperability samples, and seed the fuzzers with GRIB1 second-order
  and large-message fields

## 0.6.0 - 2026-06-25

- prepare the workspace crates for the 0.6.0 release line so package
  verification resolves against matching internal crate APIs
- update `memmap2` to 0.9.11 to pick up the fix for RUSTSEC-2026-0186
- add GRIB2 writer support for product definition templates 4.1, 4.8, and
  4.11, matching the reader product-surface support
- add GRIB2 Mercator grid template 3.10 and Albers equal-area grid template
  3.31 reader metadata parsing, scan-order normalization, projected coordinate
  offsets, flat data decode, and writer roundtrip support
- add GRIB1 writer predefined bitmap references

## 0.5.0 - 2026-05-18

- add GRIB2 JPEG2000 template 5.40 and PNG template 5.41 reader decode support behind optional feature flags
- add GRIB2 polar stereographic grid template 3.20 metadata parsing, scan-order normalization, flat data decode, and HRRR Alaska interop coverage
- add center/subcenter/local-table-aware GRIB2 parameter lookup, caller-provided local parameter overlays, and built-in NCEP/NOAA REFC local table resolution
- preserve unresolved local-use parameters as unknown local parameters instead of reporting them as WMO-standard definitions
- tighten ecCodes parity coverage so known local-table entries are compared while unknown local-use parameters remain explicitly masked

## 0.4.0 - 2026-05-09

- add GRIB2 Lambert conformal grid template 3.30 reader metadata parsing, flat data decode, scan-order handling, and real HRRR interop coverage
- add GRIB2 complex packing writer support for template 5.2, including bitmap handling and reader roundtrip coverage
- add GRIB2 spatial differencing writer support for template 5.3, including first-order and second-order differencing
- make grid definitions non-exhaustive so future GRIB grid templates can be added without breaking exhaustive downstream matches
- prepare crate badges and release metadata for the workspace crates

## 0.3.0 - 2026-04-20

- split the project into `grib-core`, `grib-reader`, and `grib-writer` crates with shared binary primitives and IBM floating-point helpers
- add GRIB2 simple-packing writer support, including GRIB scan-order value handling and multi-field messages that reuse grid sections
- add GRIB1 simple-packing writer support with bitmap handling and canonical corpus seeds
- replace reader fixture builders with writer-generated bootstrap corpus fixtures
- add writer parity fuzzing, encode benchmarks, and package-safe publishing checks

## 0.2.0 - 2026-04-15

- refactor GRIB2 Section 4 parsing around typed product definition templates and fail closed on unsupported GRIB2 product definition templates
- add low-allocation `decode_into` APIs for caller-owned `f32` and `f64` buffers
- add edition-aware forecast valid-time helpers, including corrected GRIB1 indicator-10 lead-time decoding
- fix GRIB decimal scaling for reference values and decoded values
- reduce complex-packing decode allocation by streaming group metadata from bit offsets
- make tolerant indexing and ecCodes parity behavior explicit, with expanded corpus fixtures and release checks

## 0.1.3 - 2026-04-01

- add GRIB2 complex packing reader support
- add release coverage for complex packing decode behavior

## 0.1.2 - 2026-03-27

- tighten GRIB decoding behavior
- add reference-time and forecast valid-time helper APIs

## 0.1.1 - 2026-03-24

- add ecCodes reference parity and benchmark coverage with manual reference-check workflows
- add benchmark reporting and README refinements
- prepare release metadata for the `roteiro-gis` repository migration

## 0.1.0 - 2026-03-20

- initial public release
- add pure-Rust GRIB reader core with no C dependencies and no build scripts
- add GRIB1 and GRIB2 message scanning, section parsing, and regular latitude/longitude grid support
- add GRIB2 simple packing and GRIB1 simple packing reader decode support
- add typed metadata access for reference time, parameter identity, grid geometry, and decoded data
- add corpus-driven tests, fuzzing setup, CI workflow, and release checks
