//! Bit-level readers and writers for GRIB packing templates.

use crate::error::{Error, Result};

pub fn read_bit(data: &[u8], bit_offset: usize) -> Result<bool> {
    let byte_index = bit_offset / 8;
    let bit_index = bit_offset % 8;
    let byte = *data.get(byte_index).ok_or(Error::Truncated {
        offset: byte_index as u64,
    })?;
    Ok(((byte >> (7 - bit_index)) & 1) != 0)
}

/// MSB-first bit reader over an immutable byte slice.
#[derive(Debug, Clone, Copy)]
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_offset: usize,
    next_byte: usize,
    buffer: u64,
    buffered_bits: u8,
    pending_skip: u8,
}

impl<'a> BitReader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_offset: 0,
            next_byte: 0,
            buffer: 0,
            buffered_bits: 0,
            pending_skip: 0,
        }
    }

    pub const fn with_offset(data: &'a [u8], bit_offset: usize) -> Self {
        Self {
            data,
            bit_offset,
            next_byte: bit_offset / 8,
            buffer: 0,
            buffered_bits: 0,
            pending_skip: (bit_offset % 8) as u8,
        }
    }

    pub const fn bit_offset(&self) -> usize {
        self.bit_offset
    }

    pub fn read(&mut self, bit_count: usize) -> Result<u64> {
        if bit_count == 0 {
            return Ok(0);
        }
        require_u64_width(bit_count)?;
        let end_bit_offset = self
            .bit_offset
            .checked_add(bit_count)
            .ok_or(Error::BitOffsetOverflow)?;

        self.initialize_skip()?;
        let bit_count = bit_count as u8;
        let value = if self.buffered_bits >= bit_count {
            self.take_buffered(bit_count)
        } else {
            let prefix_bits = self.buffered_bits;
            let prefix = self.take_buffered(prefix_bits);
            self.refill()?;
            let suffix_bits = bit_count - prefix_bits;
            if self.buffered_bits < suffix_bits {
                return Err(Error::Truncated {
                    offset: self.next_byte as u64,
                });
            }
            let suffix = self.take_buffered(suffix_bits);
            if prefix_bits == 0 {
                suffix
            } else {
                (prefix << suffix_bits) | suffix
            }
        };

        self.bit_offset = end_bit_offset;
        Ok(value)
    }

    fn initialize_skip(&mut self) -> Result<()> {
        if self.pending_skip != 0 {
            self.refill()?;
            if self.buffered_bits < self.pending_skip {
                return Err(Error::Truncated {
                    offset: self.next_byte as u64,
                });
            }
            self.take_buffered(self.pending_skip);
            self.pending_skip = 0;
        }
        Ok(())
    }

    fn refill(&mut self) -> Result<()> {
        debug_assert_eq!(self.buffered_bits, 0);
        let remaining = self.data.len().saturating_sub(self.next_byte);
        if remaining == 0 {
            return Err(Error::Truncated {
                offset: self.next_byte as u64,
            });
        }
        let byte_count = remaining.min(8);
        let end = self.next_byte + byte_count;
        let mut bytes = [0u8; 8];
        bytes[..byte_count].copy_from_slice(&self.data[self.next_byte..end]);
        self.buffer = u64::from_be_bytes(bytes) >> ((8 - byte_count) * 8);
        self.buffered_bits = (byte_count * 8) as u8;
        self.next_byte = end;
        Ok(())
    }

    fn take_buffered(&mut self, bit_count: u8) -> u64 {
        debug_assert!(bit_count <= self.buffered_bits);
        if bit_count == 0 {
            return 0;
        }
        let remaining = self.buffered_bits - bit_count;
        let value = if bit_count == u64::BITS as u8 {
            self.buffer
        } else {
            (self.buffer >> remaining) & ((1u64 << bit_count) - 1)
        };
        self.buffered_bits = remaining;
        self.buffer = low_bits(self.buffer, remaining);
        value
    }

    pub fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read(1)? != 0)
    }

    pub fn read_signed(&mut self, bit_count: usize) -> Result<i64> {
        if bit_count == 0 {
            return Ok(0);
        }
        require_u64_width(bit_count)?;

        let value = self.read(bit_count)?;
        let sign_mask = 1u64 << (bit_count - 1);
        if value & sign_mask == 0 {
            return i64::try_from(value)
                .map_err(|_| Error::ValueOutOfRange("signed value exceeds i64 range".into()));
        }

        let magnitude_mask = sign_mask - 1;
        let magnitude = value & magnitude_mask;
        let magnitude = i64::try_from(magnitude)
            .map_err(|_| Error::ValueOutOfRange("signed value exceeds i64 range".into()))?;
        Ok(-magnitude)
    }
}

