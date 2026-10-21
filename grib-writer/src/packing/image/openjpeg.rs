#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::TryReserveError;
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use grib_core::Error;
use openjpeg_sys as sys;

use crate::Result;

const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// OpenJPEG stores tier-1 coefficients in signed 32-bit integers after adding
/// six fractional bits. An unsigned 26-bit component includes `-2^25` after
/// the JPEG 2000 DC level shift, which overflows that representation. Keep one
/// sign bit and six tier-1 fractional bits by accepting at most 25 input bits.
pub(super) const MAX_LOSSLESS_BITS_PER_VALUE: u8 = 25;

/// Encode unsigned grayscale samples as a lossless raw JPEG 2000 codestream.
///
/// This safe boundary owns every OpenJPEG resource and callback allocation for
/// the entire synchronous encode operation. The FFI implementation below is
/// the only unsafe-code exception in `grib-writer`.
pub(super) fn encode_jpeg2000_codestream(
    deltas: &[u64],
    bits_per_value: u8,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    if !(1..=MAX_LOSSLESS_BITS_PER_VALUE).contains(&bits_per_value) {
        return Err(Error::UnsupportedPackingWidth(bits_per_value));
    }
    if width == 0 || height == 0 {
        return Err(Error::ValueOutOfRange(
            "JPEG2000 image dimensions must be nonzero".into(),
        ));
    }
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(Error::ArithmeticOverflow {
            operation: "calculating JPEG2000 image sample count",
        })?;
    if deltas.len() != expected {
        return Err(Error::DataLengthMismatch {
            expected,
            actual: deltas.len(),
        });
    }
    let maximum_sample = (1u64 << bits_per_value) - 1;
    if deltas.iter().any(|delta| *delta > maximum_sample) {
        return Err(Error::UnsupportedPackingWidth(bits_per_value));
    }

    let mut image = OpenJpegImage::new(deltas, bits_per_value, width, height)?;
    let mut encoder = OpenJpegEncoder::new()?;
    encoder.setup(
        &mut image,
        jpeg2000_num_resolutions(width, height, bits_per_value),
    )?;
    let output = MemoryOutputStream::new()?;

    encoder.start(&mut image, &output)?;
    encoder.encode(&output)?;
    encoder.finish(&output)?;
    output.into_bytes()
}

struct OpenJpegImage(NonNull<sys::opj_image_t>);

impl OpenJpegImage {
    fn new(deltas: &[u64], bits_per_value: u8, width: u32, height: u32) -> Result<Self> {
        let mut component = sys::opj_image_cmptparm_t {
            dx: 1,
            dy: 1,
            w: width,
            h: height,
            x0: 0,
            y0: 0,
            prec: u32::from(bits_per_value),
            bpp: u32::from(bits_per_value),
            sgnd: 0,
        };

        // SAFETY: `component` is initialized according to OpenJPEG's image
        // creation contract and remains live for the duration of this call.
        let pointer =
            unsafe { sys::opj_image_create(1, &mut component, sys::COLOR_SPACE::OPJ_CLRSPC_GRAY) };
        let mut image = Self(NonNull::new(pointer).ok_or_else(|| Error::Codec {
            codec: "JPEG2000",
            operation: "creating image",
            reason: "OpenJPEG returned a null image".into(),
        })?);

        // SAFETY: `image` owns a valid OpenJPEG image with one component.
        // `opj_image_create` allocates that component's sample array from the
        // supplied width and height, which were already checked against
        // `deltas.len()` by the safe entry point.
        unsafe {
            let raw_image = image.0.as_mut();
            raw_image.x0 = 0;
            raw_image.y0 = 0;
            raw_image.x1 = width;
            raw_image.y1 = height;

            if raw_image.numcomps != 1 || raw_image.comps.is_null() {
                return Err(Error::Codec {
                    codec: "JPEG2000",
                    operation: "creating image component",
                    reason: "OpenJPEG returned an invalid component array".into(),
                });
            }
            let component = &mut *raw_image.comps;
            if component.data.is_null() {
                return Err(Error::Codec {
                    codec: "JPEG2000",
                    operation: "allocating image samples",
                    reason: "OpenJPEG returned a null sample buffer".into(),
                });
            }
            let samples = std::slice::from_raw_parts_mut(component.data, deltas.len());
            for (sample, delta) in samples.iter_mut().zip(deltas) {
                *sample = i32::try_from(*delta)
                    .map_err(|_| Error::UnsupportedPackingWidth(bits_per_value))?;
            }
        }

        Ok(image)
    }

