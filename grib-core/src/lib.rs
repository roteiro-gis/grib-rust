//! Shared GRIB model, primitives, validation helpers, and code tables.

#![forbid(unsafe_code)]

pub mod binary;
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
    ComplexPackingParams, DataRepresentation, ImagePackingParams, Jpeg2000PackingParams,
    PngPackingParams, SimplePackingParams, SpatialDifferencingParams,
};
pub use error::{Error, Result};
pub use grid::{GridDefinition, LambertConformalGrid, LatLonGrid, PolarStereographicGrid};
pub use metadata::{ForecastTimeUnit, Parameter, ParameterTableSource, ReferenceTime};
pub use parameter::{LocalParameterEntry, BUILTIN_LOCAL_PARAMETERS};
pub use product::{
    AnalysisOrForecastTemplate, FixedSurface, Identification, ProductDefinition,
    ProductDefinitionTemplate,
};