/// MSB-first bit writer for GRIB packing templates.
#[derive(Debug, Clone, Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    bit_offset: usize,
}

impl BitWriter {
    pub const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_offset: 0,
        }
    }

    pub fn with_capacity_bits(bit_capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(bit_capacity.div_ceil(8)),
            bit_offset: 0,
        }
    }

    pub fn bit_len(&self) -> usize {
        self.bit_offset
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bit_offset == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn write(&mut self, value: u64, bit_count: usize) -> Result<()> {
        if bit_count == 0 {
            if value == 0 {
                return Ok(());
            }
            return Err(Error::ValueOutOfRange(
                "non-zero value cannot be written with zero bits".into(),
            ));
        }
        require_u64_width(bit_count)?;
        if bit_count < u64::BITS as usize && (value >> bit_count) != 0 {
            return Err(Error::ValueOutOfRange(format!(
                "value {value} does not fit in {bit_count} bits"
            )));
        }

        let end_bit_offset = self
            .bit_offset
            .checked_add(bit_count)
            .ok_or(Error::BitOffsetOverflow)?;
        let required_bytes = end_bit_offset.div_ceil(8);
        if required_bytes > self.bytes.len() {
            let additional = required_bytes - self.bytes.len();
            self.bytes
                .try_reserve(additional)
                .map_err(|error| Error::allocation("bit-writer bytes", additional, error))?;
            self.bytes.resize(required_bytes, 0);
        }

        let mut remaining = bit_count;
        let mut byte_index = self.bit_offset / 8;
        let bit_index = self.bit_offset % 8;

        if bit_index != 0 {
            let available = 8 - bit_index;
            let take = remaining.min(available);
            let source_shift = remaining - take;
            let mask = (1u16 << take) - 1;
            let chunk = ((value >> source_shift) as u8) & mask as u8;
            self.bytes[byte_index] |= chunk << (available - take);
            remaining -= take;
            byte_index += usize::from(take == available);
        }

        while remaining >= 8 {
            let source_shift = remaining - 8;
            self.bytes[byte_index] = (value >> source_shift) as u8;
            remaining -= 8;
            byte_index += 1;
        }

        if remaining != 0 {
            let mask = (1u16 << remaining) - 1;
            self.bytes[byte_index] |= ((value as u8) & mask as u8) << (8 - remaining);
        }

        self.bit_offset = end_bit_offset;
        Ok(())
    }

    pub fn align_to_byte(&mut self) -> Result<()> {
        let remainder = self.bit_offset % 8;
        if remainder != 0 {
            self.bit_offset = self
                .bit_offset
                .checked_add(8 - remainder)
                .ok_or(Error::BitOffsetOverflow)?;
        }
        Ok(())
    }
}

fn low_bits(value: u64, bit_count: u8) -> u64 {
    match bit_count {
        0 => 0,
        64 => value,
        bits => value & ((1u64 << bits) - 1),
    }
}

fn require_u64_width(bit_count: usize) -> Result<()> {
    if bit_count <= u64::BITS as usize {
        return Ok(());
    }

    Err(Error::UnsupportedPackingWidth(
        u8::try_from(bit_count).unwrap_or(u8::MAX),
    ))
}

#[cfg(test)]
mod tests {
    use super::{read_bit, BitReader, BitWriter};

    #[test]
    fn reads_msb_first_across_byte_boundaries() {
        let mut reader = BitReader::new(&[0b1011_0010, 0b0110_0000]);

        assert_eq!(reader.read(3).unwrap(), 0b101);
        assert_eq!(reader.read(5).unwrap(), 0b10010);
        assert_eq!(reader.read(4).unwrap(), 0b0110);
        assert_eq!(reader.bit_offset(), 12);
    }

    #[test]
    fn buffered_reader_crosses_multiple_word_boundaries() {
        let data: Vec<u8> = (0..24).collect();
        let mut reader = BitReader::with_offset(&data, 3);
        let mut reference = Vec::new();
        for bit in 3..(data.len() * 8) {
            reference.push(read_bit(&data, bit).unwrap());
        }
        for expected in reference {
            assert_eq!(reader.read_bool().unwrap(), expected);
        }
    }

