# Changelog

## Unreleased

- prepare the workspace crates for the 0.6.0 release line so package
  verification resolves against matching internal crate APIs
- update `memmap2` to 0.9.11 to pick up the fix for RUSTSEC-2026-0186
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
