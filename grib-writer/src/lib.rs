//! GRIB writer crate.

#![forbid(unsafe_code)]

use std::io::Write;

use grib_core::binary::{
    encode_ibm_f32, encode_wmo_i16, encode_wmo_i24, encode_wmo_i32, encode_wmo_i8, write_u16_be,
    write_u24_be, write_u32_be, write_u64_be, write_u8_be, U24_MAX,
};
use grib_core::bit::BitWriter;
use grib_core::{
    ComplexPackingParams, DataRepresentation, FixedSurface, GridDefinition, Identification,
    ImagePackingParams, Jpeg2000PackingParams, LatLonGrid, PngPackingParams, ProductDefinition,
    ProductDefinitionTemplate, SimplePackingParams, SpatialDifferencingParams,
};

pub use grib_core::grib1::ProductDefinition as Grib1ProductDefinition;
pub use grib_core::{Error, Result};

/// Field packing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackingStrategy {
    /// Simple packing with binary scale 0 and automatic bit-width selection.
    SimpleAuto { decimal_scale: i16 },
    /// GRIB2 complex packing with fixed-size general groups.
    ComplexAuto {
        decimal_scale: i16,
        spatial_differencing: Option<SpatialDifferencingOrder>,
    },
    /// GRIB2 JPEG 2000 code stream packing template 5.40.
    Jpeg2000Auto { decimal_scale: i16 },
    /// GRIB2 PNG image packing template 5.41.
    PngAuto { decimal_scale: i16 },
}

/// Spatial differencing order for GRIB2 complex packing template 5.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialDifferencingOrder {
    /// First-order spatial differencing.
    First,
    /// Second-order spatial differencing.
    Second,
}

const COMPLEX_AUTO_GROUP_LEN: usize = 32;

/// Order of values supplied to field builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueOrder {
    /// Logical row-major order matching `grib-reader` ndarray output.
    #[default]
    LogicalRowMajor,
    /// Native GRIB scan order; skips the logical-to-scan reordering pass.
    GribScanOrder,
}

/// Builder for a single GRIB1 message field.
#[derive(Debug, Clone, Default)]
pub struct Grib1FieldBuilder {
    product: Option<Grib1ProductDefinition>,
    grid: Option<GridDefinition>,
    packing: Option<PackingStrategy>,
    values: Option<Vec<f64>>,
    bitmap: Option<Vec<bool>>,
    value_order: ValueOrder,
}

impl Grib1FieldBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn product(mut self, product: Grib1ProductDefinition) -> Self {
        self.product = Some(product);
        self
    }

    pub fn grid(mut self, grid: GridDefinition) -> Self {
        self.grid = Some(grid);
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

    pub fn build(self) -> Result<Grib1Field> {
        let mut product = self
            .product
            .ok_or_else(|| Error::Other("missing GRIB1 product definition".into()))?;
        let grid = self
            .grid
            .ok_or_else(|| Error::Other("missing GRIB1 grid definition".into()))?;
        let packing = self
            .packing
            .ok_or_else(|| Error::Other("missing GRIB1 packing strategy".into()))?;
        let mut values = self
            .values
            .ok_or_else(|| Error::Other("missing GRIB1 field values".into()))?;
        let mut bitmap = self.bitmap;

        validate_supported_grib1_grid(&grid)?;

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
                product.decimal_scale = decimal_scale;
                pack_simple_auto(&values, bitmap.as_deref(), decimal_scale)?
            }
            PackingStrategy::ComplexAuto { .. } => {
                return Err(Error::Other(
                    "GRIB1 writer does not support complex packing".into(),
                ));
            }
            PackingStrategy::Jpeg2000Auto { .. } => {
                return Err(Error::Other(
                    "GRIB1 writer does not support JPEG2000 packing".into(),
                ));
            }
            PackingStrategy::PngAuto { .. } => {
                return Err(Error::Other(
                    "GRIB1 writer does not support PNG packing".into(),
                ));
            }
        };
        product.has_grid_definition = true;
        product.has_bitmap = packed.bitmap_payload.is_some();

        Ok(Grib1Field {
            product,
            grid,
            packed,
        })
    }
}

/// A validated, packed GRIB1 field ready for message serialization.
#[derive(Debug, Clone)]
pub struct Grib1Field {
    product: Grib1ProductDefinition,
    grid: GridDefinition,
    packed: PackedField,
}

impl Grib1Field {
    pub fn product(&self) -> &Grib1ProductDefinition {
        &self.product
    }

    pub fn grid(&self) -> &GridDefinition {
        &self.grid
    }

    pub fn data_representation(&self) -> &DataRepresentation {
        &self.packed.representation
    }
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
            PackingStrategy::ComplexAuto {
                decimal_scale,
                spatial_differencing,
            } => pack_complex_auto(
                &values,
                bitmap.as_deref(),
                decimal_scale,
                spatial_differencing,
            )?,
            PackingStrategy::Jpeg2000Auto { decimal_scale } => {
                pack_jpeg2000_auto(&values, bitmap.as_deref(), &grid, decimal_scale)?
            }
            PackingStrategy::PngAuto { decimal_scale } => {
                pack_png_auto(&values, bitmap.as_deref(), &grid, decimal_scale)?
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

    pub fn write_grib1_message(&mut self, field: Grib1Field) -> Result<()> {
        let mut body = Vec::new();
        write_grib1_product_section(&mut body, &field.product)?;
        write_grib1_grid_section(&mut body, &field.grid)?;
        if let Some(bitmap) = &field.packed.bitmap_payload {
            write_grib1_bitmap_section(&mut body, bitmap, field.grid.num_points())?;
        }
        write_grib1_data_section(&mut body, &field.packed, 0)?;

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

    let quantized = quantize_present_values(values, &present, decimal_scale, "simple packing")?;
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

fn pack_complex_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    decimal_scale: i16,
    spatial_differencing: Option<SpatialDifferencingOrder>,
) -> Result<PackedField> {
    let present = present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };

    let quantized = quantize_present_values(values, &present, decimal_scale, "complex packing")?;
    let (reference_value, deltas) = simple_packing_deltas(&quantized)?;
    let spatial_packing = spatial_differencing
        .map(|order| spatially_difference_values(&deltas, order))
        .transpose()?;
    let packed_values = spatial_packing
        .as_ref()
        .map_or(deltas.as_slice(), |spatial| spatial.values.as_slice());
    let groups = complex_groups(packed_values)?;
    let max_group_reference = groups
        .iter()
        .map(|group| group.reference)
        .max()
        .unwrap_or(0);
    let max_group_width = groups.iter().map(|group| group.width).max().unwrap_or(0);
    let group_reference_bits = bits_needed(max_group_reference)?;
    let group_width_bits = bits_needed(u64::from(max_group_width))?;
    let group_length_reference = complex_group_length_reference(present_count)?;
    let true_length_last_group = complex_true_length_last_group(present_count)?;

    let mut writer = BitWriter::new();
    if let Some(spatial) = &spatial_packing {
        write_spatial_descriptors(&mut writer, spatial)?;
    }
    for group in &groups {
        writer.write(group.reference, usize::from(group_reference_bits))?;
    }
    writer.align_to_byte()?;
    for group in &groups {
        writer.write(u64::from(group.width), usize::from(group_width_bits))?;
    }
    writer.align_to_byte()?;
    for group in &groups {
        for value in &group.values {
            writer.write(
                value
                    .checked_sub(group.reference)
                    .ok_or_else(|| Error::Other("complex group value underflow".into()))?,
                usize::from(group.width),
            )?;
        }
    }
    writer.align_to_byte()?;

    let representation = DataRepresentation::ComplexPacking(ComplexPackingParams {
        encoded_values: present_count,
        reference_value,
        binary_scale: 0,
        decimal_scale,
        group_reference_bits,
        original_field_type: 0,
        group_splitting_method: 1,
        missing_value_management: 0,
        primary_missing_substitute: u32::MAX,
        secondary_missing_substitute: u32::MAX,
        num_groups: groups.len(),
        group_width_reference: 0,
        group_width_bits,
        group_length_reference,
        group_length_increment: 1,
        true_length_last_group,
        scaled_group_length_bits: 0,
        spatial_differencing: spatial_packing.as_ref().map(|spatial| spatial.params),
    });

    Ok(PackedField {
        representation,
        bitmap_payload,
        data_payload: writer.into_bytes(),
    })
}

#[cfg(feature = "jpeg2000")]
fn pack_jpeg2000_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    grid: &GridDefinition,
    decimal_scale: i16,
) -> Result<PackedField> {
    let prepared = prepare_image_packing(
        values,
        explicit_bitmap,
        grid,
        decimal_scale,
        "JPEG2000 packing",
        jpeg2000_bits_per_value,
    )?;
    let data_payload = encode_jpeg2000_payload(
        &prepared.deltas,
        prepared.params.bits_per_value,
        prepared.dimensions,
    )?;

    Ok(PackedField {
        representation: DataRepresentation::Jpeg2000Packing(Jpeg2000PackingParams {
            packing: prepared.params,
            compression_type: 0,
            target_compression_ratio: 0,
        }),
        bitmap_payload: prepared.bitmap_payload,
        data_payload,
    })
}

