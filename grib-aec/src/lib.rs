//! Checked buffer APIs for CCSDS 121.0-B-3 adaptive entropy coding.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

use std::ffi::{c_int, c_void};
use std::ptr;

use thiserror::Error;

const AEC_OK: c_int = 0;

/// libaec option flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AecFlags(u32);

impl AecFlags {
    /// No optional codec behavior.
    pub const NONE: Self = Self(0);
    /// Samples are signed two's-complement integers.
    pub const DATA_SIGNED: Self = Self(1 << 0);
    /// Samples with 17 through 24 bits use three-byte containers.
    pub const DATA_3BYTE: Self = Self(1 << 1);
    /// Sample containers store their most-significant byte first.
    pub const DATA_MSB: Self = Self(1 << 2);
    /// Apply the CCSDS unit-delay preprocessor.
    pub const DATA_PREPROCESS: Self = Self(1 << 3);
    /// Use the restricted code-option set for samples no wider than four bits.
    pub const RESTRICTED: Self = Self(1 << 4);
    /// Decode legacy streams whose reference sample intervals are byte padded.
    pub const PAD_RSI: Self = Self(1 << 5);
    /// Permit non-standard even block sizes.
    pub const NOT_ENFORCE: Self = Self(1 << 6);

    const KNOWN_BITS: u32 = Self::DATA_SIGNED.0
        | Self::DATA_3BYTE.0
        | Self::DATA_MSB.0
        | Self::DATA_PREPROCESS.0
        | Self::RESTRICTED.0
        | Self::PAD_RSI.0
        | Self::NOT_ENFORCE.0;

    /// Constructs flags when no unknown bits are set.
    pub const fn from_bits(bits: u32) -> Option<Self> {
        if bits & !Self::KNOWN_BITS == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    /// Returns the libaec-compatible bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether every bit in `other` is present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for AecFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for AecFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Validated codec parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AecParams {
    bits_per_sample: u8,
    block_size: u8,
    reference_sample_interval: u16,
    flags: AecFlags,
}

impl AecParams {
    /// Constructs parameters after validating the interdependent codec limits.
    pub fn new(
        bits_per_sample: u8,
        block_size: u8,
        reference_sample_interval: u16,
        flags: AecFlags,
    ) -> Result<Self, Error> {
        if !(1..=32).contains(&bits_per_sample) {
            return Err(Error::InvalidParameter(
                "bits per sample must be between 1 and 32",
            ));
        }
        if block_size == 0 || !block_size.is_multiple_of(2) {
            return Err(Error::InvalidParameter(
                "block size must be a non-zero even value",
            ));
        }
        if !flags.contains(AecFlags::NOT_ENFORCE) && !matches!(block_size, 8 | 16 | 32 | 64) {
            return Err(Error::InvalidParameter(
                "non-standard block size requires NOT_ENFORCE",
            ));
        }
        if !(1..=4096).contains(&reference_sample_interval) {
            return Err(Error::InvalidParameter(
                "reference sample interval must be between 1 and 4096",
            ));
        }
        if flags.contains(AecFlags::RESTRICTED) && bits_per_sample > 4 {
            return Err(Error::InvalidParameter(
                "RESTRICTED requires at most four bits per sample",
            ));
        }

        Ok(Self {
            bits_per_sample,
            block_size,
            reference_sample_interval,
            flags,
        })
    }

    /// Returns the encoded sample width.
    pub const fn bits_per_sample(self) -> u8 {
        self.bits_per_sample
    }

    /// Returns the number of samples in each coding block.
    pub const fn block_size(self) -> u8 {
        self.block_size
    }

    /// Returns the number of blocks between reference samples.
    pub const fn reference_sample_interval(self) -> u16 {
        self.reference_sample_interval
    }

    /// Returns the codec option flags.
    pub const fn flags(self) -> AecFlags {
        self.flags
    }

