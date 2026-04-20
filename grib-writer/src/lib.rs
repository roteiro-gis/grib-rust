//! GRIB writer crate.

#![forbid(unsafe_code)]

use std::io::Write;

use grib_core::binary::{
    encode_wmo_i16, encode_wmo_i32, encode_wmo_i8, write_u16_be, write_u32_be, write_u64_be,
    write_u8_be,
};
use grib_core::bit::BitWriter;
use grib_core::{
    DataRepresentation, FixedSurface, GridDefinition, Identification, LatLonGrid,
    ProductDefinition, ProductDefinitionTemplate, SimplePackingParams,
};

pub use grib_core::{Error, Result};

/// Field packing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackingStrategy {
    /// GRIB2 Template 5.0 with binary scale 0 and automatic bit-width selection.
    SimpleAuto { decimal_scale: i16 },
}

/// Order of values supplied to [`Grib2FieldBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueOrder {
    /// Logical row-major order matching `grib-reader` ndarray output.
    #[default]
    LogicalRowMajor,
    /// Native GRIB scan order; skips the logical-to-scan reordering pass.
    GribScanOrder,
}

/// Builder for a single GRIB2 field.
#[derive(Debug, Clone, Default)]
pub struct Grib2FieldBuilder {
    discipline: u8,
    identification: Option<Identification>,
    grid: Option<GridDefinition>,
    product: Option<ProductDefinition>,
    packing: Option<PackingStrategy>,
    values: Option<Vec<f64>>,
    bitmap: Option<Vec<bool>>,
    value_order: ValueOrder,
}

impl Grib2FieldBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn discipline(mut self, discipline: u8) -> Self {
        self.discipline = discipline;
        self
    }

    pub fn identification(mut self, identification: Identification) -> Self {
        self.identification = Some(identification);
        self
    }

    pub fn grid(mut self, grid: GridDefinition) -> Self {
        self.grid = Some(grid);
        self
    }

    pub fn product(mut self, product: ProductDefinition) -> Self {
        self.product = Some(product);
        self
    }

    pub fn packing(mut self, packing: PackingStrategy) -> Self {
        self.packing = Some(packing);
        self
    }

    pub fn values<T>(mut self, values: &[T]) -> Self
    where
        T: Copy + Into<f64>,
    {
        self.values = Some(values.iter().copied().map(Into::into).collect());
        self
    }

    pub fn bitmap(mut self, bitmap: &[bool]) -> Self {
        self.bitmap = Some(bitmap.to_vec());
        self
    }

    pub fn value_order(mut self, value_order: ValueOrder) -> Self {
        self.value_order = value_order;
        self
    }

    pub fn build(self) -> Result<Grib2Field> {
        let identification = self
            .identification
            .ok_or_else(|| Error::Other("missing GRIB2 identification".into()))?;
        let grid = self
            .grid
            .ok_or_else(|| Error::Other("missing GRIB2 grid definition".into()))?;
        let product = self
            .product
            .ok_or_else(|| Error::Other("missing GRIB2 product definition".into()))?;
        let packing = self
            .packing
            .ok_or_else(|| Error::Other("missing GRIB2 packing strategy".into()))?;
        let mut values = self
            .values
            .ok_or_else(|| Error::Other("missing GRIB2 field values".into()))?;
        let mut bitmap = self.bitmap;

        validate_supported_grid(&grid)?;
        validate_supported_product(&product)?;

        let expected = checked_grid_point_count(&grid)?;
        if values.len() != expected {
            return Err(Error::DataLengthMismatch {
                expected,
                actual: values.len(),
            });
        }
        if let Some(bitmap) = &bitmap {
            if bitmap.len() != expected {
                return Err(Error::DataLengthMismatch {
                    expected,
                    actual: bitmap.len(),
                });
            }
        }

        if self.value_order == ValueOrder::LogicalRowMajor {
            reorder_field_to_grib_scan_order(&grid, &mut values, bitmap.as_deref_mut())?;
        }

        let packed = match packing {
            PackingStrategy::SimpleAuto { decimal_scale } => {
                pack_simple_auto(&values, bitmap.as_deref(), decimal_scale)?
            }
        };

        Ok(Grib2Field {
            discipline: self.discipline,
            identification,
            grid,
            product,
            packed,
        })
    }
}