#[cfg(not(feature = "jpeg2000"))]
fn pack_jpeg2000_auto(
    _values: &[f64],
    _explicit_bitmap: Option<&[bool]>,
    _grid: &GridDefinition,
    _decimal_scale: i16,
) -> Result<PackedField> {
    Err(Error::UnsupportedDataTemplate(40))
}

#[cfg(feature = "png")]
fn pack_png_auto(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    grid: &GridDefinition,
    decimal_scale: i16,
) -> Result<PackedField> {
    let prepared = prepare_image_packing(
        values,
        explicit_bitmap,
        grid,
        decimal_scale,
        "PNG packing",
        png_bits_per_value,
    )?;
    let data_payload = encode_png_payload(
        &prepared.deltas,
        prepared.params.bits_per_value,
        prepared.dimensions,
    )?;

    Ok(PackedField {
        representation: DataRepresentation::PngPacking(PngPackingParams {
            packing: prepared.params,
        }),
        bitmap_payload: prepared.bitmap_payload,
        data_payload,
    })
}

#[cfg(not(feature = "png"))]
fn pack_png_auto(
    _values: &[f64],
    _explicit_bitmap: Option<&[bool]>,
    _grid: &GridDefinition,
    _decimal_scale: i16,
) -> Result<PackedField> {
    Err(Error::UnsupportedDataTemplate(41))
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
#[derive(Debug, Clone)]
struct PreparedImagePacking {
    params: ImagePackingParams,
    bitmap_payload: Option<Vec<u8>>,
    deltas: Vec<u64>,
    dimensions: ImageDimensions,
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImageDimensions {
    width: u32,
    height: u32,
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn prepare_image_packing(
    values: &[f64],
    explicit_bitmap: Option<&[bool]>,
    grid: &GridDefinition,
    decimal_scale: i16,
    packing_name: &str,
    select_bits_per_value: fn(u8) -> Result<u8>,
) -> Result<PreparedImagePacking> {
    let present = present_mask(values, explicit_bitmap)?;
    let present_count = present.iter().filter(|present| **present).count();
    if present_count == 0 {
        return Err(Error::Other(format!(
            "{packing_name} requires at least one present value"
        )));
    }

    let bitmap_payload = if present.iter().any(|present| !*present) {
        Some(pack_bitmap(&present)?)
    } else {
        None
    };

    let quantized = quantize_present_values(values, &present, decimal_scale, packing_name)?;
    let (reference_value, deltas) = simple_packing_deltas(&quantized)?;
    let max_delta = deltas.iter().copied().max().unwrap_or(0);
    let minimum_bits = bits_needed(max_delta)?.max(1);
    let bits_per_value = select_bits_per_value(minimum_bits)?;
    validate_image_deltas_fit(&deltas, bits_per_value)?;

    Ok(PreparedImagePacking {
        params: ImagePackingParams {
            encoded_values: present_count,
            reference_value,
            binary_scale: 0,
            decimal_scale,
            bits_per_value,
            original_field_type: 0,
        },
        bitmap_payload,
        deltas,
        dimensions: image_dimensions(grid, values.len(), present_count)?,
    })
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn image_dimensions(
    grid: &GridDefinition,
    total_values: usize,
    present_count: usize,
) -> Result<ImageDimensions> {
    if present_count == total_values {
        let Some(grid) = grid.as_lat_lon() else {
            return Err(Error::UnsupportedGridTemplate(grid.template_number()));
        };
        return Ok(ImageDimensions {
            width: grid.ni,
            height: grid.nj,
        });
    }

    Ok(ImageDimensions {
        width: u32::try_from(present_count)
            .map_err(|_| Error::Other("image width exceeds u32".into()))?,
        height: 1,
    })
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn validate_image_deltas_fit(deltas: &[u64], bits_per_value: u8) -> Result<()> {
    let max_value = if bits_per_value == u64::BITS as u8 {
        u64::MAX
    } else {
        (1u64 << bits_per_value) - 1
    };
    if deltas.iter().any(|delta| *delta > max_value) {
        return Err(Error::UnsupportedPackingWidth(bits_per_value));
    }
    Ok(())
}

#[cfg(feature = "jpeg2000")]
fn jpeg2000_bits_per_value(minimum_bits: u8) -> Result<u8> {
    if (1..=31).contains(&minimum_bits) {
        Ok(minimum_bits)
    } else {
        Err(Error::UnsupportedPackingWidth(minimum_bits))
    }
}

#[cfg(feature = "png")]
fn png_bits_per_value(minimum_bits: u8) -> Result<u8> {
    match minimum_bits {
        0 | 1 => Ok(1),
        2 => Ok(2),
        3 | 4 => Ok(4),
        5..=8 => Ok(8),
        9..=16 => Ok(16),
        17..=24 => Ok(24),
        25..=32 => Ok(32),
        bits => Err(Error::UnsupportedPackingWidth(bits)),
    }
}

#[cfg(feature = "png")]
fn encode_png_payload(
    deltas: &[u64],
    bits_per_value: u8,
    dimensions: ImageDimensions,
) -> Result<Vec<u8>> {
    validate_image_sample_count(deltas.len(), dimensions)?;
    let (color_type, bit_depth, image_data) = png_image_data(deltas, bits_per_value, dimensions)?;

    let mut payload = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut payload, dimensions.width, dimensions.height);
        encoder.set_color(color_type);
        encoder.set_depth(bit_depth);
        let mut writer = encoder
            .write_header()
            .map_err(|err| Error::Other(format!("PNG encode failed: {err}")))?;
        writer
            .write_image_data(&image_data)
            .map_err(|err| Error::Other(format!("PNG encode failed: {err}")))?;
    }
    Ok(payload)
}

#[cfg(feature = "png")]
fn png_image_data(
    deltas: &[u64],
    bits_per_value: u8,
    dimensions: ImageDimensions,
) -> Result<(png::ColorType, png::BitDepth, Vec<u8>)> {
    match bits_per_value {
        1 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::One,
            pack_png_subbyte_rows(deltas, dimensions, 1)?,
        )),
        2 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::Two,
            pack_png_subbyte_rows(deltas, dimensions, 2)?,
        )),
        4 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::Four,
            pack_png_subbyte_rows(deltas, dimensions, 4)?,
        )),
        8 => Ok((
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            deltas
                .iter()
                .map(|delta| u8::try_from(*delta))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?,
        )),
        16 => {
            let mut data = Vec::with_capacity(deltas.len() * 2);
            for delta in deltas {
                data.extend_from_slice(
                    &u16::try_from(*delta)
                        .map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?
                        .to_be_bytes(),
                );
            }
            Ok((png::ColorType::Grayscale, png::BitDepth::Sixteen, data))
        }
        24 => Ok((
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            pack_png_multibyte_samples(deltas, 3)?,
        )),
        32 => Ok((
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            pack_png_multibyte_samples(deltas, 4)?,
        )),
        bits => Err(Error::UnsupportedPackingWidth(bits)),
    }
}

