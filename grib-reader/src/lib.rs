//! Pure-Rust GRIB file reader.
//!
//! The current implementation supports the production-critical baseline for both
//! GRIB1 and GRIB2: regular latitude/longitude grids, simple packing, and
//! GRIB2 complex packing with general group splitting.
//!
//! # Example
//!
//! ```no_run
//! use grib_reader::GribFile;
//!
//! let file = GribFile::open("gfs.grib2")?;
//! println!("messages: {}", file.message_count());
//!
//! for msg in file.messages() {
//!     println!(
//!         "  {} {:?} {:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
//!         msg.parameter_name(),
//!         msg.grid_shape(),
//!         msg.reference_time().year,
//!         msg.reference_time().month,
//!         msg.reference_time().day,
//!         msg.reference_time().hour,
//!         msg.reference_time().minute,
//!         msg.reference_time().second,
//!     );
//! }
//!
//! let data = file.message(0)?.read_data_as_f64()?;
//! println!("shape: {:?}", data.shape());
//! # Ok::<(), grib_reader::Error>(())
//! ```

pub mod data;
pub mod error;
pub mod grib1;
pub mod grid;
pub mod indicator;
pub mod metadata;
pub mod parameter;
pub mod product;
pub mod sections;
mod util;

pub use data::DecodeSample;
pub use error::{Error, Result};
pub use metadata::{Parameter, ReferenceTime};
pub use product::{
    AnalysisOrForecastTemplate, FixedSurface, Identification, ProductDefinition,
    ProductDefinitionTemplate,
};

use std::path::Path;

use memmap2::Mmap;
use ndarray::{ArrayD, IxDyn};

use crate::data::{
    bitmap_payload as grib2_bitmap_payload, decode_field_into, decode_payload_into,
    DataRepresentation,
};
use crate::grib1::{BinaryDataSection, GridDescription};
use crate::grid::GridDefinition;
use crate::indicator::Indicator;
use crate::sections::{index_fields, FieldSections, SectionRef};

#[cfg(feature = "rayon")]
use rayon::prelude::*;

const GRIB_MAGIC: &[u8; 4] = b"GRIB";

/// Configuration for opening GRIB data.
#[derive(Debug, Clone, Copy)]
pub struct OpenOptions {
    /// When `true`, the first malformed GRIB candidate aborts opening.
    ///
    /// When `false`, candidate offsets with invalid framing are skipped and
    /// scanning continues. Once a candidate has a valid indicator, message
    /// length, and end marker, any indexing or decoding error is returned.
    pub strict: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self { strict: true }
    }
}

/// A parsed GRIB field.
#[derive(Debug, Clone)]
pub struct MessageMetadata {
    pub edition: u8,
    pub center_id: u16,
    pub subcenter_id: u16,
    pub discipline: Option<u8>,
    pub reference_time: ReferenceTime,
    pub parameter: Parameter,
    pub grid: GridDefinition,
    pub data_representation: DataRepresentation,
    pub forecast_time_unit: Option<u8>,
    pub forecast_time: Option<u32>,
    pub message_offset: u64,
    pub message_length: u64,
    pub field_index_in_message: usize,
    grib1_product: Option<grib1::ProductDefinition>,
    grib2_identification: Option<Identification>,
    grib2_product: Option<ProductDefinition>,
}

/// A GRIB file containing one or more logical fields.
pub struct GribFile {
    data: GribData,
    messages: Vec<MessageIndex>,
}

#[derive(Clone)]
struct MessageIndex {
    offset: usize,
    length: usize,
    metadata: MessageMetadata,
    decode_plan: DecodePlan,
}

#[derive(Clone, Copy)]
enum DecodePlan {
    Grib1 {
        bitmap: Option<SectionRef>,
        data: SectionRef,
    },
    Grib2(FieldSections),
}

enum GribData {
    Mmap(Mmap),
    Bytes(Vec<u8>),
}

impl GribData {
    fn as_bytes(&self) -> &[u8] {
        match self {
            GribData::Mmap(m) => m,
            GribData::Bytes(b) => b,
        }
    }
}