    fn as_ptr(&mut self) -> *mut sys::opj_image_t {
        self.0.as_ptr()
    }
}

impl Drop for OpenJpegImage {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the image pointer and destroys it
        // exactly once.
        unsafe { sys::opj_image_destroy(self.0.as_ptr()) };
    }
}

struct OpenJpegEncoder(NonNull<sys::opj_codec_t>);

impl OpenJpegEncoder {
    fn new() -> Result<Self> {
        // SAFETY: creating a codec has no preconditions; the returned pointer is
        // checked before ownership is assumed.
        let pointer = unsafe { sys::opj_create_compress(sys::CODEC_FORMAT::OPJ_CODEC_J2K) };
        NonNull::new(pointer).map(Self).ok_or_else(|| Error::Codec {
            codec: "JPEG2000",
            operation: "creating encoder",
            reason: "OpenJPEG returned a null encoder".into(),
        })
    }

    fn setup(&mut self, image: &mut OpenJpegImage, num_resolutions: i32) -> Result<()> {
        let mut params = std::mem::MaybeUninit::<sys::opj_cparameters_t>::uninit();
        // SAFETY: OpenJPEG initializes the complete parameter structure with a
        // `memset` followed by its documented defaults. The pointer is valid
        // and properly aligned for the duration of the call.
        unsafe { sys::opj_set_default_encoder_parameters(params.as_mut_ptr()) };
        // SAFETY: the preceding OpenJPEG call initialized every byte.
        let mut params = unsafe { params.assume_init() };
        params.tcp_numlayers = 1;
        params.tcp_rates[0] = 0.0;
        params.cp_disto_alloc = 1;
        params.irreversible = 0;
        params.numresolution = num_resolutions;
        params.cod_format = 0;
        params.tcp_mct = 0;

        // SAFETY: the codec and image are valid and exclusively borrowed for
        // the duration of setup. OpenJPEG copies the parameter values it needs.
        let success =
            unsafe { sys::opj_setup_encoder(self.0.as_ptr(), &mut params, image.as_ptr()) };
        bool_result(success, "setting up encoder")
    }

    fn start(&mut self, image: &mut OpenJpegImage, output: &MemoryOutputStream) -> Result<()> {
        // SAFETY: codec, image, and stream are valid and remain alive through
        // the complete synchronous encoding sequence.
        let success =
            unsafe { sys::opj_start_compress(self.0.as_ptr(), image.as_ptr(), output.as_ptr()) };
        output.result_or(success, "starting codestream")
    }

    fn encode(&mut self, output: &MemoryOutputStream) -> Result<()> {
        // SAFETY: setup and start completed successfully, and the stream remains
        // valid for this synchronous call.
        let success = unsafe { sys::opj_encode(self.0.as_ptr(), output.as_ptr()) };
        output.result_or(success, "encoding codestream")
    }

    fn finish(&mut self, output: &MemoryOutputStream) -> Result<()> {
        // SAFETY: the codec and stream are live and this is called exactly once
        // after a successful encode.
        let success = unsafe { sys::opj_end_compress(self.0.as_ptr(), output.as_ptr()) };
        output.result_or(success, "finishing codestream")
    }
}

impl Drop for OpenJpegEncoder {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the codec pointer and destroys it
        // exactly once.
        unsafe { sys::opj_destroy_codec(self.0.as_ptr()) };
    }
}

struct MemoryOutputStream {
    stream: Option<NonNull<sys::opj_stream_t>>,
    state: Option<Box<CallbackState>>,
}