    /// Returns the byte width of each input or output sample container.
    pub const fn bytes_per_sample(self) -> usize {
        match self.bits_per_sample {
            1..=8 => 1,
            9..=16 => 2,
            17..=24 if self.flags.contains(AecFlags::DATA_3BYTE) => 3,
            17..=32 => 4,
            _ => unreachable!(),
        }
    }
}

/// AEC buffer-operation failure.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Codec parameters violate libaec's supported ranges or combinations.
    #[error("invalid AEC parameter: {0}")]
    InvalidParameter(&'static str),
    /// Input bytes do not contain whole sample containers.
    #[error("AEC input length {actual} is not a multiple of sample width {sample_width}")]
    InputLength {
        /// Supplied input length.
        actual: usize,
        /// Required sample-container width.
        sample_width: usize,
    },
    /// An input sample has non-zero bits outside its declared width.
    #[error("AEC sample {index} value {value} exceeds its {bits}-bit width")]
    SampleOutOfRange {
        /// Zero-based sample index.
        index: usize,
        /// Container value supplied by the caller.
        value: u32,
        /// Declared sample width.
        bits: u8,
    },
    /// The native codec rejected configuration, stream state, or data.
    #[error("AEC {operation} failed with {status}")]
    Codec {
        /// Operation being performed.
        operation: &'static str,
        /// Native libaec status.
        status: Status,
    },
    /// A caller buffer or native result has the wrong byte length.
    #[error("AEC {operation} produced {actual} bytes; expected {expected}")]
    OutputLength {
        /// Operation being performed.
        operation: &'static str,
        /// Required byte length.
        expected: usize,
        /// Produced or supplied byte length.
        actual: usize,
    },
    /// A fallible Rust-side buffer allocation failed.
    #[error("failed to allocate {bytes} bytes for AEC {operation}: {reason}")]
    Allocation {
        /// Operation being performed.
        operation: &'static str,
        /// Requested allocation size.
        bytes: usize,
        /// Allocator failure detail.
        reason: String,
    },
    /// A byte-count or codec-capacity calculation overflowed.
    #[error("AEC byte-count arithmetic overflow while {operation}")]
    ArithmeticOverflow {
        /// Size computation that overflowed.
        operation: &'static str,
    },
}

/// libaec status returned by a buffer operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Status {
    /// Invalid codec parameters.
    Configuration,
    /// Invalid streaming state.
    Stream,
    /// Malformed compressed data.
    Data,
    /// Native allocation or output-buffer failure.
    Memory,
    /// Invalid reference-sample offset data.
    ReferenceOffsets,
    /// Status introduced by a newer native codec.
    Unknown(c_int),
}

impl std::fmt::Display for Status {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration => formatter.write_str("a configuration error"),
            Self::Stream => formatter.write_str("a stream error"),
            Self::Data => formatter.write_str("a compressed-data error"),
            Self::Memory => formatter.write_str("a memory error"),
            Self::ReferenceOffsets => formatter.write_str("a reference-offset error"),
            Self::Unknown(code) => write!(formatter, "unknown status {code}"),
        }
    }
}

impl From<c_int> for Status {
    fn from(value: c_int) -> Self {
        match value {
            -1 => Self::Configuration,
            -2 => Self::Stream,
            -3 => Self::Data,
            -4 => Self::Memory,
            -5 => Self::ReferenceOffsets,
            value => Self::Unknown(value),
        }
    }
}

/// Decodes exactly `sample_count` samples into a newly allocated byte buffer.
///
/// Signed samples narrower than their byte container are sign-extended by
/// libaec. Unsigned samples retain zeroes in unused high bits.
pub fn decode(input: &[u8], sample_count: usize, params: AecParams) -> Result<Vec<u8>, Error> {
    let output_len =
        sample_count
            .checked_mul(params.bytes_per_sample())
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing decoded size",
            })?;
    let mut output = allocate_bytes(output_len, "decode")?;
    decode_into(input, sample_count, params, &mut output)?;
    Ok(output)
}

/// Decodes exactly `sample_count` samples into `output`.
pub fn decode_into(
    input: &[u8],
    sample_count: usize,
    params: AecParams,
    output: &mut [u8],
) -> Result<(), Error> {
    let expected =
        sample_count
            .checked_mul(params.bytes_per_sample())
            .ok_or(Error::ArithmeticOverflow {
                operation: "computing decoded size",
            })?;
    if output.len() != expected {
        return Err(Error::OutputLength {
            operation: "decode",
            expected,
            actual: output.len(),
        });
    }
    if output.is_empty() {
        return if input.is_empty() {
            Ok(())
        } else {
            Err(Error::OutputLength {
                operation: "decode",
                expected: 0,
                actual: input.len(),
            })
        };
    }

    let mut stream = RawAecStream::new(input, output, params);
    // SAFETY: `stream` contains pointers derived from live input/output slices,
    // their byte lengths, validated scalar parameters, and a null internal
    // state. libaec initializes and releases that state within this call.
    let status = unsafe { aec_buffer_decode(&mut stream) };
    check_status("decode", status)?;
    if stream.total_out != expected {
        return Err(Error::OutputLength {
            operation: "decode",
            expected,
            actual: stream.total_out,
        });
    }
    Ok(())
}

