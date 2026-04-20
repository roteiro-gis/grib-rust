#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image_name="${ECCODES_DOCKER_IMAGE:-grib-rust-eccodes}"
cargo_term_color="${CARGO_TERM_COLOR:-always}"
rustflags="${RUSTFLAGS:--D warnings}"

docker build -f "${repo_root}/docker/eccodes.Dockerfile" -t "${image_name}" "${repo_root}"
docker run --rm \
  -e CARGO_TERM_COLOR="${cargo_term_color}" \
  -e RUSTFLAGS="${rustflags}" \
  -v "${repo_root}:/workspace" \
  -w /workspace \
  "${image_name}" bash -lc '
  mkdir -p target
  cc -O2 -Wall -Wextra $(pkg-config --cflags eccodes) tools/eccodes-reference.c -o target/eccodes-reference $(pkg-config --libs eccodes)
  GRIB_READER_ECCODES_HELPER=/workspace/target/eccodes-reference /usr/local/cargo/bin/cargo bench -p grib-reader --bench compare_eccodes --all-features
  GRIB_READER_ECCODES_HELPER=/workspace/target/eccodes-reference /usr/local/cargo/bin/cargo bench -p grib-writer --bench encode --all-features
'