impl GribFile {
    /// Open a GRIB file from disk using memory-mapped I/O.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with_options(path, OpenOptions::default())
    }

    /// Open a GRIB file from disk using explicit decoder options.
    pub fn open_with_options<P: AsRef<Path>>(path: P, options: OpenOptions) -> Result<Self> {
        let file = std::fs::File::open(path.as_ref())
            .map_err(|e| Error::Io(e, path.as_ref().display().to_string()))?;
        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| Error::Io(e, path.as_ref().display().to_string()))?;
        Self::from_data(GribData::Mmap(mmap), options)
    }

    /// Open a GRIB file from an owned byte buffer.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        Self::from_bytes_with_options(data, OpenOptions::default())
    }

    /// Open a GRIB file from an owned byte buffer using explicit decoder options.
    pub fn from_bytes_with_options(data: Vec<u8>, options: OpenOptions) -> Result<Self> {
        Self::from_data(GribData::Bytes(data), options)
    }

    fn from_data(data: GribData, options: OpenOptions) -> Result<Self> {
        let messages = scan_messages(data.as_bytes(), options)?;
        if messages.is_empty() {
            return Err(Error::NoMessages);
        }
        Ok(Self { data, messages })
    }

    /// Returns the GRIB edition of the first field.
    pub fn edition(&self) -> u8 {
        self.messages
            .first()
            .map(|message| message.metadata.edition)
            .unwrap_or(0)
    }

    /// Returns the number of logical fields in the file.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Access a field by index.
    pub fn message(&self, index: usize) -> Result<Message<'_>> {
        let record = self
            .messages
            .get(index)
            .ok_or(Error::MessageNotFound(index))?;
        let bytes = &self.data.as_bytes()[record.offset..record.offset + record.length];
        Ok(Message {
            index,
            bytes,
            metadata: &record.metadata,
            decode_plan: record.decode_plan,
        })
    }

    /// Iterate over all fields.
    pub fn messages(&self) -> MessageIter<'_> {
        MessageIter {
            file: self,
            index: 0,
        }
    }

    /// Decode every field in the file.
    pub fn read_all_data_as_f64(&self) -> Result<Vec<ArrayD<f64>>> {
        #[cfg(feature = "rayon")]
        {
            (0..self.message_count())
                .into_par_iter()
                .map(|index| self.message(index)?.read_data_as_f64())
                .collect()
        }
        #[cfg(not(feature = "rayon"))]
        {
            (0..self.message_count())
                .map(|index| self.message(index)?.read_data_as_f64())
                .collect()
        }
    }

    /// Decode every field in the file as `f32`.
    pub fn read_all_data_as_f32(&self) -> Result<Vec<ArrayD<f32>>> {
        #[cfg(feature = "rayon")]
        {
            (0..self.message_count())
                .into_par_iter()
                .map(|index| self.message(index)?.read_data_as_f32())
                .collect()
        }
        #[cfg(not(feature = "rayon"))]
        {
            (0..self.message_count())
                .map(|index| self.message(index)?.read_data_as_f32())
                .collect()
        }
    }
}

/// A single logical GRIB field.
pub struct Message<'a> {
    bytes: &'a [u8],
    metadata: &'a MessageMetadata,
    decode_plan: DecodePlan,
    index: usize,
}

impl<'a> Message<'a> {
    pub fn edition(&self) -> u8 {
        self.metadata.edition
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn metadata(&self) -> &MessageMetadata {
        self.metadata
    }

    pub fn reference_time(&self) -> &ReferenceTime {
        &self.metadata.reference_time
    }

    pub fn parameter(&self) -> &Parameter {
        &self.metadata.parameter
    }

    pub fn center_id(&self) -> u16 {
        self.metadata.center_id
    }

    pub fn subcenter_id(&self) -> u16 {
        self.metadata.subcenter_id
    }

    pub fn identification(&self) -> Option<&Identification> {
        self.metadata.grib2_identification.as_ref()
    }

    pub fn product_definition(&self) -> Option<&ProductDefinition> {
        self.metadata.grib2_product.as_ref()
    }

    pub fn grib1_product_definition(&self) -> Option<&grib1::ProductDefinition> {
        self.metadata.grib1_product.as_ref()
    }

    pub fn grid_definition(&self) -> &GridDefinition {
        &self.metadata.grid
    }

    pub fn parameter_name(&self) -> &'static str {
        self.metadata.parameter.short_name
    }

