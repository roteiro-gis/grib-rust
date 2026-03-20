#![no_main]

mod common;

use grib_reader::OpenOptions;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    common::exercise_open(data, OpenOptions { strict: false });
});
