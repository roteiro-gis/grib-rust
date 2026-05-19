# Changelog

## 0.5.0

- Add GRIB2 JPEG2000 template 5.40 and PNG template 5.41 reader decode support behind optional feature flags.
- Add GRIB2 polar stereographic grid template 3.20 metadata parsing, scan-order normalization, flat data decode, and HRRR Alaska interop coverage.
- Add center/subcenter/local-table-aware GRIB2 parameter lookup, caller-provided local parameter overlays, and built-in NCEP/NOAA REFC local table resolution.
- Preserve unresolved local-use parameters as unknown local parameters instead of reporting them as WMO-standard definitions.
- Tighten ecCodes parity coverage so known local-table entries are compared while unknown local-use parameters remain explicitly masked.

## 0.4.0

- Add GRIB2 Lambert conformal grid template 3.30 reader metadata parsing, flat data decode, scan-order handling, and real HRRR interop coverage.
- Add GRIB2 complex packing writer support for template 5.2, including bitmap handling and reader roundtrip coverage.
- Add GRIB2 spatial differencing writer support for template 5.3, including first-order and second-order differencing.
- Make grid definitions non-exhaustive so future GRIB grid templates can be added without breaking exhaustive downstream matches.
- Prepare crate badges and release metadata for the workspace crates.

## 0.3.0

- Split the project into `grib-core`, `grib-reader`, and `grib-writer` crates with shared binary primitives and IBM floating-point helpers.
- Add GRIB2 simple-packing writer support, including GRIB scan-order value handling and multi-field messages that reuse grid sections.
- Add GRIB1 simple-packing writer support with bitmap handling and canonical corpus seeds.
- Replace reader fixture builders with writer-generated bootstrap corpus fixtures.
- Add writer parity fuzzing, encode benchmarks, and package-safe publishing checks.

## 0.2.0

- Refactor GRIB2 Section 4 parsing around typed product definition templates and fail closed on unsupported GRIB2 product definition templates.
- Add low-allocation `decode_into` APIs for caller-owned `f32` and `f64` buffers.
- Add edition-aware forecast valid-time helpers, including corrected GRIB1 indicator-10 lead-time decoding.
- Fix GRIB decimal scaling for reference values and decoded values.
- Reduce complex-packing decode allocation by streaming group metadata from bit offsets.
- Make tolerant indexing and ecCodes parity behavior explicit, with expanded corpus fixtures and release checks.

## 0.1.3

- Add GRIB2 complex packing reader support.
- Add release coverage for complex packing decode behavior.

## 0.1.2

- Tighten GRIB decoding behavior.
- Add reference-time and forecast valid-time helper APIs.

## 0.1.1

- Add ecCodes reference parity and benchmark coverage with manual reference-check workflows.
- Add benchmark reporting and README refinements.
- Prepare release metadata for the `roteiro-gis` repository migration.

## 0.1.0

Initial public release.

Highlights:

- Pure-Rust GRIB reader core with no C dependencies and no build scripts.
- GRIB1 and GRIB2 message scanning, section parsing, and regular latitude/longitude grid support.
- GRIB2 simple packing and GRIB1 simple packing reader decode support.
- Typed metadata access for reference time, parameter identity, grid geometry, and decoded data.
- Corpus-driven tests, fuzzing setup, CI workflow, and release checks.
