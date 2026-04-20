//! GRIB writer crate.
//!
//! Phase 1 establishes the crate boundary. Writer builders, packing planners,
//! and section serializers belong here as the encoder implementation lands.

#![forbid(unsafe_code)]

pub use grib_core::{Error, Result};
