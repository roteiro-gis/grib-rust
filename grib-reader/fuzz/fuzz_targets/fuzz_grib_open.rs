#![no_main]

use grib_reader::GribFile;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = GribFile::from_bytes(data.to_vec()) {
        for message in file.messages() {
            let _ = message.edition();
            let _ = message.parameter_name();
            let _ = message.parameter_description();
            let _ = message.grid_shape();
            let _ = message.reference_time();
            let _ = message.read_data_as_f64();
        }
    }
});
