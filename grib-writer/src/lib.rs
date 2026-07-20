//! GRIB writer crate.

#![forbid(unsafe_code)]

mod builder;
mod grib1;
mod packing;
mod sections;

use std::io::Write;

use grib_core::binary::{write_u24_be, write_u8_be};

#[cfg(test)]
use grib1::validate_grib1_binary_data_flags;
use grib1::{
    checked_grib1_u24_length, validate_supported_grib1_grid, write_grib1_bitmap_section,
    write_grib1_data_section, write_grib1_grid_section, write_grib1_predefined_bitmap_section,
    write_grib1_product_section,
};
use sections::{
    checked_grid_point_count, validate_supported_grid, validate_supported_product,
    write_bitmap_section, write_data_representation_section, write_data_section,
    write_grid_section, write_identification_section, write_indicator_placeholder,
    write_product_section,
};

pub use builder::{Grib1Field, Grib1FieldBuilder, Grib2Field, Grib2FieldBuilder, ValueOrder};
pub use grib_core::grib1::ProductDefinition as Grib1ProductDefinition;
pub use grib_core::{Error, Result};
pub use packing::{PackingStrategy, SpatialDifferencingOrder};

/// Streaming GRIB writer.
pub struct GribWriter<'a, W> {
    out: &'a mut W,
}

impl<'a, W: Write> GribWriter<'a, W> {
    pub fn new(out: &'a mut W) -> Self {
        Self { out }
    }

    pub fn write_grib1_message(&mut self, field: Grib1Field) -> Result<()> {
        let mut body = Vec::new();
        write_grib1_product_section(&mut body, field.product())?;
        write_grib1_grid_section(&mut body, field.grid())?;
        if let Some(table_reference) = field.predefined_bitmap_reference() {
            write_grib1_predefined_bitmap_section(&mut body, table_reference)?;
        } else if let Some(bitmap) = &field.packed().bitmap_payload {
            write_grib1_bitmap_section(&mut body, bitmap, field.grid().num_points())?;
        }
        write_grib1_data_section(&mut body, field.packed(), 0)?;

        let total_len = checked_grib1_u24_length(8usize + body.len() + 4, 0)?;
        let mut message = Vec::new();
        message.extend_from_slice(b"GRIB");
        write_u24_be(&mut message, total_len)?;
        write_u8_be(&mut message, 1)?;
        message.extend_from_slice(&body);
        message.extend_from_slice(b"7777");

        self.out
            .write_all(&message)
            .map_err(|err| Error::Io(err, "GRIB writer output".into()))
    }

    pub fn write_grib2_message<I>(&mut self, fields: I) -> Result<()>
    where
        I: IntoIterator<Item = Grib2Field>,
    {
        let fields = fields.into_iter().collect::<Vec<_>>();
        if fields.is_empty() {
            return Err(Error::InvalidMessage(
                "cannot write a GRIB2 message without fields".into(),
            ));
        }

        let first = &fields[0];
        for field in &fields[1..] {
            if field.discipline() != first.discipline() {
                return Err(Error::InvalidMessage(
                    "all fields in a GRIB2 message must share a discipline".into(),
                ));
            }
            if field.identification() != first.identification() {
                return Err(Error::InvalidMessage(
                    "all fields in a GRIB2 message must share Section 1 identification".into(),
                ));
            }
        }

        let mut message = Vec::new();
        write_indicator_placeholder(&mut message, first.discipline())?;
        write_identification_section(&mut message, first.identification())?;
        let mut current_grid = None;
        for field in &fields {
            if current_grid != Some(field.grid()) {
                write_grid_section(&mut message, field.grid())?;
                current_grid = Some(field.grid());
            }
            write_product_section(&mut message, field.product())?;
            write_data_representation_section(&mut message, field.packed())?;
            if let Some(bitmap) = &field.packed().bitmap_payload {
                write_bitmap_section(&mut message, bitmap)?;
            }
            write_data_section(&mut message, &field.packed().data_payload)?;
        }
        message.extend_from_slice(b"7777");

        let total_len = u64::try_from(message.len())
            .map_err(|_| Error::Other("GRIB2 message length exceeds u64".into()))?;
        message[8..16].copy_from_slice(&total_len.to_be_bytes());

        self.out
            .write_all(&message)
            .map_err(|err| Error::Io(err, "GRIB writer output".into()))
    }
}

#[cfg(test)]
mod tests;
