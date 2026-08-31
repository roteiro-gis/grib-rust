# grib-aec

Safe Rust buffer APIs for the CCSDS 121.0-B-3 adaptive entropy codec used by
GRIB2 data representation template 5.42.

The crate compiles a pinned copy of upstream libaec 1.1.7, the current release
containing the 2026 decoder-overflow and fuzzing security fixes. The C codec is
isolated behind a checked Rust interface shared by `grib-reader` and
`grib-writer`.

The vendored files come from tag `v1.1.7`, commit
`0c4c01463d2c64a112a61271d317b74efb660608` in
<https://github.com/Deutsches-Klimarechenzentrum/libaec>. The downloaded source
archive had SHA-256
`26661a569a7def45a2e97fbbd09e0dc5bbb2f8ab1b41250c19e795559eec6fb2`.
The upstream BSD license is preserved at `vendor/libaec/LICENSE.txt`.
