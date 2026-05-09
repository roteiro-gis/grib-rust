Bootstrap samples in `bootstrap/` are deterministic fixtures generated from the crate's canonical test builders.

Real interoperability samples belong in `interop/samples/` and should satisfy these rules:

- commit only redistributable files with clear provenance
- prefer small representative slices over large operational dumps
- include producer diversity such as NCEP, ECMWF, DWD, JMA, UKMO, and NOAA products when available
- keep filenames descriptive, for example `ncep-gfs-surface-tmp.grib2`
- ensure every committed sample opens and decodes through `cargo test`

Regenerate the bootstrap corpus and fuzz seed corpus with:

```sh
cargo run -p grib-reader --example sync_corpus
```

Current interoperability samples:

- `interop/samples/noaa-hrrr-conus-lambert-refc.grib2`: first GRIB message
  from NOAA HRRR CONUS surface analysis `hrrr.t00z.wrfsfcf00.grib2`, cycle
  `2024-01-01 00Z`, bytes `0-202809`, public NOAA Open Data S3 object
  `https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.20240101/conus/hrrr.t00z.wrfsfcf00.grib2`.