impl MemoryOutputStream {
    fn new() -> Result<Self> {
        let mut state = Box::new(CallbackState::default());
        // SAFETY: creating an output stream has no caller-side preconditions;
        // the returned pointer is checked before use.
        let pointer = unsafe { sys::opj_stream_create(STREAM_BUFFER_SIZE, 0) };
        let stream = NonNull::new(pointer).ok_or_else(|| Error::Codec {
            codec: "JPEG2000",
            operation: "creating in-memory stream",
            reason: "OpenJPEG returned a null stream".into(),
        })?;
        let state_ptr = (&mut *state as *mut CallbackState).cast::<c_void>();

        // SAFETY: `stream` is valid, and `state_ptr` targets a boxed allocation
        // that remains stable until after the stream is destroyed. Callbacks do
        // not retain OpenJPEG buffers, synchronize state access, catch unwinds,
        // and communicate failures through the prescribed return values. This
        // wrapper retains ownership, so no C free callback is installed.
        unsafe {
            sys::opj_stream_set_write_function(stream.as_ptr(), Some(write_callback));
            sys::opj_stream_set_skip_function(stream.as_ptr(), Some(skip_callback));
            sys::opj_stream_set_seek_function(stream.as_ptr(), Some(seek_callback));
            sys::opj_stream_set_user_data(stream.as_ptr(), state_ptr, None);
        }

        Ok(Self {
            stream: Some(stream),
            state: Some(state),
        })
    }

    fn as_ptr(&self) -> *mut sys::opj_stream_t {
        self.stream
            .expect("memory stream pointer exists until drop")
            .as_ptr()
    }

    fn result_or(&self, success: sys::OPJ_BOOL, operation: &'static str) -> Result<()> {
        if let Some(failure) = self.state().take_failure() {
            return Err(failure);
        }
        bool_result(success, operation)
    }

    fn into_bytes(mut self) -> Result<Vec<u8>> {
        self.destroy_stream();
        if let Some(failure) = self.state().take_failure() {
            return Err(failure);
        }

        let state = self
            .state
            .take()
            .expect("callback state exists until stream destruction");
        Ok((*state).into_bytes())
    }

    fn state(&self) -> &CallbackState {
        self.state
            .as_deref()
            .expect("callback state exists until stream destruction")
    }

    fn destroy_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            // SAFETY: this wrapper uniquely owns the stream and clears the
            // option so destruction occurs exactly once.
            unsafe { sys::opj_stream_destroy(stream.as_ptr()) };
        }
    }
}

impl Drop for MemoryOutputStream {
    fn drop(&mut self) {
        self.destroy_stream();
    }
}

#[derive(Default)]
struct CallbackState {
    output: Mutex<MemoryOutputState>,
    callback_panicked: AtomicBool,
}

impl CallbackState {
    fn output(&self) -> MutexGuard<'_, MemoryOutputState> {
        match self.output.lock() {
            Ok(output) => output,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn record_failure(&self, failure: MemoryStreamFailure) {
        self.output().record_failure(failure);
    }

    fn take_failure(&self) -> Option<Error> {
        if self.callback_panicked.swap(false, Ordering::Relaxed) {
            return Some(Error::Codec {
                codec: "JPEG2000",
                operation: "running in-memory stream callback",
                reason: "an internal callback panicked".into(),
            });
        }
        self.output()
            .failure
            .take()
            .map(MemoryStreamFailure::into_error)
    }

    fn into_bytes(self) -> Vec<u8> {
        let Self { output, .. } = self;
        match output.into_inner() {
            Ok(output) => output.bytes,
            Err(poisoned) => poisoned.into_inner().bytes,
        }
    }
}

#[derive(Default)]
struct MemoryOutputState {
    bytes: Vec<u8>,
    position: usize,
    failure: Option<MemoryStreamFailure>,
}

impl MemoryOutputState {
    fn write(&mut self, input: &[u8]) -> std::result::Result<usize, MemoryStreamFailure> {
        let end = self
            .position
            .checked_add(input.len())
            .ok_or(MemoryStreamFailure::PositionOverflow)?;

        if end > self.bytes.len() {
            let additional = end - self.bytes.len();
            self.bytes.try_reserve(additional).map_err(|source| {
                MemoryStreamFailure::Allocation {
                    requested: end,
                    source,
                }
            })?;
            self.bytes.resize(end, 0);
        }

        self.bytes[self.position..end].copy_from_slice(input);
        self.position = end;
        Ok(input.len())
    }

    fn skip(&mut self, offset: i64) -> std::result::Result<i64, MemoryStreamFailure> {
        self.position = checked_offset(self.position, offset)?;
        Ok(offset)
    }

    fn seek(&mut self, offset: i64) -> std::result::Result<(), MemoryStreamFailure> {
        self.position = usize::try_from(offset).map_err(|_| MemoryStreamFailure::InvalidSeek)?;
        Ok(())
    }