#[cfg(feature = "png")]
fn pack_png_subbyte_rows(
    deltas: &[u64],
    dimensions: ImageDimensions,
    bits_per_value: u8,
) -> Result<Vec<u8>> {
    let width =
        usize::try_from(dimensions.width).map_err(|_| Error::Other("PNG width overflow".into()))?;
    let height = usize::try_from(dimensions.height)
        .map_err(|_| Error::Other("PNG height overflow".into()))?;
    let bits = usize::from(bits_per_value);
    let row_bits = width
        .checked_mul(bits)
        .ok_or_else(|| Error::Other("PNG row width overflow".into()))?;
    let row_bytes = row_bits.div_ceil(8);
    let mut data = vec![
        0;
        row_bytes
            .checked_mul(height)
            .ok_or_else(|| Error::Other("PNG data length overflow".into()))?
    ];

    for (index, delta) in deltas.iter().copied().enumerate() {
        let row = index / width;
        let column = index % width;
        let bit_offset = column
            .checked_mul(bits)
            .ok_or_else(|| Error::Other("PNG bit offset overflow".into()))?;
        let byte_index = row
            .checked_mul(row_bytes)
            .and_then(|row_offset| row_offset.checked_add(bit_offset / 8))
            .ok_or_else(|| Error::Other("PNG byte offset overflow".into()))?;
        let shift = 8 - bits - (bit_offset % 8);
        data[byte_index] |= (delta as u8) << shift;
    }

    Ok(data)
}

#[cfg(feature = "png")]
fn pack_png_multibyte_samples(deltas: &[u64], bytes_per_sample: usize) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(
        deltas
            .len()
            .checked_mul(bytes_per_sample)
            .ok_or_else(|| Error::Other("PNG data length overflow".into()))?,
    );
    for delta in deltas {
        let bytes = u32::try_from(*delta)
            .map_err(|_| Error::UnsupportedPackingWidth((bytes_per_sample * 8) as u8))?
            .to_be_bytes();
        data.extend_from_slice(&bytes[4 - bytes_per_sample..]);
    }
    Ok(data)
}

#[cfg(feature = "jpeg2000")]
fn encode_jpeg2000_payload(
    deltas: &[u64],
    bits_per_value: u8,
    dimensions: ImageDimensions,
) -> Result<Vec<u8>> {
    validate_image_sample_count(deltas.len(), dimensions)?;

    let component = openjp2::opj_image_comptparm {
        dx: 1,
        dy: 1,
        w: dimensions.width,
        h: dimensions.height,
        prec: u32::from(bits_per_value),
        bpp: u32::from(bits_per_value),
        sgnd: 0,
        ..Default::default()
    };
    let mut image = openjp2::opj_image::create(&[component], openjp2::OPJ_CLRSPC_GRAY)
        .ok_or_else(|| Error::Other("failed to create JPEG2000 image".into()))?;
    image.x1 = dimensions.width;
    image.y1 = dimensions.height;

    let components = image
        .comps_mut()
        .ok_or_else(|| Error::Other("JPEG2000 image has no components".into()))?;
    let component = components
        .get_mut(0)
        .ok_or_else(|| Error::Other("JPEG2000 image has no components".into()))?;
    component.bpp = u32::from(bits_per_value);
    component.prec = u32::from(bits_per_value);
    let data = components
        .get_mut(0)
        .and_then(|component| component.data_mut())
        .ok_or_else(|| Error::Other("JPEG2000 image component has no data".into()))?;
    if data.len() != deltas.len() {
        return Err(Error::DataLengthMismatch {
            expected: deltas.len(),
            actual: data.len(),
        });
    }
    for (target, delta) in data.iter_mut().zip(deltas) {
        *target =
            i32::try_from(*delta).map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?;
    }

    let path = tempfile::Builder::new()
        .prefix("grib-writer-")
        .suffix(".j2k")
        .tempfile()
        .map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))?
        .into_temp_path();

    {
        let mut stream = openjp2::Stream::new_file(&path, 64 * 1024, false)
            .map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))?;
        let mut codec = openjp2::Codec::new_encoder(openjp2::OPJ_CODEC_J2K)
            .ok_or_else(|| Error::Other("failed to create JPEG2000 encoder".into()))?;
        let mut params = openjp2::opj_cparameters_t {
            tcp_numlayers: 1,
            cp_disto_alloc: 1,
            numresolution: jpeg2000_num_resolutions(dimensions),
            ..Default::default()
        };

        if codec.setup_encoder(&mut params, &mut image) == 0 {
            return Err(Error::Other("JPEG2000 encoder setup failed".into()));
        }
        if codec.start_compress(&mut image, &mut stream) == 0 {
            return Err(Error::Other("JPEG2000 start-compress failed".into()));
        }
        if codec.encode(&mut stream) == 0 {
            return Err(Error::Other("JPEG2000 codestream encode failed".into()));
        }
        if codec.end_compress(&mut stream) == 0 {
            return Err(Error::Other("JPEG2000 end-compress failed".into()));
        }
        stream
            .flush()
            .map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))?;
    }

    std::fs::read(&path).map_err(|err| Error::Io(err, "JPEG2000 temporary codestream".into()))
}

#[cfg(feature = "jpeg2000")]
fn jpeg2000_num_resolutions(dimensions: ImageDimensions) -> i32 {
    let min_dimension = dimensions.width.min(dimensions.height);
    let mut resolutions = 1;
    while resolutions < 32 && min_dimension >= (1u32 << resolutions) {
        resolutions += 1;
    }
    resolutions
}

#[cfg(any(feature = "jpeg2000", feature = "png"))]
fn validate_image_sample_count(sample_count: usize, dimensions: ImageDimensions) -> Result<()> {
    let width = usize::try_from(dimensions.width)
        .map_err(|_| Error::Other("image width overflow".into()))?;
    let height = usize::try_from(dimensions.height)
        .map_err(|_| Error::Other("image height overflow".into()))?;
    let expected = width
        .checked_mul(height)
        .ok_or_else(|| Error::Other("image sample count overflow".into()))?;
    if sample_count != expected {
        return Err(Error::DataLengthMismatch {
            expected,
            actual: sample_count,
        });
    }
    Ok(())
}