/// A validated, packed GRIB2 field ready for message serialization.
#[derive(Debug, Clone)]
pub struct Grib2Field {
    discipline: u8,
    identification: Identification,
    grid: GridDefinition,
    product: ProductDefinition,
    packed: PackedField,
}

impl Grib2Field {
    pub fn discipline(&self) -> u8 {
        self.discipline
    }

    pub fn identification(&self) -> &Identification {
        &self.identification
    }

    pub fn grid(&self) -> &GridDefinition {
        &self.grid
    }

    pub fn product(&self) -> &ProductDefinition {
        &self.product
    }

    pub fn data_representation(&self) -> &DataRepresentation {
        &self.packed.representation
    }
}

/// Streaming GRIB writer.
pub struct GribWriter<'a, W> {
    out: &'a mut W,
}

impl<'a, W: Write> GribWriter<'a, W> {
    pub fn new(out: &'a mut W) -> Self {
        Self { out }
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
            if field.discipline != first.discipline {
                return Err(Error::InvalidMessage(
                    "all fields in a GRIB2 message must share a discipline".into(),
                ));
            }
            if field.identification != first.identification {
                return Err(Error::InvalidMessage(
                    "all fields in a GRIB2 message must share Section 1 identification".into(),
                ));
            }
        }

        let mut message = Vec::new();
        write_indicator_placeholder(&mut message, first.discipline)?;
        write_identification_section(&mut message, &first.identification)?;
        let mut current_grid = None;
        for field in &fields {
            if current_grid != Some(&field.grid) {
                write_grid_section(&mut message, &field.grid)?;
                current_grid = Some(&field.grid);
            }
            write_product_section(&mut message, &field.product)?;
            write_data_representation_section(&mut message, &field.packed)?;
            if let Some(bitmap) = &field.packed.bitmap_payload {
                write_bitmap_section(&mut message, bitmap)?;
            }
            write_data_section(&mut message, &field.packed.data_payload)?;
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

#[derive(Debug, Clone)]
struct PackedField {
    representation: DataRepresentation,
    bitmap_payload: Option<Vec<u8>>,
    data_payload: Vec<u8>,
}

fn pack_simple_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    decimal_scale: i16,
) -> Result<PackedField> {
    let present = present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };

    let decimal_factor = 10.0_f64.powi(i32::from(decimal_scale));
    if !decimal_factor.is_finite() || decimal_factor <= 0.0 {
        return Err(Error::Other(format!(
            "invalid decimal scale for simple packing: {decimal_scale}"
        )));
    }

    let quantized = values
        .iter()
        .zip(&present)
        .filter_map(|(value, present)| present.then_some(*value))
        .map(|value| {
            if !value.is_finite() {
                return Err(Error::Other(
                    "present values must be finite for simple packing".into(),
                ));
            }
            let scaled = value * decimal_factor;
            if !scaled.is_finite() {
                return Err(Error::Other(
                    "scaled value overflow during simple packing".into(),
                ));
            }
            Ok(scaled.round())
        })
        .collect::<Result<Vec<_>>>()?;

    let (reference_value, deltas) = simple_packing_deltas(&quantized)?;
    let max_delta = deltas.iter().copied().max().unwrap_or(0);
    let bits_per_value = if max_delta == 0 {
        0
    } else {
        (u64::BITS - max_delta.leading_zeros()) as u8
    };

    let mut writer = BitWriter::with_capacity_bits(deltas.len() * usize::from(bits_per_value));
    if bits_per_value > 0 {
        for delta in &deltas {
            writer.write(*delta, usize::from(bits_per_value))?;
        }
        writer.align_to_byte()?;
    }

