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

- `interop/samples/eccc-gem-global-latlon-albedo-jpeg2000.grib2`: Environment
  and Climate Change Canada GEM Global 15 km lat/lon albedo analysis
  `CMC_glb_ALBDO_SFC_0_latlon.15x.15_2026051000_P000.grib2`, cycle
  `2026-05-10 00Z`, public MSC Datamart object
  `https://dd.weather.gc.ca/today/model_gem_global/15km/grib2/lat_lon/00/000/CMC_glb_ALBDO_SFC_0_latlon.15x.15_2026051000_P000.grib2`.
- `interop/samples/noaa-hrrr-conus-lambert-refc.grib2`: first GRIB message
  from NOAA HRRR CONUS surface analysis `hrrr.t00z.wrfsfcf00.grib2`, cycle
  `2024-01-01 00Z`, bytes `0-202809`, public NOAA Open Data S3 object
  `https://noaa-hrrr-bdp-pds.s3.amazonaws.com/hrrr.20240101/conus/hrrr.t00z.wrfsfcf00.grib2`.