fn quantize_present_values(
    values: &[f64],
    present: &[bool],
    decimal_scale: i16,
    packing_name: &str,
) -> Result<Vec<f64>> {
    let decimal_factor = 10.0_f64.powi(i32::from(decimal_scale));
    if !decimal_factor.is_finite() || decimal_factor <= 0.0 {
        return Err(Error::Other(format!(
            "invalid decimal scale for {packing_name}: {decimal_scale}"
        )));
    }

    values
        .iter()
        .zip(present)
        .filter_map(|(value, present)| present.then_some(*value))
        .map(|value| {
            if !value.is_finite() {
                return Err(Error::Other(format!(
                    "present values must be finite for {packing_name}"
                )));
            }
            let scaled = value * decimal_factor;
            if !scaled.is_finite() {
                return Err(Error::Other(format!(
                    "scaled value overflow during {packing_name}"
                )));
            }
            Ok(scaled.round())
        })
        .collect()
}

impl SpatialDifferencingOrder {
    const fn grib_order(self) -> u8 {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }

    const fn min_values(self) -> usize {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }
}

#[derive(Debug, Clone)]
struct SpatialPacking {
    params: SpatialDifferencingParams,
    descriptors: SpatialDescriptors,
    values: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
struct SpatialDescriptors {
    first_value: i64,
    second_value: Option<i64>,
    overall_minimum: i64,
}

fn spatially_difference_values(
    values: &[u64],
    order: SpatialDifferencingOrder,
) -> Result<SpatialPacking> {
    if values.len() < order.min_values() {
        return Err(Error::DataLengthMismatch {
            expected: order.min_values(),
            actual: values.len(),
        });
    }

    let values = values
        .iter()
        .copied()
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| Error::Other("spatial differencing value exceeds i64".into()))
        })
        .collect::<Result<Vec<_>>>()?;

    let (descriptors, differenced) = match order {
        SpatialDifferencingOrder::First => first_order_spatial_difference(&values)?,
        SpatialDifferencingOrder::Second => second_order_spatial_difference(&values)?,
    };
    let descriptor_octets = spatial_descriptor_octets(&descriptors)?;

    Ok(SpatialPacking {
        params: SpatialDifferencingParams {
            order: order.grib_order(),
            descriptor_octets,
        },
        descriptors,
        values: differenced,
    })
}

fn first_order_spatial_difference(values: &[i64]) -> Result<(SpatialDescriptors, Vec<u64>)> {
    let mut differences = Vec::with_capacity(values.len().saturating_sub(1));
    for pair in values.windows(2) {
        differences.push(
            pair[1]
                .checked_sub(pair[0])
                .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?,
        );
    }
    let overall_minimum = differences.iter().copied().min().unwrap_or(0);

    let mut differenced = Vec::with_capacity(values.len());
    differenced.push(0);
    for difference in differences {
        differenced.push(spatial_difference_delta(difference, overall_minimum)?);
    }

    Ok((
        SpatialDescriptors {
            first_value: values[0],
            second_value: None,
            overall_minimum,
        },
        differenced,
    ))
}

fn second_order_spatial_difference(values: &[i64]) -> Result<(SpatialDescriptors, Vec<u64>)> {
    let first_value = values[0];
    let second_value = values[1];
    let mut previous_difference = second_value
        .checked_sub(first_value)
        .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
    let mut second_differences = Vec::with_capacity(values.len().saturating_sub(2));

    for index in 2..values.len() {
        let difference = values[index]
            .checked_sub(values[index - 1])
            .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
        second_differences.push(
            difference
                .checked_sub(previous_difference)
                .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?,
        );
        previous_difference = difference;
    }

    let overall_minimum = second_differences.iter().copied().min().unwrap_or(0);
    let mut differenced = Vec::with_capacity(values.len());
    differenced.push(0);
    differenced.push(0);
    for second_difference in second_differences {
        differenced.push(spatial_difference_delta(
            second_difference,
            overall_minimum,
        )?);
    }

    Ok((
        SpatialDescriptors {
            first_value,
            second_value: Some(second_value),
            overall_minimum,
        },
        differenced,
    ))
}

fn spatial_difference_delta(value: i64, overall_minimum: i64) -> Result<u64> {
    let delta = value
        .checked_sub(overall_minimum)
        .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
    u64::try_from(delta)
        .map_err(|_| Error::Other("spatial differencing produced negative delta".into()))
}

fn spatial_descriptor_octets(descriptors: &SpatialDescriptors) -> Result<u8> {
    let values = [
        Some(descriptors.first_value),
        descriptors.second_value,
        Some(descriptors.overall_minimum),
    ];
    for octets in 1..=8 {
        if values
            .iter()
            .flatten()
            .all(|value| signed_magnitude_fits(*value, octets))
        {
            return Ok(octets);
        }
    }

    Err(Error::Other(
        "spatial differencing descriptor exceeds signed-magnitude range".into(),
    ))
}

fn signed_magnitude_fits(value: i64, octets: u8) -> bool {
    signed_magnitude_bits(value, octets).is_ok()
}

fn write_spatial_descriptors(writer: &mut BitWriter, spatial: &SpatialPacking) -> Result<()> {
    let bit_count = usize::from(spatial.params.descriptor_octets) * 8;
    writer.write(
        signed_magnitude_bits(
            spatial.descriptors.first_value,
            spatial.params.descriptor_octets,
        )?,
        bit_count,
    )?;
    if let Some(second_value) = spatial.descriptors.second_value {
        writer.write(
            signed_magnitude_bits(second_value, spatial.params.descriptor_octets)?,
            bit_count,
        )?;
    }
    writer.write(
        signed_magnitude_bits(
            spatial.descriptors.overall_minimum,
            spatial.params.descriptor_octets,
        )?,
        bit_count,
    )
}

fn signed_magnitude_bits(value: i64, octets: u8) -> Result<u64> {
    let bit_count = u32::from(octets) * 8;
    if bit_count == 0 || bit_count > u64::BITS {
        return Err(Error::Other(
            "spatial differencing descriptor width must be 1..=8 octets".into(),
        ));
    }
    let magnitude = value
        .checked_abs()
        .ok_or_else(|| Error::Other("spatial differencing descriptor magnitude overflow".into()))?
        as u64;
    let magnitude_bits = bit_count - 1;
    let max_magnitude = if magnitude_bits == u64::BITS {
        u64::MAX
    } else {
        (1u64 << magnitude_bits) - 1
    };
    if magnitude > max_magnitude {
        return Err(Error::Other(
            "spatial differencing descriptor exceeds signed-magnitude range".into(),
        ));
    }

    let sign_bit = if value < 0 {
        1u64 << (bit_count - 1)
    } else {
        0
    };
    Ok(sign_bit | magnitude)
}