    pub fn parameter_description(&self) -> &'static str {
        self.metadata.parameter.description
    }

    pub fn forecast_time_unit(&self) -> Option<u8> {
        self.metadata.forecast_time_unit
    }

    pub fn forecast_time(&self) -> Option<u32> {
        self.metadata.forecast_time
    }

    pub fn valid_time(&self) -> Option<ReferenceTime> {
        let unit = self.metadata.forecast_time_unit?;
        let lead = self.metadata.forecast_time?;
        self.metadata
            .reference_time
            .checked_add_forecast_time(unit, lead)
    }

    pub fn grid_shape(&self) -> (usize, usize) {
        self.metadata.grid.shape()
    }

    pub fn latitudes(&self) -> Option<Vec<f64>> {
        match &self.metadata.grid {
            GridDefinition::LatLon(grid) => Some(grid.latitudes()),
            GridDefinition::Unsupported(_) => None,
        }
    }

    pub fn longitudes(&self) -> Option<Vec<f64>> {
        match &self.metadata.grid {
            GridDefinition::LatLon(grid) => Some(grid.longitudes()),
            GridDefinition::Unsupported(_) => None,
        }
    }

    pub fn decode_into<T: DecodeSample>(&self, out: &mut [T]) -> Result<()> {
        let grid = match &self.metadata.grid {
            GridDefinition::LatLon(grid) => grid,
            GridDefinition::Unsupported(template) => {
                return Err(Error::UnsupportedGridTemplate(*template));
            }
        };

        match self.decode_plan {
            DecodePlan::Grib2(field) => {
                let data_section = section_bytes(self.bytes, field.data);
                let bitmap_section = match field.bitmap {
                    Some(section) => grib2_bitmap_payload(section_bytes(self.bytes, section))?,
                    None => None,
                };
                decode_field_into(
                    data_section,
                    &self.metadata.data_representation,
                    bitmap_section,
                    self.metadata.grid.num_points(),
                    out,
                )?
            }
            DecodePlan::Grib1 { bitmap, data } => {
                let bitmap_section = match bitmap {
                    Some(section) => grib1::bitmap_payload(section_bytes(self.bytes, section))?,
                    None => None,
                };
                let data_section = section_bytes(self.bytes, data);
                if data_section.len() < 11 {
                    return Err(Error::InvalidSection {
                        section: 4,
                        reason: format!("expected at least 11 bytes, got {}", data_section.len()),
                    });
                }
                decode_payload_into(
                    &data_section[11..],
                    &self.metadata.data_representation,
                    bitmap_section,
                    self.metadata.grid.num_points(),
                    out,
                )?
            }
        }

        grid.reorder_for_ndarray_in_place(out)
    }

    pub fn read_flat_data_as_f64(&self) -> Result<Vec<f64>> {
        let mut decoded = vec![0.0; self.metadata.grid.num_points()];
        self.decode_into(&mut decoded)?;
        Ok(decoded)
    }

    pub fn read_flat_data_as_f32(&self) -> Result<Vec<f32>> {
        let mut decoded = vec![0.0_f32; self.metadata.grid.num_points()];
        self.decode_into(&mut decoded)?;
        Ok(decoded)
    }

    pub fn read_data_as_f64(&self) -> Result<ArrayD<f64>> {
        let ordered = self.read_flat_data_as_f64()?;
        ArrayD::from_shape_vec(IxDyn(&self.metadata.grid.ndarray_shape()), ordered)
            .map_err(|e| Error::Other(format!("failed to build ndarray from decoded field: {e}")))
    }

    pub fn read_data_as_f32(&self) -> Result<ArrayD<f32>> {
        let ordered = self.read_flat_data_as_f32()?;
        ArrayD::from_shape_vec(IxDyn(&self.metadata.grid.ndarray_shape()), ordered)
            .map_err(|e| Error::Other(format!("failed to build ndarray from decoded field: {e}")))
    }

    pub fn raw_bytes(&self) -> &[u8] {
        self.bytes
    }
}

/// Iterator over fields in a GRIB file.
pub struct MessageIter<'a> {
    file: &'a GribFile,
    index: usize,
}

impl<'a> Iterator for MessageIter<'a> {
    type Item = Message<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.file.message_count() {
            return None;
        }
        let message = self.file.message(self.index).ok()?;
        self.index += 1;
        Some(message)
    }
}

