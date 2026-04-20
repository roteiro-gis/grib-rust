//! Shared GRIB model, primitives, validation helpers, and code tables.

#![forbid(unsafe_code)]

pub mod bit;
pub mod data;
pub mod error;
pub mod grib1;
pub mod grid;
pub mod metadata;
pub mod parameter;
pub mod product;
pub mod util;

pub use data::{
    ComplexPackingParams, DataRepresentation, SimplePackingParams, SpatialDifferencingParams,
};
pub use error::{Error, Result};
pub use grid::{GridDefinition, LatLonGrid};
pub use metadata::{ForecastTimeUnit, Parameter, ReferenceTime};
pub use product::{
    AnalysisOrForecastTemplate, FixedSurface, Identification, ProductDefinition,
    ProductDefinitionTemplate,
};
