//! GRIB writer crate.

#![forbid(unsafe_code)]

mod builder;
mod grib1;
mod packing;
mod sections;
mod writer;

pub use builder::{Grib1Field, Grib1FieldBuilder, Grib2Field, Grib2FieldBuilder, ValueOrder};
pub use grib_core::grib1::ProductDefinition as Grib1ProductDefinition;
pub use grib_core::{Error, Result};
pub use packing::{PackingStrategy, SpatialDifferencingOrder};
pub use writer::GribWriter;

#[cfg(test)]
mod tests;
