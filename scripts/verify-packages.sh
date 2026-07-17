#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO:-cargo}"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/grib-rust-packages.XXXXXX")"

cleanup() {
  if [[ "${KEEP_PACKAGE_VERIFY_TEMP:-0}" == "1" ]]; then
    printf 'Package verification files retained at %s\n' "${work_dir}"
  else
    rm -rf "${work_dir}"
  fi
}
trap cleanup EXIT

package_target="${work_dir}/package-target"
package_dir="${package_target}/package"
packages_dir="${work_dir}/packages"
mkdir -p "${packages_dir}"

package_options=(--workspace --locked --no-verify)
if [[ "${VERIFY_PACKAGES_ALLOW_DIRTY:-0}" == "1" ]]; then
  package_options+=(--allow-dirty)
fi

(
  cd "${repo_root}"
  CARGO_TARGET_DIR="${package_target}" \
    "${cargo_bin}" package "${package_options[@]}"
)

shopt -s nullglob
archives=("${package_dir}"/*.crate)
if (( ${#archives[@]} == 0 )); then
  printf 'No workspace package archives were produced in %s\n' "${package_dir}" >&2
  exit 1
fi

package_names=()
package_paths=()
for archive in "${archives[@]}"; do
  top_level="$(tar -tzf "${archive}" | sed -n '1{s#/.*##;p;}')"
  if [[ -z "${top_level}" ]]; then
    printf 'Package archive has no top-level directory: %s\n' "${archive}" >&2
    exit 1
  fi
  tar -xzf "${archive}" -C "${packages_dir}"
  manifest="${packages_dir}/${top_level}/Cargo.toml"
  package_name="$(awk -F '"' '/^name = "/ { print $2; exit }' "${manifest}")"
  if [[ -z "${package_name}" ]]; then
    printf 'Could not read package name from %s\n' "${manifest}" >&2
    exit 1
  fi
  package_names+=("${package_name}")
  package_paths+=("packages/${top_level}")
done

{
  printf '[workspace]\nresolver = "2"\nmembers = [\n'
  for package_path in "${package_paths[@]}"; do
    printf '  "%s",\n' "${package_path}"
  done
  printf ']\n\n[patch.crates-io]\n'
  for index in "${!package_names[@]}"; do
    printf '%s = { path = "%s" }\n' \
      "${package_names[index]}" "${package_paths[index]}"
  done
} > "${work_dir}/Cargo.toml"
cp "${repo_root}/Cargo.lock" "${work_dir}/Cargo.lock"

(
  cd "${work_dir}"
  CARGO_TARGET_DIR="${work_dir}/verify-target" \
    "${cargo_bin}" check --workspace --all-targets --all-features --locked
)