    #[test]
    fn buffered_reader_matches_bit_reference_for_every_width_and_offset() {
        let data = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xf0, 0x0f, 0xaa, 0x55,
        ];
        for offset in 0..data.len() * 8 {
            let available = data.len() * 8 - offset;
            for width in 0..=available.min(64) {
                let mut expected = 0u64;
                for bit in offset..offset + width {
                    expected = (expected << 1) | u64::from(read_bit(&data, bit).unwrap());
                }
                let mut reader = BitReader::with_offset(&data, offset);
                assert_eq!(
                    reader.read(width).unwrap(),
                    expected,
                    "offset={offset}, width={width}"
                );
                assert_eq!(reader.bit_offset(), offset + width);
            }
        }
    }

    #[test]
    fn reads_single_bits_by_offset() {
        assert!(read_bit(&[0b1010_0000], 0).unwrap());
        assert!(!read_bit(&[0b1010_0000], 1).unwrap());
        assert!(read_bit(&[0b1010_0000], 2).unwrap());
        assert!(read_bit(&[], 0).is_err());
    }

    #[test]
    fn reads_grib_style_signed_magnitudes() {
        let mut reader = BitReader::new(&[0b1000_0101]);
        assert_eq!(reader.read_signed(8).unwrap(), -5);
    }

    #[test]
    fn rejects_invalid_read_widths_without_panicking() {
        let mut reader = BitReader::new(&[0xff; 9]);
        assert!(reader.read(65).is_err());
        assert!(reader.read_signed(65).is_err());
    }

    #[test]
    fn reads_bits_written_by_writer() {
        let mut writer = BitWriter::new();
        writer.write(0b101, 3).unwrap();
        writer.write(0b1111_0000, 8).unwrap();

        let bytes = writer.into_bytes();
        let mut reader = BitReader::new(&bytes);
        assert_eq!(reader.read(3).unwrap(), 0b101);
        assert_eq!(reader.read(8).unwrap(), 0b1111_0000);
    }

    #[test]
    fn writes_msb_first_and_pads_final_byte() {
        let mut writer = BitWriter::new();
        writer.write(0b101, 3).unwrap();
        writer.write(0b10010, 5).unwrap();
        writer.write(0b0110, 4).unwrap();

        assert_eq!(writer.bit_len(), 12);
        assert_eq!(writer.as_bytes(), &[0b1011_0010, 0b0110_0000]);
    }

    #[test]
    fn chunked_writer_matches_bit_reference_across_boundaries() {
        let fields = [
            (0u64, 0usize),
            (1, 1),
            (0b10, 2),
            (0b1_0110, 5),
            (0xab, 8),
            (0x12_3456, 21),
            (0x0123_4567_89ab_cdef, 64),
            (0b101, 3),
        ];
        let mut writer = BitWriter::new();
        let mut expected = Vec::new();
        for (value, width) in fields {
            writer.write(value, width).unwrap();
            for shift in (0..width).rev() {
                expected.push((value >> shift) & 1 != 0);
            }
        }

        assert_eq!(writer.bit_len(), expected.len());
        for (offset, expected) in expected.into_iter().enumerate() {
            assert_eq!(read_bit(writer.as_bytes(), offset).unwrap(), expected);
        }
    }

    #[test]
    fn aligns_to_byte_and_tracks_lengths() {
        let mut writer = BitWriter::new();
        writer.write(0b101, 3).unwrap();
        assert_eq!(writer.bit_len(), 3);
        assert_eq!(writer.byte_len(), 1);

        writer.align_to_byte().unwrap();
        assert_eq!(writer.bit_len(), 8);
        assert_eq!(writer.byte_len(), 1);

        writer.write(0xff, 8).unwrap();
        assert_eq!(writer.bit_len(), 16);
        assert_eq!(writer.byte_len(), 2);
        assert_eq!(writer.as_bytes(), &[0b1010_0000, 0xff]);
    }

    #[test]
    fn rejects_values_that_do_not_fit_width() {
        let mut writer = BitWriter::new();
        assert!(writer.write(0b100, 2).is_err());
        assert!(writer.write(0, 65).is_err());
    }
}
