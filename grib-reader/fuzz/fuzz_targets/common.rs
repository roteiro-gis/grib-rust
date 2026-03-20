#![allow(dead_code)]

use grib_reader::indicator::Indicator;
use grib_reader::sections::{index_fields, scan_sections};
use grib_reader::grid::GridDefinition;
use grib_reader::{grib1, GribFile, Message, OpenOptions};

pub fn exercise_open(data: &[u8], options: OpenOptions) {
    if let Ok(file) = GribFile::from_bytes_with_options(data.to_vec(), options) {
        exercise_file(&file);
    }
}

pub fn exercise_sections(data: &[u8]) {
    let Some(indicator) = Indicator::parse(data) else {
        return;
    };
    let Ok(length) = usize::try_from(indicator.total_length) else {
        return;
    };
    if length > data.len() {
        return;
    }

    let message = &data[..length];
    match indicator.edition {
        1 => {
            let _ = grib1::parse_message_sections(message);
        }
        2 => {
            if let Ok(sections) = scan_sections(message) {
                let _ = index_fields(message);
                for section in sections {
                    if section.number == 3 {
                        let _ = GridDefinition::parse(
                            &message[section.offset..section.offset + section.length],
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

fn exercise_file(file: &GribFile) {
    let _ = file.edition();
    let _ = file.message_count();
    let _ = file.read_all_data_as_f64();
    for message in file.messages() {
        exercise_message(&message);
    }
}

fn exercise_message(message: &Message<'_>) {
    let _ = message.edition();
    let _ = message.index();
    let _ = message.metadata();
    let _ = message.center_id();
    let _ = message.subcenter_id();
    let _ = message.parameter();
    let _ = message.parameter_name();
    let _ = message.parameter_description();
    let _ = message.reference_time();
    let _ = message.identification();
    let _ = message.product_definition();
    let _ = message.grib1_product_definition();
    let _ = message.grid_definition();
    let _ = message.grid_shape();
    let _ = message.latitudes();
    let _ = message.longitudes();
    let _ = message.raw_bytes();
    let _ = message.read_data_as_f64();
}