fn reorder_field_to_grib_scan_order(
    grid: &GridDefinition,
    values: &mut [f64],
    bitmap: Option<&mut [bool]>,
) -> Result<()> {
    if let Some(grid) = grid.as_lat_lon() {
        grid.reorder_for_ndarray_in_place(values)?;
        if let Some(bitmap) = bitmap {
            grid.reorder_for_ndarray_in_place(bitmap)?;
        }
        Ok(())
    } else {
        Err(Error::UnsupportedGridTemplate(grid.template_number()))
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
                        "infinite values cannot be written as packed data".into(),
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

#[derive(Debug, Clone)]
struct ComplexGroup {
    reference: u64,
    width: u8,
    values: Vec<u64>,
}

fn complex_groups(deltas: &[u64]) -> Result<Vec<ComplexGroup>> {
    if deltas.is_empty() {
        return Ok(vec![ComplexGroup {
            reference: 0,
            width: 0,
            values: Vec::new(),
        }]);
    }

    let group_len = complex_group_len(deltas.len());
    let mut groups = Vec::with_capacity(deltas.len().div_ceil(group_len));
    for chunk in deltas.chunks(group_len) {
        let reference = chunk.iter().copied().min().unwrap_or(0);
        let max_value = chunk.iter().copied().max().unwrap_or(reference);
        if max_value > i64::MAX as u64 {
            return Err(Error::Other(
                "complex packing value exceeds i64 decoder range".into(),
            ));
        }
        let width = bits_needed(max_value - reference)?;
        groups.push(ComplexGroup {
            reference,
            width,
            values: chunk.to_vec(),
        });
    }
    Ok(groups)
}

fn complex_group_length_reference(value_count: usize) -> Result<u32> {
    u32::try_from(complex_group_len(value_count))
        .map_err(|_| Error::Other("complex group length exceeds u32".into()))
}

fn complex_true_length_last_group(value_count: usize) -> Result<u32> {
    if value_count == 0 {
        return Ok(0);
    }

    let group_len = complex_group_len(value_count);
    let remainder = value_count % group_len;
    let length = if remainder == 0 { group_len } else { remainder };
    u32::try_from(length).map_err(|_| Error::Other("complex group length exceeds u32".into()))
}

fn complex_group_len(value_count: usize) -> usize {
    COMPLEX_AUTO_GROUP_LEN.min(value_count)
}

fn bits_needed(value: u64) -> Result<u8> {
    let bits = if value == 0 {
        0
    } else {
        u64::BITS - value.leading_zeros()
    };
    u8::try_from(bits).map_err(|_| Error::Other("bit width exceeds u8".into()))
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

fn write_grib1_product_section(out: &mut Vec<u8>, product: &Grib1ProductDefinition) -> Result<()> {
    let (year_of_century, century) = grib1_reference_year_fields(product.reference_time.year)?;

    write_u24_be(out, 28)?;
    write_u8_be(out, product.table_version)?;
    write_u8_be(out, product.center_id)?;
    write_u8_be(out, product.generating_process_id)?;
    write_u8_be(out, product.grid_id)?;
    let mut flags = 0b1000_0000;
    if product.has_bitmap {
        flags |= 0b0100_0000;
    }
    write_u8_be(out, flags)?;
    write_u8_be(out, product.parameter_number)?;
    write_u8_be(out, product.level_type)?;
    write_u16_be(out, product.level_value)?;
    write_u8_be(out, year_of_century)?;
    write_u8_be(out, product.reference_time.month)?;
    write_u8_be(out, product.reference_time.day)?;
    write_u8_be(out, product.reference_time.hour)?;
    write_u8_be(out, product.reference_time.minute)?;
    write_u8_be(out, product.forecast_time_unit)?;
    write_u8_be(out, product.p1)?;
    write_u8_be(out, product.p2)?;
    write_u8_be(out, product.time_range_indicator)?;
    write_u16_be(out, product.average_count)?;
    write_u8_be(out, product.missing_count)?;
    write_u8_be(out, century)?;
    write_u8_be(out, product.subcenter_id)?;
    out.extend_from_slice(
        &encode_wmo_i16(product.decimal_scale)
            .ok_or_else(|| Error::Other("decimal scale does not fit GRIB signed i16".into()))?,
    );
    Ok(())
}

fn write_grib1_grid_section(out: &mut Vec<u8>, grid: &GridDefinition) -> Result<()> {
    let Some(grid) = grid.as_lat_lon() else {
        return Err(Error::UnsupportedGridTemplate(grid.template_number()));
    };

    write_u24_be(out, 32)?;
    write_u8_be(out, 0)?;
    write_u8_be(out, 255)?;
    write_u8_be(out, 0)?;
    write_u16_be(out, checked_grib1_grid_dimension(grid.ni, "Ni")?)?;
    write_u16_be(out, checked_grib1_grid_dimension(grid.nj, "Nj")?)?;
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lat_first,
        "latitude of first grid point",
    )?);
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lon_first,
        "longitude of first grid point",
    )?);
    write_u8_be(out, 0x80)?;
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lat_last,
        "latitude of last grid point",
    )?);
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lon_last,
        "longitude of last grid point",
    )?);
    write_u16_be(
        out,
        checked_grib1_increment(grid.di, "i direction increment")?,
    )?;
    write_u16_be(
        out,
        checked_grib1_increment(grid.dj, "j direction increment")?,
    )?;
    write_u8_be(out, grid.scanning_mode)?;
    out.extend_from_slice(&[0; 4]);
    Ok(())
}

fn write_grib1_bitmap_section(
    out: &mut Vec<u8>,
    bitmap_payload: &[u8],
    num_points: usize,
) -> Result<()> {
    let length = checked_grib1_u24_length(6usize + bitmap_payload.len(), 3)?;
    write_u24_be(out, length)?;
    write_u8_be(out, unused_bits_for_width(num_points, 1)?)?;
    write_u16_be(out, 0)?;
    out.extend_from_slice(bitmap_payload);
    Ok(())
}

fn write_grib1_data_section(out: &mut Vec<u8>, packed: &PackedField, flags: u8) -> Result<()> {
    validate_grib1_binary_data_flags(flags)?;
    let DataRepresentation::SimplePacking(params) = &packed.representation else {
        return Err(Error::UnsupportedDataTemplate(1004));
    };

    let length = checked_grib1_u24_length(11usize + packed.data_payload.len(), 4)?;
    write_u24_be(out, length)?;
    let unused_bits = unused_bits_for_width(params.encoded_values, params.bits_per_value)?;
    write_u8_be(out, (flags << 4) | unused_bits)?;
    out.extend_from_slice(
        &encode_wmo_i16(params.binary_scale)
            .ok_or_else(|| Error::Other("binary scale does not fit GRIB signed i16".into()))?,
    );
    out.extend_from_slice(
        &encode_ibm_f32(params.reference_value)
            .ok_or_else(|| Error::Other("reference value does not fit GRIB1 IBM float".into()))?,
    );
    write_u8_be(out, params.bits_per_value)?;
    out.extend_from_slice(&packed.data_payload);
    Ok(())
}

fn validate_grib1_binary_data_flags(flags: u8) -> Result<()> {
    if flags == 0 {
        return Ok(());
    }
    if flags > 0x0f {
        return Err(Error::Other(
            "GRIB1 binary data flags must fit in four bits".into(),
        ));
    }
    let template = if flags & 0b1000 != 0 {
        1004
    } else if flags & 0b0100 != 0 {
        1005
    } else if flags & 0b0010 != 0 {
        1006
    } else {
        1007
    };
    Err(Error::UnsupportedDataTemplate(template))
}

fn unused_bits_for_width(values: usize, bits_per_value: u8) -> Result<u8> {
    let bits = values
        .checked_mul(usize::from(bits_per_value))
        .ok_or_else(|| Error::Other("packed bit count overflow".into()))?;
    Ok(((8 - (bits % 8)) % 8) as u8)
}

fn grib1_reference_year_fields(year: u16) -> Result<(u8, u8)> {
    if year == 0 {
        return Err(Error::Other(
            "GRIB1 reference year 0 cannot be encoded".into(),
        ));
    }

    let century = ((year - 1) / 100) + 1;
    let year_of_century = year - ((century - 1) * 100);
    Ok((
        u8::try_from(year_of_century)
            .map_err(|_| Error::Other("GRIB1 year of century exceeds u8".into()))?,
        u8::try_from(century).map_err(|_| Error::Other("GRIB1 century exceeds u8".into()))?,
    ))
}

fn encode_grib1_coordinate(value: i32, name: &str) -> Result<[u8; 3]> {
    if value % 1_000 != 0 {
        return Err(Error::Other(format!(
            "{name} must be representable in GRIB1 millidegrees"
        )));
    }
    encode_wmo_i24(value / 1_000)
        .ok_or_else(|| Error::Other(format!("{name} does not fit GRIB signed i24")))
}