    let representation = DataRepresentation::SimplePacking(SimplePackingParams {
        encoded_values: present_count,
        reference_value,
        binary_scale: 0,
        decimal_scale,
        bits_per_value,
        original_field_type: 0,
    });

    Ok(PackedField {
        representation,
        bitmap_payload,
        data_payload: writer.into_bytes(),
    })
}

fn reorder_field_to_grib_scan_order(
    grid: &GridDefinition,
    values: &mut [f64],
    bitmap: Option<&mut [bool]>,
) -> Result<()> {
    match grid {
        GridDefinition::LatLon(grid) => {
            grid.reorder_ndarray_to_grib_scan_in_place(values)?;
            if let Some(bitmap) = bitmap {
                grid.reorder_ndarray_to_grib_scan_in_place(bitmap)?;
            }
            Ok(())
        }
        GridDefinition::Unsupported(template) => Err(Error::UnsupportedGridTemplate(*template)),
    }
}

fn present_mask(values: &[f64], explicit_bitmap: Option<&[bool]>) -> Result<Vec<bool>> {
    match explicit_bitmap {
        Some(bitmap) => values
            .iter()
            .zip(bitmap)
            .map(|(value, present)| {
                if *present && !value.is_finite() {
                    return Err(Error::Other(
                        "explicit bitmap marks a non-finite value as present".into(),
                    ));
                }
                Ok(*present)
            })
            .collect(),
        None => values
            .iter()
            .map(|value| {
                if value.is_nan() {
                    Ok(false)
                } else if value.is_finite() {
                    Ok(true)
                } else {
                    Err(Error::Other(
                        "infinite values cannot be written as GRIB2 simple-packed data".into(),
                    ))
                }
            })
            .collect(),
    }
}

fn simple_packing_deltas(quantized: &[f64]) -> Result<(f32, Vec<u64>)> {
    if quantized.is_empty() {
        return Ok((0.0, Vec::new()));
    }

    let min_value = quantized.iter().copied().fold(f64::INFINITY, f64::min);
    let reference_value = f32_not_greater_than(min_value)
        .ok_or_else(|| Error::Other("failed to choose simple-packing reference value".into()))?;
    let reference = f64::from(reference_value);

    let mut deltas = Vec::with_capacity(quantized.len());
    for value in quantized {
        let delta = (value - reference).round();
        if !delta.is_finite() || delta < 0.0 || delta > u64::MAX as f64 {
            return Err(Error::Other(
                "packed simple-packing delta does not fit in u64".into(),
            ));
        }
        deltas.push(delta as u64);
    }

    Ok((reference_value, deltas))
}

fn f32_not_greater_than(value: f64) -> Option<f32> {
    if !value.is_finite() || value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return None;
    }

    let mut candidate = value as f32;
    while f64::from(candidate) > value {
        candidate = next_down_f32(candidate)?;
    }
    Some(candidate)
}

fn next_down_f32(value: f32) -> Option<f32> {
    if value.is_nan() || value == f32::NEG_INFINITY {
        return None;
    }
    if value == 0.0 {
        return Some(-f32::from_bits(1));
    }
    let bits = value.to_bits();
    Some(if value.is_sign_positive() {
        f32::from_bits(bits - 1)
    } else {
        f32::from_bits(bits + 1)
    })
}

fn pack_bitmap(present: &[bool]) -> Result<Vec<u8>> {
    let mut writer = BitWriter::with_capacity_bits(present.len());
    for present in present {
        writer.write(u64::from(*present), 1)?;
    }
    writer.align_to_byte()?;
    Ok(writer.into_bytes())
}

fn write_indicator_placeholder(out: &mut Vec<u8>, discipline: u8) -> Result<()> {
    out.extend_from_slice(b"GRIB");
    write_u16_be(out, 0)?;
    write_u8_be(out, discipline)?;
    write_u8_be(out, 2)?;
    write_u64_be(out, 0)
}

