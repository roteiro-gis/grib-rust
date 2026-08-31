#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image_name="${ECCODES_DOCKER_IMAGE:-grib-rust-eccodes}"
docker_cpus="${ECCODES_DOCKER_CPUS:-2}"
cargo_build_jobs="${CARGO_BUILD_JOBS:-2}"
cargo_term_color="${CARGO_TERM_COLOR:-always}"
rustflags="${RUSTFLAGS:--D warnings}"
if [[ -n "${ECCODES_CARGO_TARGET_DIR:-}" ]]; then
  cargo_target_dir="${ECCODES_CARGO_TARGET_DIR}"
elif [[ "$(uname -s)" == "Linux" ]]; then
  cargo_target_dir=/workspace/target
else
  cargo_target_dir=/tmp/grib-rust-target
fi

docker build -f "${repo_root}/docker/eccodes.Dockerfile" -t "${image_name}" "${repo_root}"
docker run --rm --cpus "${docker_cpus}" \
  -e CARGO_BUILD_JOBS="${cargo_build_jobs}" \
  -e CARGO_TARGET_DIR="${cargo_target_dir}" \
  -e CARGO_TERM_COLOR="${cargo_term_color}" \
  -e RUSTFLAGS="${rustflags}" \
  -v "${repo_root}:/workspace" \
  -w /workspace \
  "${image_name}" bash -lc '
  set -euo pipefail
  mkdir -p "${CARGO_TARGET_DIR}"
  helper="${CARGO_TARGET_DIR}/eccodes-reference"
  cc -O2 -Wall -Wextra $(pkg-config --cflags eccodes) tools/eccodes-reference.c -o "${helper}" $(pkg-config --libs eccodes)
  GRIB_READER_ECCODES_HELPER="${helper}" /usr/local/cargo/bin/cargo test -p grib-reader --test parity_eccodes --all-features --locked -- --ignored
  GRIB_READER_ECCODES_HELPER="${helper}" /usr/local/cargo/bin/cargo test -p grib-writer --test parity_eccodes --all-features --locked -- --ignored
'
