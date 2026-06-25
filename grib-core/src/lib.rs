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
pub use grid::{
    AlbersEqualAreaGrid, GridDefinition, LambertConformalGrid, LatLonGrid, MercatorGrid,
    PolarStereographicGrid,
};
pub use metadata::{ForecastTimeUnit, Parameter, ParameterTableSource, ReferenceTime};
pub use parameter::{
    LocalParameterEntry, LocalParameterTable, OwnedLocalParameterEntry, BUILTIN_LOCAL_PARAMETERS,
    LOCAL_PARAMETER_TABLE_CSV_HEADER,
};
pub use product::{
    AnalysisOrForecastTemplate, EnsembleStatisticalProcessTemplate, FixedSurface, Identification,
    IndividualEnsembleForecastTemplate, ProductDefinition, ProductDefinitionTemplate,
    StatisticalProcessTemplate, StatisticalTimeRange,
};