fn write_identification_section(out: &mut Vec<u8>, identification: &Identification) -> Result<()> {
    write_u32_be(out, 21)?;
    write_u8_be(out, 1)?;
    write_u16_be(out, identification.center_id)?;
    write_u16_be(out, identification.subcenter_id)?;
    write_u8_be(out, identification.master_table_version)?;
    write_u8_be(out, identification.local_table_version)?;
    write_u8_be(out, identification.significance_of_reference_time)?;
    write_u16_be(out, identification.reference_year)?;
    write_u8_be(out, identification.reference_month)?;
    write_u8_be(out, identification.reference_day)?;
    write_u8_be(out, identification.reference_hour)?;
    write_u8_be(out, identification.reference_minute)?;
    write_u8_be(out, identification.reference_second)?;
    write_u8_be(out, identification.production_status)?;
    write_u8_be(out, identification.processed_data_type)
}

fn write_grid_section(out: &mut Vec<u8>, grid: &GridDefinition) -> Result<()> {
    let GridDefinition::LatLon(grid) = grid else {
        return Err(Error::UnsupportedGridTemplate(match grid {
            GridDefinition::Unsupported(template) => *template,
            GridDefinition::LatLon(_) => unreachable!(),
        }));
    };

    let mut section = vec![0u8; 72];
    section[..4].copy_from_slice(&72u32.to_be_bytes());
    section[4] = 3;
    section[6..10].copy_from_slice(&checked_latlon_point_count(grid)?.to_be_bytes());
    section[12..14].copy_from_slice(&0u16.to_be_bytes());
    section[30..34].copy_from_slice(&grid.ni.to_be_bytes());
    section[34..38].copy_from_slice(&grid.nj.to_be_bytes());
    section[46..50].copy_from_slice(&encode_wmo_i32(grid.lat_first).ok_or_else(|| {
        Error::Other("latitude of first grid point does not fit GRIB signed i32".into())
    })?);
    section[50..54].copy_from_slice(&encode_wmo_i32(grid.lon_first).ok_or_else(|| {
        Error::Other("longitude of first grid point does not fit GRIB signed i32".into())
    })?);
    section[55..59].copy_from_slice(&encode_wmo_i32(grid.lat_last).ok_or_else(|| {
        Error::Other("latitude of last grid point does not fit GRIB signed i32".into())
    })?);
    section[59..63].copy_from_slice(&encode_wmo_i32(grid.lon_last).ok_or_else(|| {
        Error::Other("longitude of last grid point does not fit GRIB signed i32".into())
    })?);
    section[63..67].copy_from_slice(&grid.di.to_be_bytes());
    section[67..71].copy_from_slice(&grid.dj.to_be_bytes());
    section[71] = grid.scanning_mode;
    out.extend_from_slice(&section);
    Ok(())
}

fn write_product_section(out: &mut Vec<u8>, product: &ProductDefinition) -> Result<()> {
    let ProductDefinitionTemplate::AnalysisOrForecast(template) = &product.template;

    write_u32_be(out, 34)?;
    write_u8_be(out, 4)?;
    write_u16_be(out, 0)?;
    write_u16_be(out, 0)?;
    write_u8_be(out, product.parameter_category)?;
    write_u8_be(out, product.parameter_number)?;
    write_u8_be(out, template.generating_process)?;
    write_u8_be(out, 0)?;
    write_u8_be(out, 0)?;
    write_u16_be(out, 0)?;
    write_u8_be(out, 0)?;
    write_u8_be(out, template.forecast_time_unit)?;
    write_u32_be(out, template.forecast_time)?;
    write_surface(out, template.first_surface.as_ref())?;
    write_surface(out, template.second_surface.as_ref())
}