    fn record_failure(&mut self, failure: MemoryStreamFailure) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
    }
}

fn checked_offset(position: usize, offset: i64) -> std::result::Result<usize, MemoryStreamFailure> {
    if offset >= 0 {
        let offset = usize::try_from(offset).map_err(|_| MemoryStreamFailure::PositionOverflow)?;
        position
            .checked_add(offset)
            .ok_or(MemoryStreamFailure::PositionOverflow)
    } else {
        let magnitude =
            usize::try_from(offset.unsigned_abs()).map_err(|_| MemoryStreamFailure::InvalidSeek)?;
        position
            .checked_sub(magnitude)
            .ok_or(MemoryStreamFailure::InvalidSeek)
    }
}

#[derive(Debug)]
enum MemoryStreamFailure {
    Allocation {
        requested: usize,
        source: TryReserveError,
    },
    InvalidCallbackPointer,
    InvalidSeek,
    PositionOverflow,
}

impl MemoryStreamFailure {
    fn into_error(self) -> Error {
        match self {
            Self::Allocation { requested, source } => {
                Error::allocation("bytes for JPEG2000 codestream", requested, source)
            }
            Self::InvalidCallbackPointer => Error::Codec {
                codec: "JPEG2000",
                operation: "writing in-memory codestream",
                reason: "OpenJPEG supplied a null callback pointer".into(),
            },
            Self::InvalidSeek => Error::Codec {
                codec: "JPEG2000",
                operation: "seeking in-memory codestream",
                reason: "OpenJPEG requested a position before the start of the stream".into(),
            },
            Self::PositionOverflow => Error::ArithmeticOverflow {
                operation: "positioning the JPEG2000 in-memory codestream",
            },
        }
    }
}

unsafe extern "C" fn write_callback(
    input: *mut c_void,
    length: usize,
    user_data: *mut c_void,
) -> usize {
    match catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: OpenJPEG invokes this function with the callback state and
        // write buffer configured by `MemoryOutputStream::new`.
        unsafe { write_callback_inner(input, length, user_data) }
    })) {
        Ok(written) => written,
        Err(_) => {
            record_callback_panic(user_data);
            usize::MAX
        }
    }
}

unsafe extern "C" fn skip_callback(offset: i64, user_data: *mut c_void) -> i64 {
    match catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: OpenJPEG invokes this function with the callback state
        // configured by `MemoryOutputStream::new`.
        unsafe { skip_callback_inner(offset, user_data) }
    })) {
        Ok(skipped) => skipped,
        Err(_) => {
            record_callback_panic(user_data);
            -1
        }
    }
}

unsafe extern "C" fn seek_callback(offset: i64, user_data: *mut c_void) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: OpenJPEG invokes this function with the callback state
        // configured by `MemoryOutputStream::new`.
        unsafe { seek_callback_inner(offset, user_data) }
    })) {
        Ok(seeked) => seeked,
        Err(_) => {
            record_callback_panic(user_data);
            0
        }
    }
}

unsafe fn write_callback_inner(input: *mut c_void, length: usize, user_data: *mut c_void) -> usize {
    // SAFETY: the caller guarantees `user_data` is the pointer installed by
    // `MemoryOutputStream::new` and remains live for the callback duration.
    let Some(state) = (unsafe { state_from_ptr(user_data) }) else {
        return usize::MAX;
    };
    if length == 0 {
        return 0;
    }
    if input.is_null() {
        state.record_failure(MemoryStreamFailure::InvalidCallbackPointer);
        return usize::MAX;
    }

    // SAFETY: OpenJPEG guarantees that a non-null write buffer contains
    // `length` readable bytes for the duration of this callback. The slice is
    // copied into owned storage and is not retained.
    let input = unsafe { std::slice::from_raw_parts(input.cast::<u8>(), length) };
    let mut output = state.output();
    match output.write(input) {
        Ok(written) => written,
        Err(failure) => {
            output.record_failure(failure);
            usize::MAX
        }
    }
}

unsafe fn skip_callback_inner(offset: i64, user_data: *mut c_void) -> i64 {
    // SAFETY: the caller guarantees `user_data` is the pointer installed by
    // `MemoryOutputStream::new` and remains live for the callback duration.
    let Some(state) = (unsafe { state_from_ptr(user_data) }) else {
        return -1;
    };
    let mut output = state.output();
    match output.skip(offset) {
        Ok(skipped) => skipped,
        Err(failure) => {
            output.record_failure(failure);
            -1
        }
    }
}

