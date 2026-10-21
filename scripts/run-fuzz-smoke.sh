#!/usr/bin/env bash
set -euo pipefail

duration="${1:-60}"
if [[ ! "${duration}" =~ ^[1-9][0-9]*$ ]]; then
  echo "usage: $0 [positive-duration-seconds]" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fuzz_dir="${repo_root}/grib-reader/fuzz"
toolchain="${FUZZ_TOOLCHAIN:-nightly}"
toolchain_cargo="$(rustup which --toolchain "${toolchain}" cargo)"
toolchain_bin="$(dirname "${toolchain_cargo}")"
run_root="$(mktemp -d "${TMPDIR:-/tmp}/grib-rust-fuzz.XXXXXX")"
targets=(
  fuzz_grib_open
  fuzz_grib_tolerant
  fuzz_grib_sections
  fuzz_grib_writer_inputs
)

cleanup() {
  rm -rf -- "${run_root}"
}
trap cleanup EXIT

for target in "${targets[@]}"; do
  corpus="${run_root}/${target}"
  seed_corpus="${fuzz_dir}/corpus/${target}"
  mkdir -p "${corpus}"
  if [[ -d "${seed_corpus}" ]]; then
    cp -R "${seed_corpus}/." "${corpus}/"
  fi

  PATH="${toolchain_bin}:${PATH}" "${toolchain_cargo}" fuzz run \
    --fuzz-dir "${fuzz_dir}" \
    "${target}" \
    "${corpus}" \
    -- \
    "-max_total_time=${duration}" \
    -timeout=10 \
    -rss_limit_mb=3072
done