/// Encodes sample containers into a newly allocated CCSDS/AEC byte stream.
pub fn encode(input: &[u8], params: AecParams) -> Result<Vec<u8>, Error> {
    if params.flags.contains(AecFlags::PAD_RSI) {
        return Err(Error::InvalidParameter(
            "PAD_RSI is a legacy decode-only option",
        ));
    }
    validate_samples(input, params)?;
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let output_capacity = input
        .len()
        .checked_mul(67)
        .and_then(|bytes| bytes.checked_add(63))
        .map(|bytes| bytes / 64)
        .and_then(|bytes| bytes.checked_add(256))
        .ok_or(Error::ArithmeticOverflow {
            operation: "computing encoded capacity",
        })?;
    let mut output = allocate_bytes(output_capacity, "encode")?;
    let mut stream = RawAecStream::new(input, &mut output, params);
    // SAFETY: the same slice-lifetime and parameter invariants documented for
    // `decode_into` hold here; the output follows libaec's documented upper
    // bound and the input samples were checked for width compliance.
    let status = unsafe { aec_buffer_encode(&mut stream) };
    check_status("encode", status)?;
    let encoded_len = stream.total_out;
    if encoded_len > output.len() {
        return Err(Error::OutputLength {
            operation: "encode",
            expected: output.len(),
            actual: encoded_len,
        });
    }
    output.truncate(encoded_len);
    Ok(output)
}

fn allocate_bytes(bytes: usize, operation: &'static str) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes)
        .map_err(|error| Error::Allocation {
            operation,
            bytes,
            reason: error.to_string(),
        })?;
    output.resize(bytes, 0);
    Ok(output)
}

fn validate_samples(input: &[u8], params: AecParams) -> Result<(), Error> {
    let width = params.bytes_per_sample();
    if !input.len().is_multiple_of(width) {
        return Err(Error::InputLength {
            actual: input.len(),
            sample_width: width,
        });
    }

    let unused_bits = width as u32 * 8 - u32::from(params.bits_per_sample);
    if unused_bits == 0 {
        return Ok(());
    }
    let max_value = (1u32 << params.bits_per_sample) - 1;
    for (index, bytes) in input.chunks_exact(width).enumerate() {
        let value = if params.flags.contains(AecFlags::DATA_MSB) {
            bytes
                .iter()
                .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
        } else {
            bytes
                .iter()
                .rev()
                .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
        };
        if value > max_value {
            return Err(Error::SampleOutOfRange {
                index,
                value,
                bits: params.bits_per_sample,
            });
        }
    }
    Ok(())
}

fn check_status(operation: &'static str, status: c_int) -> Result<(), Error> {
    if status == AEC_OK {
        Ok(())
    } else {
        Err(Error::Codec {
            operation,
            status: Status::from(status),
        })
    }
}

#[repr(C)]
struct RawAecStream {
    next_in: *const u8,
    avail_in: usize,
    total_in: usize,
    next_out: *mut u8,
    avail_out: usize,
    total_out: usize,
    bits_per_sample: u32,
    block_size: u32,
    rsi: u32,
    flags: u32,
    state: *mut c_void,
}

impl RawAecStream {
    fn new(input: &[u8], output: &mut [u8], params: AecParams) -> Self {
        Self {
            next_in: input.as_ptr(),
            avail_in: input.len(),
            total_in: 0,
            next_out: output.as_mut_ptr(),
            avail_out: output.len(),
            total_out: 0,
            bits_per_sample: u32::from(params.bits_per_sample),
            block_size: u32::from(params.block_size),
            rsi: u32::from(params.reference_sample_interval),
            flags: params.flags.bits(),
            state: ptr::null_mut(),
        }
    }
}