fn write_surface(out: &mut Vec<u8>, surface: Option<&FixedSurface>) -> Result<()> {
    match surface {
        Some(surface) => {
            write_u8_be(out, surface.surface_type)?;
            write_u8_be(
                out,
                encode_wmo_i8(surface.scale_factor).ok_or_else(|| {
                    Error::Other("fixed-surface scale factor does not fit GRIB signed i8".into())
                })?,
            )?;
            out.extend_from_slice(&encode_wmo_i32(surface.scaled_value).ok_or_else(|| {
                Error::Other("fixed-surface scaled value does not fit GRIB signed i32".into())
            })?);
            Ok(())
        }
        None => {
            write_u8_be(out, 255)?;
            out.extend_from_slice(&[0xff; 5]);
            Ok(())
        }
    }
}

fn write_data_representation_section(out: &mut Vec<u8>, packed: &PackedField) -> Result<()> {
    let DataRepresentation::SimplePacking(params) = &packed.representation else {
        return Err(Error::UnsupportedDataTemplate(0));
    };

    let encoded_values = u32::try_from(params.encoded_values)
        .map_err(|_| Error::Other("encoded value count exceeds u32".into()))?;
    write_u32_be(out, 21)?;
    write_u8_be(out, 5)?;
    write_u32_be(out, encoded_values)?;
    write_u16_be(out, 0)?;
    out.extend_from_slice(&params.reference_value.to_be_bytes());
    out.extend_from_slice(
        &encode_wmo_i16(params.binary_scale)
            .ok_or_else(|| Error::Other("binary scale does not fit GRIB signed i16".into()))?,
    );
    out.extend_from_slice(
        &encode_wmo_i16(params.decimal_scale)
            .ok_or_else(|| Error::Other("decimal scale does not fit GRIB signed i16".into()))?,
    );
    write_u8_be(out, params.bits_per_value)?;
    write_u8_be(out, params.original_field_type)
}

fn write_bitmap_section(out: &mut Vec<u8>, bitmap_payload: &[u8]) -> Result<()> {
    let length = checked_section_length(6usize + bitmap_payload.len(), 6)?;
    write_u32_be(out, length)?;
    write_u8_be(out, 6)?;
    write_u8_be(out, 0)?;
    out.extend_from_slice(bitmap_payload);
    Ok(())
}

fn write_data_section(out: &mut Vec<u8>, data_payload: &[u8]) -> Result<()> {
    let length = checked_section_length(5usize + data_payload.len(), 7)?;
    write_u32_be(out, length)?;
    write_u8_be(out, 7)?;
    out.extend_from_slice(data_payload);
    Ok(())
}

fn checked_section_length(length: usize, section: u8) -> Result<u32> {
    u32::try_from(length).map_err(|_| Error::InvalidSection {
        section,
        reason: format!("section length {length} exceeds u32"),
    })
}

fn checked_grid_point_count(grid: &GridDefinition) -> Result<usize> {
    match grid {
        GridDefinition::LatLon(grid) => Ok(checked_latlon_point_count(grid)? as usize),
        GridDefinition::Unsupported(template) => Err(Error::UnsupportedGridTemplate(*template)),
    }
}

fn checked_latlon_point_count(grid: &LatLonGrid) -> Result<u32> {
    let count = u64::from(grid.ni)
        .checked_mul(u64::from(grid.nj))
        .ok_or_else(|| Error::Other("grid point count overflow".into()))?;
    u32::try_from(count).map_err(|_| Error::Other("grid point count exceeds u32".into()))
}

fn validate_supported_grid(grid: &GridDefinition) -> Result<()> {
    match grid {
        GridDefinition::LatLon(grid) => grid.validate_supported_scan_order(),
        GridDefinition::Unsupported(template) => Err(Error::UnsupportedGridTemplate(*template)),
    }
}