fn section_bytes(msg_bytes: &[u8], section: SectionRef) -> &[u8] {
    &msg_bytes[section.offset..section.offset + section.length]
}

fn scan_messages(data: &[u8], options: OpenOptions) -> Result<Vec<MessageIndex>> {
    let mut messages = Vec::new();
    let mut pos = 0usize;

    while pos + 8 <= data.len() {
        if &data[pos..pos + 4] != GRIB_MAGIC {
            pos += 1;
            continue;
        }

        let (indicator, next_pos) = match locate_message(data, pos) {
            Ok(located) => located,
            Err(err) if !options.strict && is_recoverable_candidate_error(&err) => {
                pos += 4;
                continue;
            }
            Err(err) => return Err(err),
        };

        let message_bytes = &data[pos..next_pos];
        let indexed = match indicator.edition {
            1 => index_grib1_message(message_bytes, pos)?,
            2 => index_grib2_message(message_bytes, pos, &indicator)?,
            other => return Err(Error::UnsupportedEdition(other)),
        };

        messages.extend(indexed);
        pos = next_pos;
    }

    Ok(messages)
}

fn is_recoverable_candidate_error(err: &Error) -> bool {
    matches!(err, Error::InvalidMessage(_) | Error::Truncated { .. })
}

fn locate_message(data: &[u8], pos: usize) -> Result<(Indicator, usize)> {
    let indicator = Indicator::parse(&data[pos..]).ok_or_else(|| {
        Error::InvalidMessage(format!("failed to parse indicator at byte offset {pos}"))
    })?;
    let length = indicator.total_length as usize;
    if length < 12 {
        return Err(Error::InvalidMessage(format!(
            "message at byte offset {pos} reports impossible length {length}"
        )));
    }
    let end = pos
        .checked_add(length)
        .ok_or_else(|| Error::InvalidMessage("message length overflow".into()))?;
    if end > data.len() {
        return Err(Error::Truncated { offset: end as u64 });
    }
    if &data[end - 4..end] != b"7777" {
        return Err(Error::InvalidMessage(format!(
            "message at byte offset {pos} does not end with 7777"
        )));
    }

    Ok((indicator, end))
}

fn index_grib1_message(message_bytes: &[u8], offset: usize) -> Result<Vec<MessageIndex>> {
    let sections = grib1::parse_message_sections(message_bytes)?;
    let grid_ref = sections.grid.ok_or_else(|| {
        Error::InvalidSectionOrder(
            "GRIB1 decoding requires an explicit grid definition section".into(),
        )
    })?;
    let grid_description = GridDescription::parse(section_bytes(message_bytes, grid_ref))?;
    let grid = grid_description.grid;

    let bitmap_present_count = match sections.bitmap {
        Some(bitmap) => count_bitmap_present_points(
            grib1::bitmap_payload(section_bytes(message_bytes, bitmap))?,
            grid.num_points(),
        )?,
        None => grid.num_points(),
    };

    let (_bds, data_representation) = BinaryDataSection::parse(
        section_bytes(message_bytes, sections.data),
        sections.product.decimal_scale,
        bitmap_present_count,
    )?;
    let parameter = sections.product.parameter();

    Ok(vec![MessageIndex {
        offset,
        length: message_bytes.len(),
        decode_plan: DecodePlan::Grib1 {
            bitmap: sections.bitmap,
            data: sections.data,
        },
        metadata: MessageMetadata {
            edition: 1,
            center_id: sections.product.center_id as u16,
            subcenter_id: sections.product.subcenter_id as u16,
            discipline: None,
            reference_time: sections.product.reference_time,
            parameter,
            grid,
            data_representation,
            forecast_time_unit: Some(sections.product.forecast_time_unit),
            forecast_time: sections.product.forecast_time(),
            message_offset: offset as u64,
            message_length: message_bytes.len() as u64,
            field_index_in_message: 0,
            grib1_product: Some(sections.product),
            grib2_identification: None,
            grib2_product: None,
        },
    }])
}

