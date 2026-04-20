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
}

impl<'a> BitReader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_offset: 0,
        }
    }

    pub const fn with_offset(data: &'a [u8], bit_offset: usize) -> Self {
        Self { data, bit_offset }
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
            .ok_or_else(|| Error::Other("bit offset overflow".into()))?;

        let mut remaining = bit_count;
        let mut value = 0u64;

        while remaining > 0 {
            let byte_index = self.bit_offset / 8;
            let bit_index = self.bit_offset % 8;
            let byte = *self.data.get(byte_index).ok_or(Error::Truncated {
                offset: byte_index as u64,
            })?;
            let available = 8 - bit_index;
            let take = remaining.min(available);
            let mask = ((1u16 << take) - 1) as u8;
            let shift = available - take;
            let bits = (byte >> shift) & mask;

            value = (value << take) | u64::from(bits);
            self.bit_offset = self
                .bit_offset
                .checked_add(take)
                .ok_or_else(|| Error::Other("bit offset overflow".into()))?;
            remaining -= take;
        }

        debug_assert_eq!(self.bit_offset, end_bit_offset);
        Ok(value)
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
                .map_err(|_| Error::Other("signed value exceeds i64 range".into()));
        }

        let magnitude_mask = sign_mask - 1;
        let magnitude = value & magnitude_mask;
        let magnitude = i64::try_from(magnitude)
            .map_err(|_| Error::Other("signed value exceeds i64 range".into()))?;
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
            return Err(Error::Other(
                "non-zero value cannot be written with zero bits".into(),
            ));
        }
        require_u64_width(bit_count)?;
        if bit_count < u64::BITS as usize && (value >> bit_count) != 0 {
            return Err(Error::Other(format!(
                "value {value} does not fit in {bit_count} bits"
            )));
        }

        let start_bit_offset = self.bit_offset;
        let end_bit_offset = self
            .bit_offset
            .checked_add(bit_count)
            .ok_or_else(|| Error::Other("bit offset overflow".into()))?;
        let required_bytes = end_bit_offset.div_ceil(8);
        if required_bytes > self.bytes.len() {
            let additional = required_bytes - self.bytes.len();
            self.bytes
                .try_reserve(additional)
                .map_err(|e| Error::Other(format!("failed to reserve {additional} bytes: {e}")))?;
            self.bytes.resize(required_bytes, 0);
        }

        for target_index in 0..bit_count {
            let source_shift = bit_count - 1 - target_index;
            let bit = ((value >> source_shift) & 1) as u8;
            if bit != 0 {
                let target_offset = start_bit_offset + target_index;
                let byte_index = target_offset / 8;
                let shift = 7 - (target_offset % 8);
                self.bytes[byte_index] |= 1 << shift;
            }
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
                .ok_or_else(|| Error::Other("bit offset overflow".into()))?;
        }
        Ok(())
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