fn validate_supported_product(product: &ProductDefinition) -> Result<()> {
    match product.template {
        ProductDefinitionTemplate::AnalysisOrForecast(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Grib2FieldBuilder, GribWriter, PackingStrategy, ValueOrder};
    use std::process::Command;

    use grib_core::{
        AnalysisOrForecastTemplate, DataRepresentation, FixedSurface, GridDefinition,
        Identification, LatLonGrid, ProductDefinition, ProductDefinitionTemplate,
    };
    use grib_reader::sections::scan_sections;
    use grib_reader::GribFile;
    use serde::Deserialize;

    fn identification() -> Identification {
        Identification {
            center_id: 7,
            subcenter_id: 0,
            master_table_version: 35,
            local_table_version: 1,
            significance_of_reference_time: 1,
            reference_year: 2026,
            reference_month: 3,
            reference_day: 20,
            reference_hour: 12,
            reference_minute: 0,
            reference_second: 0,
            production_status: 0,
            processed_data_type: 1,
        }
    }

    fn grid() -> GridDefinition {
        grid_with_shape_and_scanning_mode(2, 2, 0)
    }

    fn grid_with_scanning_mode(scanning_mode: u8) -> GridDefinition {
        grid_with_shape_and_scanning_mode(3, 2, scanning_mode)
    }

    fn grid_with_shape_and_scanning_mode(ni: u32, nj: u32, scanning_mode: u8) -> GridDefinition {
        let lon_first = -120_000_000;
        let lat_first = 50_000_000;
        let di = 1_000_000;
        let dj = 1_000_000;
        let i_step = if scanning_mode & 0b1000_0000 == 0 {
            di as i32
        } else {
            -(di as i32)
        };
        let j_step = if scanning_mode & 0b0100_0000 != 0 {
            dj as i32
        } else {
            -(dj as i32)
        };

        GridDefinition::LatLon(LatLonGrid {
            ni,
            nj,
            lat_first,
            lon_first,
            lat_last: lat_first + (nj.saturating_sub(1) as i32) * j_step,
            lon_last: lon_first + (ni.saturating_sub(1) as i32) * i_step,
            di,
            dj,
            scanning_mode,
        })
    }

    fn product(parameter_category: u8, parameter_number: u8) -> ProductDefinition {
        ProductDefinition {
            parameter_category,
            parameter_number,
            template: ProductDefinitionTemplate::AnalysisOrForecast(AnalysisOrForecastTemplate {
                generating_process: 2,
                forecast_time_unit: 1,
                forecast_time: 6,
                first_surface: Some(FixedSurface {
                    surface_type: 103,
                    scale_factor: 0,
                    scaled_value: 850,
                }),
                second_surface: None,
            }),
        }
    }

    fn write_message(fields: impl IntoIterator<Item = super::Grib2Field>) -> Vec<u8> {
        let mut bytes = Vec::new();
        GribWriter::new(&mut bytes)
            .write_grib2_message(fields)
            .unwrap();
        bytes
    }

    fn section_numbers(bytes: &[u8]) -> Vec<u8> {
        scan_sections(bytes)
            .unwrap()
            .iter()
            .map(|section| section.number)
            .collect()
    }

    fn simple_field(
        values: &[f64],
        parameter_category: u8,
        parameter_number: u8,
    ) -> super::Grib2Field {
        Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(parameter_category, parameter_number))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(values)
            .build()
            .unwrap()
    }

    #[test]
    fn writes_simple_grib2_field_readable_by_reader() {
        let values = [1.0, 2.0, 3.0, 4.0];
        let field = simple_field(&values, 0, 0);

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let message = file.message(0).unwrap();
        assert_eq!(message.parameter_name(), "TMP");
        assert_eq!(message.grid_shape(), (2, 2));
        assert_eq!(message.forecast_time(), Some(6));
        assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
    }

    #[test]
    fn writes_constant_field_with_zero_width_simple_packing() {
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[42.0, 42.0, 42.0, 42.0])
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let message = file.message(0).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::SimplePacking(params) => assert_eq!(params.bits_per_value, 0),
            other => panic!("expected simple packing, got {other:?}"),
        }
        assert_eq!(message.read_flat_data_as_f64().unwrap(), vec![42.0; 4]);
    }

    #[test]
    fn writes_decimal_scaled_values_within_quantization_tolerance() {
        let values = [1.2, 2.3, 3.4, 4.5];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 1 })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        for (actual, expected) in decoded.iter().zip(values) {
            assert!((actual - expected).abs() <= 0.05);
        }
    }

    #[test]
    fn writes_bitmap_from_nan_values() {
        let values = [1.0, f64::NAN, 3.0, 4.0];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[0], 1.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 3.0);
        assert_eq!(decoded[3], 4.0);
    }

    #[test]
    fn writes_bitmap_from_explicit_mask() {
        let values = [1.0, 999.0, 3.0, 4.0];
        let bitmap = [true, false, true, true];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&values)
            .bitmap(&bitmap)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[0], 1.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 3.0);
        assert_eq!(decoded[3], 4.0);
    }

    #[test]
    fn writes_all_missing_bitmap_field() {
        let values = [f64::NAN; 4];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        assert!(decoded.iter().all(|value| value.is_nan()));
    }

    #[test]
    fn writes_single_grib2_message_with_multiple_fields() {
        let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
        let second = simple_field(&[5.0, 6.0, 7.0, 8.0], 0, 2);

        let bytes = write_message([first, second]);
        assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 7, 4, 5, 7, 8]);

        let file = GribFile::from_bytes(bytes).unwrap();
        assert_eq!(file.message_count(), 2);
        assert_eq!(file.message(0).unwrap().parameter_name(), "TMP");
        assert_eq!(file.message(1).unwrap().parameter_name(), "POT");
        assert_eq!(file.message(0).unwrap().grid_shape(), (2, 2));
        assert_eq!(file.message(1).unwrap().grid_shape(), (2, 2));
        assert_eq!(
            file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
            vec![1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(
            file.message(1).unwrap().read_flat_data_as_f64().unwrap(),
            vec![5.0, 6.0, 7.0, 8.0]
        );
    }

    #[test]
    fn emits_new_grid_section_only_when_grid_changes() {
        let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
        let second = simple_field(&[5.0, 6.0, 7.0, 8.0], 0, 2);
        let third = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_shape_and_scanning_mode(3, 2, 0))
            .product(product(0, 4))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[9.0, 10.0, 11.0, 12.0, 13.0, 14.0])
            .build()
            .unwrap();

        let bytes = write_message([first, second, third]);
        assert_eq!(
            section_numbers(&bytes),
            vec![1, 3, 4, 5, 7, 4, 5, 7, 3, 4, 5, 7, 8]
        );

        let file = GribFile::from_bytes(bytes).unwrap();
        assert_eq!(file.message_count(), 3);
        assert_eq!(file.message(0).unwrap().parameter_name(), "TMP");
        assert_eq!(file.message(1).unwrap().parameter_name(), "POT");
        assert_eq!(file.message(2).unwrap().parameter_name(), "TMAX");
        assert_eq!(file.message(0).unwrap().grid_shape(), (2, 2));
        assert_eq!(file.message(1).unwrap().grid_shape(), (2, 2));
        assert_eq!(file.message(2).unwrap().grid_shape(), (3, 2));
        assert_eq!(
            file.message(2).unwrap().read_flat_data_as_f64().unwrap(),
            vec![9.0, 10.0, 11.0, 12.0, 13.0, 14.0]
        );
    }

    #[test]
    fn writes_reused_grid_multifield_message_with_bitmap() {
        let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
        let second = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 2))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[5.0, f64::NAN, 7.0, 8.0])
            .build()
            .unwrap();

        let bytes = write_message([first, second]);
        assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 7, 4, 5, 6, 7, 8]);

        let file = GribFile::from_bytes(bytes).unwrap();
        assert_eq!(file.message_count(), 2);
        let decoded = file.message(1).unwrap().read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[0], 5.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 7.0);
        assert_eq!(decoded[3], 8.0);
    }

    #[test]
    fn roundtrips_logical_row_major_order_for_supported_scan_modes() {
        let logical = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        for scanning_mode in [
            0b0000_0000,
            0b1000_0000,
            0b0100_0000,
            0b1100_0000,
            0b0001_0000,
            0b1001_0000,
        ] {
            let field = Grib2FieldBuilder::new()
                .identification(identification())
                .grid(grid_with_scanning_mode(scanning_mode))
                .product(product(0, 0))
                .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
                .values(&logical)
                .build()
                .unwrap();

            let file = GribFile::from_bytes(write_message([field])).unwrap();
            assert_eq!(
                file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
                logical,
                "scanning mode {scanning_mode:08b}"
            );
        }
    }

    #[test]
    fn accepts_grib_scan_order_fast_path() {
        let scan_order = [1.0, 2.0, 3.0, 6.0, 5.0, 4.0];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_scanning_mode(0b0001_0000))
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&scan_order)
            .value_order(ValueOrder::GribScanOrder)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        assert_eq!(
            file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn reorders_explicit_bitmap_with_logical_values() {
        let values = [1.0, 2.0, 3.0, 4.0, 999.0, 6.0];
        let bitmap = [true, true, true, true, false, true];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_scanning_mode(0b0001_0000))
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&values)
            .bitmap(&bitmap)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[..4], [1.0, 2.0, 3.0, 4.0]);
        assert!(decoded[4].is_nan());
        assert_eq!(decoded[5], 6.0);
    }

    #[test]
    fn rejects_unsupported_scan_mode_before_writing() {
        let err = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_scanning_mode(0b0010_0000))
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .build()
            .unwrap_err();

        assert!(matches!(
            err,
            grib_core::Error::UnsupportedScanningMode(0b0010_0000)
        ));
    }

    #[test]
    fn rejects_value_count_mismatch_before_writing() {
        let err = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[1.0, 2.0, 3.0])
            .build()
            .unwrap_err();
        assert!(matches!(
            err,
            grib_core::Error::DataLengthMismatch {
                expected: 4,
                actual: 3
            }
        ));
    }

    #[derive(Debug, Deserialize)]
    struct ReferenceDump {
        messages: Vec<ReferenceMessage>,
    }

    #[derive(Debug, Deserialize)]
    struct ReferenceMessage {
        edition: u8,
        name: String,
        values: Vec<Option<f64>>,
    }

    #[test]
    #[ignore = "requires GRIB_READER_ECCODES_HELPER"]
    fn generated_grib2_fixture_matches_eccodes_when_configured() {
        let helper = std::env::var_os("GRIB_READER_ECCODES_HELPER")
            .expect("GRIB_READER_ECCODES_HELPER must be set");
        let first = simple_field(&[1.0, 2.0, 3.0, 4.0], 0, 0);
        let second = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 2))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[5.0, f64::NAN, 7.0, 8.0])
            .build()
            .unwrap();
        let bytes = write_message([first, second]);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("writer-generated.grib2");
        std::fs::write(&path, &bytes).unwrap();

        let output = Command::new(helper)
            .arg("dump")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "ecCodes helper failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let reference: ReferenceDump = serde_json::from_slice(&output.stdout).unwrap();
        let rust = GribFile::from_bytes(bytes).unwrap();

        assert_eq!(reference.messages.len(), 2);
        assert_eq!(rust.message_count(), reference.messages.len());
        for (index, expected) in reference.messages.iter().enumerate() {
            let message = rust.message(index).unwrap();
            let actual = message.read_flat_data_as_f64().unwrap();
            assert_eq!(message.edition(), expected.edition);
            assert_eq!(message.parameter_description(), expected.name);
            assert_eq!(actual.len(), expected.values.len());
            for (actual, expected) in actual.iter().zip(&expected.values) {
                match expected {
                    Some(expected) => assert!((actual - expected).abs() <= 1e-6),
                    None => assert!(actual.is_nan()),
                }
            }
        }
    }
}