fn index_grib2_message(
    message_bytes: &[u8],
    offset: usize,
    indicator: &Indicator,
) -> Result<Vec<MessageIndex>> {
    let fields = index_fields(message_bytes)?;
    let mut messages = Vec::with_capacity(fields.len());

    for (field_index_in_message, field_sections) in fields.into_iter().enumerate() {
        let identification =
            Identification::parse(section_bytes(message_bytes, field_sections.identification))?;
        let grid = GridDefinition::parse(section_bytes(message_bytes, field_sections.grid))?;
        let product =
            ProductDefinition::parse(section_bytes(message_bytes, field_sections.product))?;
        let data_representation = DataRepresentation::parse(section_bytes(
            message_bytes,
            field_sections.data_representation,
        ))?;
        let parameter = Parameter::new_grib2(
            indicator.discipline,
            product.parameter_category,
            product.parameter_number,
            product.parameter_name(indicator.discipline),
            product.parameter_description(indicator.discipline),
        );

        messages.push(MessageIndex {
            offset,
            length: message_bytes.len(),
            decode_plan: DecodePlan::Grib2(field_sections),
            metadata: MessageMetadata {
                edition: indicator.edition,
                center_id: identification.center_id,
                subcenter_id: identification.subcenter_id,
                discipline: Some(indicator.discipline),
                reference_time: ReferenceTime {
                    year: identification.reference_year,
                    month: identification.reference_month,
                    day: identification.reference_day,
                    hour: identification.reference_hour,
                    minute: identification.reference_minute,
                    second: identification.reference_second,
                },
                parameter,
                grid,
                data_representation,
                forecast_time_unit: product.forecast_time_unit(),
                forecast_time: product.forecast_time(),
                message_offset: offset as u64,
                message_length: message_bytes.len() as u64,
                field_index_in_message,
                grib1_product: None,
                grib2_identification: Some(identification),
                grib2_product: Some(product),
            },
        });
    }

    Ok(messages)
}

