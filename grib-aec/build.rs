use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const LIBAEC_VERSION: (u8, u8, u8) = (1, 1, 7);

fn main() {
    let vendor = Path::new("vendor/libaec");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));

    write_config_header(&out_dir);
    write_public_header(vendor, &out_dir);

    let mut build = cc::Build::new();
    build
        .include(&out_dir)
        .include(vendor.join("src"))
        .define("LIBAEC_BUILD", None)
        .file(vendor.join("src/decode.c"))
        .file(vendor.join("src/encode.c"))
        .file(vendor.join("src/encode_accessors.c"))
        .file(vendor.join("src/vector.c"))
        .warnings(true)
        .extra_warnings(true)
        .flag_if_supported("-std=c99")
        .flag_if_supported("-fvisibility=hidden")
        .flag_if_supported("-Wno-unused-parameter");
    namespace_public_symbols(&mut build);
    build.compile("grib_aec");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", vendor.display());
}

fn namespace_public_symbols(build: &mut cc::Build) {
    for symbol in [
        "aec_encode_init",
        "aec_encode_enable_offsets",
        "aec_encode_count_offsets",
        "aec_encode_get_offsets",
        "aec_buffer_seek",
        "aec_encode",
        "aec_encode_end",
        "aec_decode_init",
        "aec_decode_enable_offsets",
        "aec_decode_count_offsets",
        "aec_decode_get_offsets",
        "aec_decode",
        "aec_decode_range",
        "aec_decode_end",
        "aec_buffer_encode",
        "aec_buffer_decode",
        "aec_get_8",
        "aec_get_lsb_16",
        "aec_get_lsb_24",
        "aec_get_lsb_32",
        "aec_get_msb_16",
        "aec_get_msb_24",
        "aec_get_msb_32",
        "aec_get_rsi_8",
        "aec_get_rsi_lsb_16",
        "aec_get_rsi_lsb_24",
        "aec_get_rsi_lsb_32",
        "aec_get_rsi_msb_16",
        "aec_get_rsi_msb_24",
        "aec_get_rsi_msb_32",
        "vector_create",
        "vector_size",
        "vector_destroy",
        "vector_equal",
        "vector_at",
        "vector_push_back",
        "vector_data",
    ] {
        let namespaced = if let Some(suffix) = symbol.strip_prefix("aec_") {
            format!("grib_aec_{suffix}")
        } else {
            format!("grib_aec_{symbol}")
        };
        build.define(symbol, Some(namespaced.as_str()));
    }
}

fn write_config_header(out_dir: &Path) {
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_endian = env::var("CARGO_CFG_TARGET_ENDIAN").unwrap_or_default();
    let bit_scan_configuration = if target_env == "msvc" {
        "#define HAVE_DECL___BUILTIN_CLZLL 0\n#define HAVE_BSR64 1\n"
    } else {
        "#define HAVE_DECL___BUILTIN_CLZLL 1\n"
    };
    let endian_configuration = if target_endian == "big" {
        "#define WORDS_BIGENDIAN 1\n"
    } else {
        ""
    };
    fs::write(
        out_dir.join("config.h"),
        format!(
            "#ifndef GRIB_AEC_CONFIG_H\n#define GRIB_AEC_CONFIG_H\n{bit_scan_configuration}{endian_configuration}#endif\n"
        ),
    )
    .expect("failed writing libaec config.h");
}

fn write_public_header(vendor: &Path, out_dir: &Path) {
    let template = fs::read_to_string(vendor.join("include/libaec.h.in"))
        .expect("failed reading vendored libaec.h.in");
    let header = template
        .replace("@PROJECT_VERSION_MAJOR@", &LIBAEC_VERSION.0.to_string())
        .replace("@PROJECT_VERSION_MINOR@", &LIBAEC_VERSION.1.to_string())
        .replace("@PROJECT_VERSION_PATCH@", &LIBAEC_VERSION.2.to_string());
    fs::write(out_dir.join("libaec.h"), header).expect("failed writing libaec.h");
}
