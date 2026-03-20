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