unsafe fn seek_callback_inner(offset: i64, user_data: *mut c_void) -> i32 {
    // SAFETY: the caller guarantees `user_data` is the pointer installed by
    // `MemoryOutputStream::new` and remains live for the callback duration.
    let Some(state) = (unsafe { state_from_ptr(user_data) }) else {
        return 0;
    };
    let mut output = state.output();
    match output.seek(offset) {
        Ok(()) => 1,
        Err(failure) => {
            output.record_failure(failure);
            0
        }
    }
}

fn record_callback_panic(user_data: *mut c_void) {
    // SAFETY: this function is only called from a registered callback while
    // `MemoryOutputStream` still owns the boxed callback state. A null pointer
    // is handled without dereferencing it.
    let Some(state) = (unsafe { state_from_ptr(user_data) }) else {
        return;
    };
    state.callback_panicked.store(true, Ordering::Relaxed);
}

unsafe fn state_from_ptr<'a>(user_data: *mut c_void) -> Option<&'a CallbackState> {
    if user_data.is_null() {
        return None;
    }

    // SAFETY: `MemoryOutputStream::new` installs a pointer to its boxed state,
    // keeps that allocation alive until after stream destruction, and the state
    // uses a mutex so callback access remains synchronized.
    Some(unsafe { &*user_data.cast::<CallbackState>() })
}

fn bool_result(success: sys::OPJ_BOOL, operation: &'static str) -> Result<()> {
    if success != 0 {
        Ok(())
    } else {
        Err(Error::Codec {
            codec: "JPEG2000",
            operation,
            reason: "OpenJPEG reported failure".into(),
        })
    }
}

fn jpeg2000_num_resolutions(width: u32, height: u32, bits_per_value: u8) -> i32 {
    let min_dimension = width.min(height);
    let dimension_levels = u32::BITS - min_dimension.leading_zeros() - 1;

    // A reversible 5/3 transform can double the coefficient magnitude in each
    // dimension, so each two-dimensional decomposition needs two bits of
    // numeric headroom. Tier-1 can safely receive magnitudes strictly below
    // 2^25; the DC-level-shifted input starts strictly below 2^(precision-1).
    let numeric_levels = u32::from(MAX_LOSSLESS_BITS_PER_VALUE + 1 - bits_per_value) / 2;
    let decomposition_levels = dimension_levels.min(numeric_levels);

    i32::try_from(decomposition_levels + 1).expect("JPEG2000 resolution count fits in i32")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_output_state_supports_backpatching() {
        let mut output = MemoryOutputState::default();
        output.write(b"abcd").unwrap();
        output.seek(1).unwrap();
        output.write(b"Z").unwrap();

        assert_eq!(output.bytes, b"aZcd");
    }

    #[test]
    fn memory_output_state_zero_fills_forward_seeks() {
        let mut output = MemoryOutputState::default();
        output.write(b"ab").unwrap();
        output.skip(2).unwrap();
        output.write(b"Z").unwrap();

        assert_eq!(output.bytes, b"ab\0\0Z");
    }

    #[test]
    fn memory_output_state_rejects_positions_before_start() {
        let mut output = MemoryOutputState::default();
        output.write(b"abc").unwrap();

        assert!(matches!(
            output.skip(-4),
            Err(MemoryStreamFailure::InvalidSeek)
        ));
    }

    #[test]
    fn resolution_count_preserves_tier_one_numeric_headroom() {
        assert_eq!(jpeg2000_num_resolutions(1024, 1024, 25), 1);
        assert_eq!(jpeg2000_num_resolutions(1024, 1024, 24), 2);
        assert_eq!(jpeg2000_num_resolutions(1024, 1024, 22), 3);
        assert_eq!(jpeg2000_num_resolutions(8, 4, 1), 3);
        assert_eq!(jpeg2000_num_resolutions(1, 1024, 1), 1);
    }

    #[test]
    fn encoder_rejects_samples_outside_the_declared_precision() {
        assert!(matches!(
            encode_jpeg2000_codestream(&[2], 1, 1, 1),
            Err(Error::UnsupportedPackingWidth(1))
        ));
    }
}
