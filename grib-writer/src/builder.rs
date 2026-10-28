use grib_core::grib1::ProductDefinition as Grib1ProductDefinition;
use grib_core::{DataRepresentation, GridDefinition, Identification, ProductDefinition};

use crate::grib1::validate_supported_grib1_grid;
use crate::packing::{
    pack_ccsds_auto, pack_complex_auto, pack_complex_row_by_row, pack_ieee, pack_jpeg2000_auto,
    pack_png_auto, pack_simple_auto, reorder_field_to_grib_scan_order, PackedField,
    PackingStrategy,
};
use crate::sections::{
    checked_grid_point_count, validate_supported_grid, validate_supported_product,
};
use crate::{Error, Result};

/// Order of values supplied to field builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
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
    bitmap: Option<Grib1BitmapDefinition>,
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
        self.bitmap = Some(Grib1BitmapDefinition {
            present: bitmap.to_vec(),
            table_reference: None,
        });
        self
    }

    /// Use a GRIB1 predefined bitmap table reference.
    ///
    /// The bitmap is used to pack only present values. The written message
    /// contains the nonzero table reference, not the bitmap payload itself.
    pub fn predefined_bitmap(mut self, table_reference: u16, bitmap: &[bool]) -> Self {
        self.bitmap = Some(Grib1BitmapDefinition {
            present: bitmap.to_vec(),
            table_reference: Some(table_reference),
        });
        self
    }

    pub fn value_order(mut self, value_order: ValueOrder) -> Self {
        self.value_order = value_order;
        self
    }

    pub fn build(self) -> Result<Grib1Field> {
        let mut product = self.product.ok_or(Error::MissingRequiredField {
            field: "GRIB1 product definition",
        })?;
        let grid = self.grid.ok_or(Error::MissingRequiredField {
            field: "GRIB1 grid definition",
        })?;
        let packing = self.packing.ok_or(Error::MissingRequiredField {
            field: "GRIB1 packing strategy",
        })?;
        let mut values = self.values.ok_or(Error::MissingRequiredField {
            field: "GRIB1 field values",
        })?;
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
            if bitmap.present.len() != expected {
                return Err(Error::DataLengthMismatch {
                    expected,
                    actual: bitmap.present.len(),
                });
            }
            if bitmap.table_reference == Some(0) {
                return Err(Error::ValueOutOfRange(
                    "GRIB1 predefined bitmap table reference must be nonzero".into(),
                ));
            }
        }

        if self.value_order == ValueOrder::LogicalRowMajor {
            reorder_field_to_grib_scan_order(
                &grid,
                &mut values,
                bitmap.as_mut().map(|bitmap| bitmap.present.as_mut_slice()),
            )?;
        }

        let bitmap_mask = bitmap.as_ref().map(|bitmap| bitmap.present.as_slice());
        let predefined_bitmap_reference = bitmap.as_ref().and_then(|bitmap| bitmap.table_reference);

        let mut packed = match packing {
            PackingStrategy::SimpleAuto { decimal_scale } => {
                product.decimal_scale = decimal_scale;
                pack_simple_auto(&values, bitmap_mask, decimal_scale)?
            }
            PackingStrategy::ComplexAuto { .. } | PackingStrategy::ComplexRowByRow { .. } => {
                return Err(Error::UnsupportedPackingForEdition {
                    edition: 1,
                    packing: "complex",
                });
            }
            PackingStrategy::Ieee { .. } => {
                return Err(Error::UnsupportedPackingForEdition {
                    edition: 1,
                    packing: "IEEE",
                });
            }
            PackingStrategy::Jpeg2000Auto { .. } => {
                return Err(Error::UnsupportedPackingForEdition {
                    edition: 1,
                    packing: "JPEG 2000",
                });
            }
            PackingStrategy::PngAuto { .. } => {
                return Err(Error::UnsupportedPackingForEdition {
                    edition: 1,
                    packing: "PNG",
                });
            }
            PackingStrategy::CcsdsAuto { .. } => {
                return Err(Error::UnsupportedPackingForEdition {
                    edition: 1,
                    packing: "CCSDS/AEC",
                });
            }
        };
        if predefined_bitmap_reference.is_some() {
            packed.bitmap_payload = None;
        }
        product.has_grid_definition = true;
        product.has_bitmap =
            packed.bitmap_payload.is_some() || predefined_bitmap_reference.is_some();

        Ok(Grib1Field {
            product,
            grid,
            packed,
            predefined_bitmap_reference,
        })
    }
}

#[derive(Debug, Clone)]
struct Grib1BitmapDefinition {
    present: Vec<bool>,
    table_reference: Option<u16>,
}

/// A validated, packed GRIB1 field ready for message serialization.
#[derive(Debug, Clone)]
pub struct Grib1Field {
    product: Grib1ProductDefinition,
    grid: GridDefinition,
    packed: PackedField,
    predefined_bitmap_reference: Option<u16>,
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

    pub(crate) fn packed(&self) -> &PackedField {
        &self.packed
    }

    pub(crate) fn predefined_bitmap_reference(&self) -> Option<u16> {
        self.predefined_bitmap_reference
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
        let identification = self.identification.ok_or(Error::MissingRequiredField {
            field: "GRIB2 identification",
        })?;
        let grid = self.grid.ok_or(Error::MissingRequiredField {
            field: "GRIB2 grid definition",
        })?;
        let product = self.product.ok_or(Error::MissingRequiredField {
            field: "GRIB2 product definition",
        })?;
        let packing = self.packing.ok_or(Error::MissingRequiredField {
            field: "GRIB2 packing strategy",
        })?;
        let mut values = self.values.ok_or(Error::MissingRequiredField {
            field: "GRIB2 field values",
        })?;
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
            PackingStrategy::ComplexRowByRow {
                decimal_scale,
                spatial_differencing,
            } => pack_complex_row_by_row(
                &values,
                bitmap.as_deref(),
                &grid,
                decimal_scale,
                spatial_differencing,
            )?,
            PackingStrategy::Ieee { precision } => {
                pack_ieee(&values, bitmap.as_deref(), precision)?
            }
            PackingStrategy::Jpeg2000Auto { decimal_scale } => {
                pack_jpeg2000_auto(&values, bitmap.as_deref(), &grid, decimal_scale)?
            }
            PackingStrategy::PngAuto { decimal_scale } => {
                pack_png_auto(&values, bitmap.as_deref(), &grid, decimal_scale)?
            }
            PackingStrategy::CcsdsAuto {
                decimal_scale,
                flags,
                block_size,
                reference_sample_interval,
            } => pack_ccsds_auto(
                &values,
                bitmap.as_deref(),
                decimal_scale,
                flags,
                block_size,
                reference_sample_interval,
            )?,
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

    pub(crate) fn packed(&self) -> &PackedField {
        &self.packed
    }
}
