//! Bit-level readers and writers for GRIB packing templates.

use crate::error::{Error, Result};

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
            self.bit_offset += take;
            remaining -= take;
        }

        Ok(value)
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

        for bit_index in (0..bit_count).rev() {
            if self.bit_offset % 8 == 0 {
                self.bytes.push(0);
            }

            let bit = ((value >> bit_index) & 1) as u8;
            if bit != 0 {
                let byte_index = self.bit_offset / 8;
                let shift = 7 - (self.bit_offset % 8);
                self.bytes[byte_index] |= 1 << shift;
            }
            self.bit_offset += 1;
        }

        Ok(())
    }

    pub fn align_to_byte(&mut self) {
        let remainder = self.bit_offset % 8;
        if remainder != 0 {
            self.bit_offset += 8 - remainder;
        }
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
    use super::{BitReader, BitWriter};

    #[test]
    fn reads_msb_first_across_byte_boundaries() {
        let mut reader = BitReader::new(&[0b1011_0010, 0b0110_0000]);

        assert_eq!(reader.read(3).unwrap(), 0b101);
        assert_eq!(reader.read(5).unwrap(), 0b10010);
        assert_eq!(reader.read(4).unwrap(), 0b0110);
        assert_eq!(reader.bit_offset(), 12);
    }

    #[test]
    fn reads_grib_style_signed_magnitudes() {
        let mut reader = BitReader::new(&[0b1000_0101]);
        assert_eq!(reader.read_signed(8).unwrap(), -5);
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
    fn rejects_values_that_do_not_fit_width() {
        let mut writer = BitWriter::new();
        assert!(writer.write(0b100, 2).is_err());
    }
}