fn checked_grib1_grid_dimension(value: u32, name: &str) -> Result<u16> {
    u16::try_from(value).map_err(|_| Error::Other(format!("{name} exceeds GRIB1 u16 limit")))
}

fn checked_grib1_increment(value: u32, name: &str) -> Result<u16> {
    if value % 1_000 != 0 {
        return Err(Error::Other(format!(
            "{name} must be representable in GRIB1 millidegrees"
        )));
    }
    u16::try_from(value / 1_000)
        .map_err(|_| Error::Other(format!("{name} exceeds GRIB1 u16 millidegree limit")))
}

fn checked_grib1_u24_length(length: usize, section: u8) -> Result<u32> {
    let length = u32::try_from(length).map_err(|_| Error::InvalidSection {
        section,
        reason: "GRIB1 length exceeds unsigned 24-bit limit".into(),
    })?;
    if length > U24_MAX {
        return Err(Error::InvalidSection {
            section,
            reason: format!("GRIB1 length {length} exceeds unsigned 24-bit limit"),
        });
    }
    Ok(length)
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
    let Some(grid) = grid.as_lat_lon() else {
        return Err(Error::UnsupportedGridTemplate(grid.template_number()));
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
    match &packed.representation {
        DataRepresentation::SimplePacking(params) => {
            write_simple_data_representation_section(out, params)
        }
        DataRepresentation::ComplexPacking(params) => {
            write_complex_data_representation_section(out, params)
        }
        DataRepresentation::Jpeg2000Packing(params) => {
            write_jpeg2000_data_representation_section(out, params)
        }
        DataRepresentation::PngPacking(params) => {
            write_png_data_representation_section(out, params)
        }
        DataRepresentation::Unsupported(template) => Err(Error::UnsupportedDataTemplate(*template)),
    }
}

fn write_simple_data_representation_section(
    out: &mut Vec<u8>,
    params: &SimplePackingParams,
) -> Result<()> {
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

fn write_complex_data_representation_section(
    out: &mut Vec<u8>,
    params: &ComplexPackingParams,
) -> Result<()> {
    let encoded_values = u32::try_from(params.encoded_values)
        .map_err(|_| Error::Other("encoded value count exceeds u32".into()))?;
    let num_groups = u32::try_from(params.num_groups)
        .map_err(|_| Error::Other("complex group count exceeds u32".into()))?;
    let template = if params.spatial_differencing.is_some() {
        3
    } else {
        2
    };
    let section_length = if params.spatial_differencing.is_some() {
        49
    } else {
        47
    };

    write_u32_be(out, section_length)?;
    write_u8_be(out, 5)?;
    write_u32_be(out, encoded_values)?;
    write_u16_be(out, template)?;
    out.extend_from_slice(&params.reference_value.to_be_bytes());
    out.extend_from_slice(
        &encode_wmo_i16(params.binary_scale)
            .ok_or_else(|| Error::Other("binary scale does not fit GRIB signed i16".into()))?,
    );
    out.extend_from_slice(
        &encode_wmo_i16(params.decimal_scale)
            .ok_or_else(|| Error::Other("decimal scale does not fit GRIB signed i16".into()))?,
    );
    write_u8_be(out, params.group_reference_bits)?;
    write_u8_be(out, params.original_field_type)?;
    write_u8_be(out, params.group_splitting_method)?;
    write_u8_be(out, params.missing_value_management)?;
    write_u32_be(out, params.primary_missing_substitute)?;
    write_u32_be(out, params.secondary_missing_substitute)?;
    write_u32_be(out, num_groups)?;
    write_u8_be(out, params.group_width_reference)?;
    write_u8_be(out, params.group_width_bits)?;
    write_u32_be(out, params.group_length_reference)?;
    write_u8_be(out, params.group_length_increment)?;
    write_u32_be(out, params.true_length_last_group)?;
    write_u8_be(out, params.scaled_group_length_bits)?;
    if let Some(spatial) = params.spatial_differencing {
        write_u8_be(out, spatial.order)?;
        write_u8_be(out, spatial.descriptor_octets)?;
    }
    Ok(())
}

fn write_jpeg2000_data_representation_section(
    out: &mut Vec<u8>,
    params: &Jpeg2000PackingParams,
) -> Result<()> {
    write_image_data_representation_base(out, 23, 40, &params.packing)?;
    write_u8_be(out, params.compression_type)?;
    write_u8_be(out, params.target_compression_ratio)
}

fn write_png_data_representation_section(
    out: &mut Vec<u8>,
    params: &PngPackingParams,
) -> Result<()> {
    write_image_data_representation_base(out, 21, 41, &params.packing)
}

fn write_image_data_representation_base(
    out: &mut Vec<u8>,
    section_length: u32,
    template: u16,
    params: &ImagePackingParams,
) -> Result<()> {
    let encoded_values = u32::try_from(params.encoded_values)
        .map_err(|_| Error::Other("encoded value count exceeds u32".into()))?;
    write_u32_be(out, section_length)?;
    write_u8_be(out, 5)?;
    write_u32_be(out, encoded_values)?;
    write_u16_be(out, template)?;
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
    if let Some(grid) = grid.as_lat_lon() {
        Ok(checked_latlon_point_count(grid)? as usize)
    } else {
        Err(Error::UnsupportedGridTemplate(grid.template_number()))
    }
}

fn checked_latlon_point_count(grid: &LatLonGrid) -> Result<u32> {
    let count = u64::from(grid.ni)
        .checked_mul(u64::from(grid.nj))
        .ok_or_else(|| Error::Other("grid point count overflow".into()))?;
    u32::try_from(count).map_err(|_| Error::Other("grid point count exceeds u32".into()))
}

fn validate_supported_grid(grid: &GridDefinition) -> Result<()> {
    if let Some(grid) = grid.as_lat_lon() {
        validate_supported_scan_order(grid)
    } else {
        Err(Error::UnsupportedGridTemplate(grid.template_number()))
    }
}

fn validate_supported_scan_order(grid: &LatLonGrid) -> Result<()> {
    if grid.scanning_mode & 0b0010_0000 == 0 {
        Ok(())
    } else {
        Err(Error::UnsupportedScanningMode(grid.scanning_mode))
    }
}

fn validate_supported_grib1_grid(grid: &GridDefinition) -> Result<()> {
    let Some(grid) = grid.as_lat_lon() else {
        return Err(Error::UnsupportedGridTemplate(grid.template_number()));
    };
    validate_supported_scan_order(grid)?;
    checked_grib1_grid_dimension(grid.ni, "Ni")?;
    checked_grib1_grid_dimension(grid.nj, "Nj")?;
    checked_grib1_increment(grid.di, "i direction increment")?;
    checked_grib1_increment(grid.dj, "j direction increment")?;
    encode_grib1_coordinate(grid.lat_first, "latitude of first grid point")?;
    encode_grib1_coordinate(grid.lon_first, "longitude of first grid point")?;
    encode_grib1_coordinate(grid.lat_last, "latitude of last grid point")?;
    encode_grib1_coordinate(grid.lon_last, "longitude of last grid point")?;
    Ok(())
}

fn validate_supported_product(product: &ProductDefinition) -> Result<()> {
    match product.template {
        ProductDefinitionTemplate::AnalysisOrForecast(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Grib1FieldBuilder, Grib1ProductDefinition, Grib2FieldBuilder, GribWriter, PackingStrategy,
        SpatialDifferencingOrder, ValueOrder,
    };
    use std::process::Command;

    use grib_core::binary::decode_ibm_f32;
    use grib_core::metadata::ReferenceTime;
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

    fn grib1_product() -> Grib1ProductDefinition {
        Grib1ProductDefinition {
            table_version: 2,
            center_id: 7,
            generating_process_id: 255,
            grid_id: 0,
            has_grid_definition: true,
            has_bitmap: false,
            parameter_number: 11,
            level_type: 100,
            level_value: 850,
            reference_time: ReferenceTime {
                year: 2026,
                month: 3,
                day: 20,
                hour: 12,
                minute: 0,
                second: 0,
            },
            forecast_time_unit: 1,
            p1: 6,
            p2: 0,
            time_range_indicator: 0,
            average_count: 0,
            missing_count: 0,
            century: 21,
            subcenter_id: 0,
            decimal_scale: 0,
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

    fn write_grib1_message(field: super::Grib1Field) -> Vec<u8> {
        let mut bytes = Vec::new();
        GribWriter::new(&mut bytes)
            .write_grib1_message(field)
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

    #[cfg(any(feature = "jpeg2000", feature = "png"))]
    fn section_payload(bytes: &[u8], section_number: u8) -> &[u8] {
        let section = scan_sections(bytes)
            .unwrap()
            .into_iter()
            .find(|section| section.number == section_number)
            .unwrap();
        &bytes[section.offset + 5..section.offset + section.length]
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

    fn grib1_simple_field(values: &[f64]) -> super::Grib1Field {
        Grib1FieldBuilder::new()
            .product(grib1_product())
            .grid(grid())
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(values)
            .build()
            .unwrap()
    }

    #[test]
    fn writes_simple_grib1_field_readable_by_reader() {
        let values = [1.0, 2.0, 3.0, 4.0];
        let bytes = write_grib1_message(grib1_simple_field(&values));

        let file = GribFile::from_bytes(bytes).unwrap();
        let message = file.message(0).unwrap();
        assert_eq!(file.edition(), 1);
        assert_eq!(file.message_count(), 1);
        assert_eq!(message.parameter_name(), "TMP");
        assert_eq!(message.grid_shape(), (2, 2));
        assert_eq!(message.forecast_time(), Some(6));
        assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
    }

    #[test]
    fn writes_grib1_bitmap_from_nan_values() {
        let values = [5.0, f64::NAN, 7.0, 8.0];
        let bytes = write_grib1_message(grib1_simple_field(&values));
        let bitmap_offset = 8 + 28 + 32;
        assert_eq!(&bytes[bitmap_offset + 4..bitmap_offset + 6], &[0, 0]);

        let file = GribFile::from_bytes(bytes).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[0], 5.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 7.0);
        assert_eq!(decoded[3], 8.0);
    }

    #[test]
    fn writes_grib1_bitmap_from_explicit_mask() {
        let field = Grib1FieldBuilder::new()
            .product(grib1_product())
            .grid(grid())
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[5.0, 999.0, 7.0, 8.0])
            .bitmap(&[true, false, true, true])
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_grib1_message(field)).unwrap();
        let decoded = file.message(0).unwrap().read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[0], 5.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 7.0);
        assert_eq!(decoded[3], 8.0);
    }

    #[test]
    fn writes_grib1_ibm_float_reference_value() {
        let bytes = write_grib1_message(grib1_simple_field(&[10.0, 11.0, 12.0, 13.0]));
        let bds_offset = 8 + 28 + 32;
        let reference = decode_ibm_f32(bytes[bds_offset + 6..bds_offset + 10].try_into().unwrap());
        assert_eq!(reference, 10.0);

        let file = GribFile::from_bytes(bytes).unwrap();
        assert_eq!(
            file.message(0).unwrap().read_flat_data_as_f64().unwrap(),
            vec![10.0, 11.0, 12.0, 13.0]
        );
    }

    #[test]
    fn rejects_grib1_u24_length_overflow() {
        let err = super::checked_grib1_u24_length(grib_core::binary::U24_MAX as usize + 1, 0)
            .unwrap_err();
        assert!(matches!(
            err,
            grib_core::Error::InvalidSection { section: 0, .. }
        ));
    }

    #[test]
    fn rejects_unsupported_grib1_binary_data_flags() {
        let err = super::validate_grib1_binary_data_flags(0b0001).unwrap_err();
        assert!(matches!(
            err,
            grib_core::Error::UnsupportedDataTemplate(1007)
        ));
    }

    #[test]
    fn rejects_grib1_grid_dimensions_beyond_u16() {
        let err = Grib1FieldBuilder::new()
            .product(grib1_product())
            .grid(GridDefinition::LatLon(LatLonGrid {
                ni: 65_536,
                nj: 1,
                lat_first: 0,
                lon_first: 0,
                lat_last: 0,
                lon_last: 0,
                di: 1_000,
                dj: 1_000,
                scanning_mode: 0,
            }))
            .packing(PackingStrategy::SimpleAuto { decimal_scale: 0 })
            .values(&[1.0])
            .build()
            .unwrap_err();
        assert!(matches!(err, grib_core::Error::Other(message) if message.contains("Ni exceeds")));
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

    #[cfg(not(feature = "png"))]
    #[test]
    fn png_packing_requires_png_feature() {
        let err = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::PngAuto { decimal_scale: 0 })
            .values(&[1.0, 2.0, 3.0, 4.0])
            .build()
            .unwrap_err();

        assert!(matches!(err, grib_core::Error::UnsupportedDataTemplate(41)));
    }

    #[cfg(not(feature = "jpeg2000"))]
    #[test]
    fn jpeg2000_packing_requires_jpeg2000_feature() {
        let err = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::Jpeg2000Auto { decimal_scale: 0 })
            .values(&[1.0, 2.0, 3.0, 4.0])
            .build()
            .unwrap_err();

        assert!(matches!(err, grib_core::Error::UnsupportedDataTemplate(40)));
    }

    #[cfg(feature = "png")]
    #[test]
    fn writes_png_grib2_field() {
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::PngAuto { decimal_scale: 0 })
            .values(&[12.0, 14.0, 16.0, 18.0])
            .build()
            .unwrap();

        match field.data_representation() {
            DataRepresentation::PngPacking(params) => {
                assert_eq!(params.packing.encoded_values, 4);
                assert_eq!(params.packing.reference_value, 12.0);
                assert_eq!(params.packing.bits_per_value, 4);
            }
            other => panic!("expected PNG packing, got {other:?}"),
        }

        let bytes = write_message([field]);
        let file = GribFile::from_bytes(bytes.clone()).unwrap();
        let message = file.message(0).unwrap();
        assert!(matches!(
            &message.metadata().data_representation,
            DataRepresentation::PngPacking(_)
        ));
        assert_eq!(
            message.read_flat_data_as_f64().unwrap(),
            vec![12.0, 14.0, 16.0, 18.0]
        );

        let payload = section_payload(&bytes, 7);
        let decoder = png::Decoder::new(std::io::Cursor::new(payload));
        let mut reader = decoder.read_info().unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        assert_eq!(info.color_type, png::ColorType::Grayscale);
        assert_eq!(info.bit_depth, png::BitDepth::Four);
        assert_eq!(&decoded[..info.buffer_size()], &[0x02, 0x46]);
    }

    #[cfg(feature = "jpeg2000")]
    #[test]
    fn writes_jpeg2000_grib2_field() {
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::Jpeg2000Auto { decimal_scale: 0 })
            .values(&[12.0, 13.0, 14.0, 15.0])
            .build()
            .unwrap();

        match field.data_representation() {
            DataRepresentation::Jpeg2000Packing(params) => {
                assert_eq!(params.packing.encoded_values, 4);
                assert_eq!(params.packing.reference_value, 12.0);
                assert_eq!(params.packing.bits_per_value, 2);
                assert_eq!(params.compression_type, 0);
                assert_eq!(params.target_compression_ratio, 0);
            }
            other => panic!("expected JPEG2000 packing, got {other:?}"),
        }

        let bytes = write_message([field]);
        let file = GribFile::from_bytes(bytes.clone()).unwrap();
        let message = file.message(0).unwrap();
        assert!(matches!(
            &message.metadata().data_representation,
            DataRepresentation::Jpeg2000Packing(_)
        ));
        assert_eq!(
            message.read_flat_data_as_f64().unwrap(),
            vec![12.0, 13.0, 14.0, 15.0]
        );

        let payload = section_payload(&bytes, 7);
        assert!(payload.starts_with(&[0xff, 0x4f, 0xff, 0x51]));
    }

    #[test]
    fn writes_complex_grib2_field_readable_by_reader() {
        let values = (0..70)
            .map(|index| f64::from((index * 37) % 113) - 50.0)
            .collect::<Vec<_>>();
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_shape_and_scanning_mode(35, 2, 0))
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: None,
            })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let message = file.message(0).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::ComplexPacking(params) => {
                assert_eq!(params.num_groups, 3);
                assert_eq!(params.group_splitting_method, 1);
                assert_eq!(params.missing_value_management, 0);
                assert_eq!(params.group_length_reference, 32);
                assert_eq!(params.true_length_last_group, 6);
                assert_eq!(params.spatial_differencing, None);
            }
            other => panic!("expected complex packing, got {other:?}"),
        }
        assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
    }

    #[test]
    fn writes_complex_grib2_decimal_scaled_values() {
        let values = [1.24, 2.34, -3.46, 4.56];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 1,
                spatial_differencing: None,
            })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let message = file.message(0).unwrap();
        assert!(matches!(
            &message.metadata().data_representation,
            DataRepresentation::ComplexPacking(_)
        ));
        let decoded = message.read_flat_data_as_f64().unwrap();
        for (actual, expected) in decoded.iter().zip(values) {
            assert!((actual - expected).abs() <= 0.05);
        }
    }

    #[test]
    fn writes_complex_grib2_bitmap_from_nan_values() {
        let values = [1.0, f64::NAN, 3.0, 4.0];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: None,
            })
            .values(&values)
            .build()
            .unwrap();

        let bytes = write_message([field]);
        assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 6, 7, 8]);

        let file = GribFile::from_bytes(bytes).unwrap();
        let message = file.message(0).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::ComplexPacking(params) => assert_eq!(params.encoded_values, 3),
            other => panic!("expected complex packing, got {other:?}"),
        }
        let decoded = message.read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[0], 1.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 3.0);
        assert_eq!(decoded[3], 4.0);
    }

    #[test]
    fn writes_all_missing_complex_grib2_bitmap_field() {
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: None,
            })
            .values(&[f64::NAN; 4])
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let message = file.message(0).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::ComplexPacking(params) => {
                assert_eq!(params.encoded_values, 0);
                assert_eq!(params.num_groups, 1);
                assert_eq!(params.true_length_last_group, 0);
            }
            other => panic!("expected complex packing, got {other:?}"),
        }
        let decoded = message.read_flat_data_as_f64().unwrap();
        assert!(decoded.iter().all(|value| value.is_nan()));
    }

    #[test]
    fn writes_first_order_spatial_differencing_grib2_field() {
        let values = (0..70)
            .map(|index| f64::from((index * index + 7 * index) % 149) - 50.0)
            .collect::<Vec<_>>();
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_shape_and_scanning_mode(35, 2, 0))
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: Some(SpatialDifferencingOrder::First),
            })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let message = file.message(0).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::ComplexPacking(params) => {
                let spatial = params.spatial_differencing.unwrap();
                assert_eq!(spatial.order, 1);
                assert!(spatial.descriptor_octets >= 1);
                assert_eq!(params.num_groups, 3);
            }
            other => panic!("expected complex packing, got {other:?}"),
        }
        assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
    }

    #[test]
    fn writes_second_order_spatial_differencing_grib2_field() {
        let values = (0..70)
            .map(|index| {
                let index = f64::from(index);
                index * index - 12.0 * index + 25.0
            })
            .collect::<Vec<_>>();
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid_with_shape_and_scanning_mode(35, 2, 0))
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: Some(SpatialDifferencingOrder::Second),
            })
            .values(&values)
            .build()
            .unwrap();

        let file = GribFile::from_bytes(write_message([field])).unwrap();
        let message = file.message(0).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::ComplexPacking(params) => {
                let spatial = params.spatial_differencing.unwrap();
                assert_eq!(spatial.order, 2);
                assert!(spatial.descriptor_octets >= 1);
                assert_eq!(params.num_groups, 3);
            }
            other => panic!("expected complex packing, got {other:?}"),
        }
        assert_eq!(message.read_flat_data_as_f64().unwrap(), values);
    }

    #[test]
    fn writes_spatial_differencing_with_bitmap_missing_values() {
        let values = [1.0, f64::NAN, 4.0, 9.0];
        let field = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: Some(SpatialDifferencingOrder::First),
            })
            .values(&values)
            .build()
            .unwrap();

        let bytes = write_message([field]);
        assert_eq!(section_numbers(&bytes), vec![1, 3, 4, 5, 6, 7, 8]);

        let file = GribFile::from_bytes(bytes).unwrap();
        let message = file.message(0).unwrap();
        match &message.metadata().data_representation {
            DataRepresentation::ComplexPacking(params) => {
                assert_eq!(params.encoded_values, 3);
                assert_eq!(params.spatial_differencing.unwrap().order, 1);
            }
            other => panic!("expected complex packing, got {other:?}"),
        }
        let decoded = message.read_flat_data_as_f64().unwrap();
        assert_eq!(decoded[0], 1.0);
        assert!(decoded[1].is_nan());
        assert_eq!(decoded[2], 4.0);
        assert_eq!(decoded[3], 9.0);
    }

    #[test]
    fn rejects_spatial_differencing_without_enough_present_values() {
        let err = Grib2FieldBuilder::new()
            .identification(identification())
            .grid(grid())
            .product(product(0, 0))
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: Some(SpatialDifferencingOrder::Second),
            })
            .values(&[1.0, f64::NAN, f64::NAN, f64::NAN])
            .build()
            .unwrap_err();

        assert!(matches!(
            err,
            grib_core::Error::DataLengthMismatch {
                expected: 2,
                actual: 1
            }
        ));
    }

    #[test]
    fn rejects_complex_packing_for_grib1() {
        let err = Grib1FieldBuilder::new()
            .product(grib1_product())
            .grid(grid())
            .packing(PackingStrategy::ComplexAuto {
                decimal_scale: 0,
                spatial_differencing: None,
            })
            .values(&[1.0, 2.0, 3.0, 4.0])
            .build()
            .unwrap_err();

        assert!(
            matches!(err, grib_core::Error::Other(message) if message.contains("GRIB1 writer does not support complex packing"))
        );
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
    fn generated_grib1_fixture_matches_eccodes_when_configured() {
        let helper = std::env::var_os("GRIB_READER_ECCODES_HELPER")
            .expect("GRIB_READER_ECCODES_HELPER must be set");
        let bytes = write_grib1_message(grib1_simple_field(&[5.0, f64::NAN, 7.0, 8.0]));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("writer-generated.grib1");
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

        assert_eq!(reference.messages.len(), 1);
        assert_eq!(rust.message_count(), reference.messages.len());
        let message = rust.message(0).unwrap();
        let actual = message.read_flat_data_as_f64().unwrap();
        let expected = &reference.messages[0];
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