fn count_bitmap_present_points(bitmap: Option<&[u8]>, num_grid_points: usize) -> Result<usize> {
    let Some(payload) = bitmap else {
        return Ok(0);
    };

    let full_bytes = num_grid_points / 8;
    let remaining_bits = num_grid_points % 8;
    let required_bytes = full_bytes + usize::from(remaining_bits > 0);
    if payload.len() < required_bytes {
        return Err(Error::MissingBitmap);
    }

    let mut present = payload[..full_bytes]
        .iter()
        .map(|byte| byte.count_ones() as usize)
        .sum();
    if remaining_bits > 0 {
        let mask = u8::MAX << (8 - remaining_bits);
        present += (payload[full_bytes] & mask).count_ones() as usize;
    }

    Ok(present)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grib_i32_bytes(value: i32) -> [u8; 4] {
        if value >= 0 {
            (value as u32).to_be_bytes()
        } else {
            ((-value) as u32 | 0x8000_0000).to_be_bytes()
        }
    }

    fn build_indicator(total_len: usize, discipline: u8) -> Vec<u8> {
        let mut indicator = Vec::with_capacity(16);
        indicator.extend_from_slice(b"GRIB");
        indicator.extend_from_slice(&[0, 0]);
        indicator.push(discipline);
        indicator.push(2);
        indicator.extend_from_slice(&(total_len as u64).to_be_bytes());
        indicator
    }

    fn build_identification() -> Vec<u8> {
        let mut section = vec![0u8; 21];
        section[..4].copy_from_slice(&(21u32).to_be_bytes());
        section[4] = 1;
        section[5..7].copy_from_slice(&7u16.to_be_bytes());
        section[7..9].copy_from_slice(&0u16.to_be_bytes());
        section[9] = 35;
        section[10] = 1;
        section[11] = 1;
        section[12..14].copy_from_slice(&2026u16.to_be_bytes());
        section[14] = 3;
        section[15] = 20;
        section[16] = 12;
        section[17] = 0;
        section[18] = 0;
        section[19] = 0;
        section[20] = 1;
        section
    }

    fn build_grid(ni: u32, nj: u32, scanning_mode: u8) -> Vec<u8> {
        let mut section = vec![0u8; 72];
        section[..4].copy_from_slice(&(72u32).to_be_bytes());
        section[4] = 3;
        section[6..10].copy_from_slice(&(ni * nj).to_be_bytes());
        section[12..14].copy_from_slice(&0u16.to_be_bytes());
        section[30..34].copy_from_slice(&ni.to_be_bytes());
        section[34..38].copy_from_slice(&nj.to_be_bytes());
        section[46..50].copy_from_slice(&grib_i32_bytes(50_000_000));
        section[50..54].copy_from_slice(&grib_i32_bytes(-120_000_000));
        section[55..59].copy_from_slice(&grib_i32_bytes(49_000_000));
        section[59..63].copy_from_slice(&grib_i32_bytes(-119_000_000));
        section[63..67].copy_from_slice(&1_000_000u32.to_be_bytes());
        section[67..71].copy_from_slice(&1_000_000u32.to_be_bytes());
        section[71] = scanning_mode;
        section
    }

    fn build_product(parameter_category: u8, parameter_number: u8) -> Vec<u8> {
        let mut section = vec![0u8; 34];
        section[..4].copy_from_slice(&(34u32).to_be_bytes());
        section[4] = 4;
        section[7..9].copy_from_slice(&0u16.to_be_bytes());
        section[9] = parameter_category;
        section[10] = parameter_number;
        section[11] = 2;
        section[17] = 1;
        section[18..22].copy_from_slice(&0u32.to_be_bytes());
        section[22] = 103;
        section[23] = 0;
        section[24..28].copy_from_slice(&850u32.to_be_bytes());
        section[28] = 255;
        section
    }

    fn build_simple_representation(encoded_values: usize, bits_per_value: u8) -> Vec<u8> {
        let mut section = vec![0u8; 21];
        section[..4].copy_from_slice(&(21u32).to_be_bytes());
        section[4] = 5;
        section[5..9].copy_from_slice(&(encoded_values as u32).to_be_bytes());
        section[9..11].copy_from_slice(&0u16.to_be_bytes());
        section[11..15].copy_from_slice(&0f32.to_be_bytes());
        section[19] = bits_per_value;
        section[20] = 0;
        section
    }

    fn pack_u8_values(values: &[u8]) -> Vec<u8> {
        values.to_vec()
    }

    fn build_bitmap(bits: &[bool]) -> Vec<u8> {
        let payload_len = bits.len().div_ceil(8) + 1;
        let mut section = vec![0u8; payload_len + 5];
        section[..4].copy_from_slice(&((payload_len + 5) as u32).to_be_bytes());
        section[4] = 6;
        section[5] = 0;
        for (index, bit) in bits.iter().copied().enumerate() {
            if bit {
                section[6 + index / 8] |= 1 << (7 - (index % 8));
            }
        }
        section
    }

    fn build_data(payload: &[u8]) -> Vec<u8> {
        let mut section = vec![0u8; payload.len() + 5];
        section[..4].copy_from_slice(&((payload.len() + 5) as u32).to_be_bytes());
        section[4] = 7;
        section[5..].copy_from_slice(payload);
        section
    }

    fn assemble_grib2_message(sections: &[Vec<u8>]) -> Vec<u8> {
        let total_len = 16 + sections.iter().map(|section| section.len()).sum::<usize>() + 4;
        let mut message = build_indicator(total_len, 0);
        for section in sections {
            message.extend_from_slice(section);
        }
        message.extend_from_slice(b"7777");
        message
    }

    fn build_grib1_message(values: &[u8]) -> Vec<u8> {
        let mut pds = vec![0u8; 28];
        pds[..3].copy_from_slice(&[0, 0, 28]);
        pds[3] = 2;
        pds[4] = 7;
        pds[5] = 255;
        pds[6] = 0;
        pds[7] = 0b1000_0000;
        pds[8] = 11;
        pds[9] = 100;
        pds[10..12].copy_from_slice(&850u16.to_be_bytes());
        pds[12] = 26;
        pds[13] = 3;
        pds[14] = 20;
        pds[15] = 12;
        pds[16] = 0;
        pds[17] = 1;
        pds[18] = 0;
        pds[19] = 0;
        pds[20] = 0;
        pds[24] = 21;
        pds[25] = 0;

        let mut gds = vec![0u8; 32];
        gds[..3].copy_from_slice(&[0, 0, 32]);
        gds[5] = 0;
        gds[6..8].copy_from_slice(&2u16.to_be_bytes());
        gds[8..10].copy_from_slice(&2u16.to_be_bytes());
        gds[10..13].copy_from_slice(&[0x01, 0x4d, 0x50]);
        gds[13..16].copy_from_slice(&[0x81, 0xd4, 0xc0]);
        gds[16] = 0x80;
        gds[17..20].copy_from_slice(&[0x01, 0x49, 0x68]);
        gds[20..23].copy_from_slice(&[0x81, 0xd0, 0xd8]);
        gds[23..25].copy_from_slice(&1000u16.to_be_bytes());
        gds[25..27].copy_from_slice(&1000u16.to_be_bytes());

        let mut bds = vec![0u8; 11 + values.len()];
        let len = bds.len() as u32;
        bds[..3].copy_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
        bds[3] = 0;
        bds[10] = 8;
        bds[11..].copy_from_slice(values);

        let total_len = 8 + pds.len() + gds.len() + bds.len() + 4;
        let mut message = Vec::new();
        message.extend_from_slice(b"GRIB");
        message.extend_from_slice(&[
            ((total_len >> 16) & 0xff) as u8,
            ((total_len >> 8) & 0xff) as u8,
            (total_len & 0xff) as u8,
            1,
        ]);
        message.extend_from_slice(&pds);
        message.extend_from_slice(&gds);
        message.extend_from_slice(&bds);
        message.extend_from_slice(b"7777");
        message
    }

    #[test]
    fn scans_single_grib2_message() {
        let message = assemble_grib2_message(&[
            build_identification(),
            build_grid(2, 2, 0),
            build_product(0, 0),
            build_simple_representation(4, 8),
            build_data(&pack_u8_values(&[1, 2, 3, 4])),
        ]);
        let messages = scan_messages(&message, OpenOptions::default()).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].metadata.parameter.short_name, "TMP");
    }

    #[test]
    fn decodes_simple_grib2_message_to_ndarray() {
        let message = assemble_grib2_message(&[
            build_identification(),
            build_grid(2, 2, 0),
            build_product(0, 0),
            build_simple_representation(4, 8),
            build_data(&pack_u8_values(&[1, 2, 3, 4])),
        ]);
        let file = GribFile::from_bytes(message).unwrap();
        let array = file.message(0).unwrap().read_data_as_f64().unwrap();
        assert_eq!(array.shape(), &[2, 2]);
        assert_eq!(
            array.iter().copied().collect::<Vec<_>>(),
            vec![1.0, 2.0, 3.0, 4.0]
        );
    }

    #[test]
    fn applies_bitmap_to_missing_values() {
        let message = assemble_grib2_message(&[
            build_identification(),
            build_grid(2, 2, 0),
            build_product(0, 1),
            build_simple_representation(3, 8),
            build_bitmap(&[true, false, true, true]),
            build_data(&pack_u8_values(&[10, 20, 30])),
        ]);
        let file = GribFile::from_bytes(message).unwrap();
        let array = file.message(0).unwrap().read_data_as_f64().unwrap();
        let values = array.iter().copied().collect::<Vec<_>>();
        assert_eq!(values[0], 10.0);
        assert!(values[1].is_nan());
        assert_eq!(values[2], 20.0);
        assert_eq!(values[3], 30.0);
    }

    #[test]
    fn indexes_multiple_fields_in_one_grib2_message() {
        let message = assemble_grib2_message(&[
            build_identification(),
            build_grid(2, 2, 0),
            build_product(0, 0),
            build_simple_representation(4, 8),
            build_data(&pack_u8_values(&[1, 2, 3, 4])),
            build_product(0, 2),
            build_simple_representation(4, 8),
            build_data(&pack_u8_values(&[5, 6, 7, 8])),
        ]);
        let file = GribFile::from_bytes(message).unwrap();
        assert_eq!(file.message_count(), 2);
        assert_eq!(file.message(0).unwrap().parameter_name(), "TMP");
        assert_eq!(file.message(1).unwrap().parameter_name(), "POT");
    }

    #[test]
    fn decodes_simple_grib1_message_to_ndarray() {
        let message = build_grib1_message(&[1, 2, 3, 4]);
        let file = GribFile::from_bytes(message).unwrap();
        assert_eq!(file.edition(), 1);
        let field = file.message(0).unwrap();
        assert_eq!(field.parameter_name(), "TMP");
        assert!(field.identification().is_none());
        assert!(field.grib1_product_definition().is_some());
        let array = field.read_data_as_f64().unwrap();
        assert_eq!(array.shape(), &[2, 2]);
        assert_eq!(
            array.iter().copied().collect::<Vec<_>>(),
            vec![1.0, 2.0, 3.0, 4.0]
        );
    }
}