unsafe extern "C" {
    #[link_name = "grib_aec_buffer_encode"]
    fn aec_buffer_encode(stream: *mut RawAecStream) -> c_int;
    #[link_name = "grib_aec_buffer_decode"]
    fn aec_buffer_decode(stream: *mut RawAecStream) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, AecFlags, AecParams, Error};

    #[test]
    fn roundtrips_all_container_widths() {
        for (bits, flags, input) in [
            (
                8,
                AecFlags::DATA_PREPROCESS,
                sample_bytes(&[0, 1, 17, 255], 1),
            ),
            (
                16,
                AecFlags::DATA_MSB | AecFlags::DATA_PREPROCESS,
                sample_bytes(&[0, 1, 0x1234, 0xffff], 2),
            ),
            (
                20,
                AecFlags::DATA_3BYTE | AecFlags::DATA_MSB | AecFlags::DATA_PREPROCESS,
                sample_bytes(&[0, 1, 0x54321, 0xfffff], 3),
            ),
            (
                32,
                AecFlags::DATA_MSB | AecFlags::DATA_PREPROCESS,
                sample_bytes(&[0, 1, 0x76543210, u32::MAX], 4),
            ),
        ] {
            let params = AecParams::new(bits, 16, 128, flags).unwrap();
            let encoded = encode(&input, params).unwrap();
            let decoded =
                decode(&encoded, input.len() / params.bytes_per_sample(), params).unwrap();
            assert_eq!(decoded, input, "bits_per_sample={bits}");
        }
    }

    #[test]
    fn roundtrips_signed_restricted_and_unenforced_modes() {
        let cases = [
            (
                12,
                16,
                AecFlags::DATA_SIGNED | AecFlags::DATA_MSB | AecFlags::DATA_PREPROCESS,
                sample_bytes(&[0xfef, 0xfff, 0, 1, 0x1ff], 2),
            ),
            (
                4,
                16,
                AecFlags::RESTRICTED | AecFlags::DATA_PREPROCESS,
                sample_bytes(&[0, 1, 2, 3, 15, 7, 4, 0], 1),
            ),
            (
                8,
                12,
                AecFlags::NOT_ENFORCE | AecFlags::DATA_PREPROCESS,
                (0..24).collect(),
            ),
        ];

        for (bits, block_size, flags, input) in cases {
            let params = AecParams::new(bits, block_size, 128, flags).unwrap();
            let encoded = encode(&input, params).unwrap();
            let decoded =
                decode(&encoded, input.len() / params.bytes_per_sample(), params).unwrap();
            if flags.contains(AecFlags::DATA_SIGNED) {
                let width = params.bytes_per_sample();
                let mask = (1u32 << bits) - 1;
                for (actual, expected) in decoded.chunks_exact(width).zip(input.chunks_exact(width))
                {
                    assert_eq!(read_be(actual) & mask, read_be(expected) & mask);
                }
            } else {
                assert_eq!(decoded, input);
            }
        }
    }

    #[test]
    fn roundtrips_incompressible_32_bit_blocks() {
        let input = sample_bytes(
            &[
                0,
                u32::MAX,
                0x5555_5555,
                0xaaaa_aaaa,
                1,
                0xffff_fffe,
                0x1234_5678,
                0x8765_4321,
            ],
            4,
        );
        let params = AecParams::new(32, 8, 1, AecFlags::DATA_MSB).unwrap();
        let encoded = encode(&input, params).unwrap();
        let decoded = decode(&encoded, 8, params).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn decodes_byte_aligned_reference_sample_intervals() {
        // A PAD_RSI stream generated by oxiarc-szip 0.3.6 and independently
        // decoded here by the vendored libaec implementation.
        let expected = from_hex(
            "81848585858789888684858383868684817f8083848584817e81848687878487858685878786898a",
        );
        let encoded = from_hex(
            "f020c040000080803c0c0c080c0018000cf0206040c0404020bc1418181008001418f0a04020800020c040",
        );
        let params = AecParams::new(
            8,
            8,
            2,
            AecFlags::DATA_MSB | AecFlags::DATA_PREPROCESS | AecFlags::PAD_RSI,
        )
        .unwrap();

        assert_eq!(decode(&encoded, expected.len(), params).unwrap(), expected);
    }

    #[test]
    fn rejects_samples_with_nonzero_unused_bits() {
        let params = AecParams::new(12, 16, 128, AecFlags::DATA_MSB).unwrap();
        let error = encode(&[0xf0, 0x00], params).unwrap_err();
        assert!(matches!(error, Error::SampleOutOfRange { index: 0, .. }));
    }

    #[test]
    fn rejects_decode_only_padding_during_encode() {
        let params = AecParams::new(8, 16, 128, AecFlags::PAD_RSI).unwrap();
        assert!(matches!(
            encode(&[1, 2, 3], params),
            Err(Error::InvalidParameter(
                "PAD_RSI is a legacy decode-only option"
            ))
        ));
    }

    fn sample_bytes(values: &[u32], width: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(values.len() * width);
        for value in values {
            bytes.extend_from_slice(&value.to_be_bytes()[4 - width..]);
        }
        bytes
    }

    fn read_be(bytes: &[u8]) -> u32 {
        bytes
            .iter()
            .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
    }

    fn from_hex(hex: &str) -> Vec<u8> {
        assert!(hex.len().is_multiple_of(2));
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect()
    }
}
