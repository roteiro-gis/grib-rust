//! Shared GRIB model, primitives, validation helpers, and code tables.

#![forbid(unsafe_code)]

mod allocation;
pub mod binary;
pub mod bit;
pub mod data;
pub mod error;
pub mod grib1;
pub mod grid;
pub mod metadata;
pub mod parameter;
pub mod product;

pub use allocation::{ensure_limit, filled_vec};
pub use data::{
    ComplexPackingParams, DataRepresentation, ImagePackingParams, Jpeg2000PackingParams,
    PngPackingParams, SimplePackingParams, SpatialDifferencingParams,
};
pub use error::{Error, Result};
pub use grid::{
    AlbersEqualAreaGrid, GridDefinition, LambertConformalGrid, LatLonGrid, MercatorGrid,
    PolarStereographicGrid, ProjectedGridCore, RegularGaussianGrid, RotatedLatLonGrid,
};
pub use metadata::{ForecastTimeUnit, Parameter, ParameterTableSource, ReferenceTime};
pub use parameter::{
    LocalParameterEntry, LocalParameterTable, OwnedLocalParameterEntry, BUILTIN_LOCAL_PARAMETERS,
    LOCAL_PARAMETER_TABLE_CSV_HEADER,
};
pub use product::{
    AnalysisOrForecastTemplate, DerivedForecastTemplate, DerivedStatisticalProcessTemplate,
    EnsembleStatisticalProcessTemplate, FixedSurface, Identification,
    IndividualEnsembleForecastTemplate, PercentileForecastTemplate,
    PercentileStatisticalProcessTemplate, ProbabilityForecastTemplate, ProbabilityLimit,
    ProbabilityStatisticalProcessTemplate, ProbabilityType, ProductDefinition,
    ProductDefinitionTemplate, ScaledValue, StatisticalInterval, StatisticalProcessTemplate,
    StatisticalTimeRange,
};
