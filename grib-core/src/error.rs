use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error reading {1}: {0}")]
    Io(#[source] std::io::Error, String),

    #[error("no GRIB messages found in file")]
    NoMessages,

    #[error("message index {0} not found")]
    MessageNotFound(usize),

    #[error("unsupported GRIB edition: {0}")]
    UnsupportedEdition(u8),

    #[error("invalid GRIB message: {0}")]
    InvalidMessage(String),

    #[error("invalid section {section}: {reason}")]
    InvalidSection { section: u8, reason: String },

    #[error("invalid section order: {0}")]
    InvalidSectionOrder(String),

    #[error("unsupported grid definition template: {0}")]
    UnsupportedGridTemplate(u16),

    #[error("unsupported data representation template: {0}")]
    UnsupportedDataTemplate(u16),

    #[error("unsupported complex packing group splitting method: {0}")]
    UnsupportedGroupSplittingMethod(u8),

    #[error("unsupported complex packing missing value management: {0}")]
    UnsupportedMissingValueManagement(u8),

    #[error("unsupported product definition template: {0}")]
    UnsupportedProductTemplate(u16),

    #[error("unsupported bitmap indicator: {0}")]
    UnsupportedBitmapIndicator(u16),

    #[error("unsupported packing width: {0} bits per value")]
    UnsupportedPackingWidth(u8),

    #[error("unsupported scanning mode: 0b{0:08b}")]
    UnsupportedScanningMode(u8),

    #[error("unsupported spatial differencing order: {0}")]
    UnsupportedSpatialDifferencingOrder(u8),

    #[error("data truncated at offset {offset}")]
    Truncated { offset: u64 },

    #[error("decoded data length mismatch: expected {expected}, got {actual}")]
    DataLengthMismatch { expected: usize, actual: usize },

    #[error("{what} limit exceeded: requested {requested}, limit {limit}")]
    LimitExceeded {
        what: &'static str,
        requested: usize,
        limit: usize,
    },

    #[error("bitmap indicates missing values but no bitmap section present")]
    MissingBitmap,

    #[error("bit offset overflow")]
    BitOffsetOverflow,

    #[error("arithmetic overflow while {operation}")]
    ArithmeticOverflow { operation: &'static str },

    #[error("value out of range: {0}")]
    ValueOutOfRange(String),

    #[error("failed to allocate {requested} {what}: {reason}")]
    AllocationFailed {
        what: String,
        requested: usize,
        reason: String,
    },

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn allocation(what: impl Into<String>, requested: usize, reason: impl ToString) -> Self {
        Self::AllocationFailed {
            what: what.into(),
            requested,
            reason: reason.to_string(),
        }
    }
}
