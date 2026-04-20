//! Shared binary encoding primitives used by GRIB section parsers and writers.

use crate::error::{Error, Result};

pub const U24_MAX: u32 = 0x00ff_ffff;

const WMO_I8_MAX: i64 = 0x7f;
const WMO_I16_MAX: i64 = 0x7fff;
const WMO_I24_MAX: i64 = 0x7f_ffff;
const WMO_I32_MAX: i64 = 0x7fff_ffff;
const IBM_FRACTION_SCALE: f64 = 16_777_216.0;

pub fn write_u8_be(out: &mut Vec<u8>, value: u8) -> Result<()> {
    reserve_bytes(out, 1)?;
    out.push(value);
    Ok(())
}

pub fn write_u16_be(out: &mut Vec<u8>, value: u16) -> Result<()> {
    reserve_bytes(out, 2)?;
    out.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

pub fn write_u24_be(out: &mut Vec<u8>, value: u32) -> Result<()> {
    if value > U24_MAX {
        return Err(Error::Other(format!(
            "value {value} does not fit in an unsigned 24-bit integer"
        )));
    }

    reserve_bytes(out, 3)?;
    out.extend_from_slice(&[
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ]);
    Ok(())
}

pub fn write_u32_be(out: &mut Vec<u8>, value: u32) -> Result<()> {
    reserve_bytes(out, 4)?;
    out.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

pub fn write_u64_be(out: &mut Vec<u8>, value: u64) -> Result<()> {
    reserve_bytes(out, 8)?;
    out.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

pub fn read_u24_be(bytes: &[u8]) -> Option<u32> {
    let bytes = bytes.get(..3)?;
    Some((u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]))
}

pub fn decode_wmo_i8(byte: u8) -> i16 {
    let magnitude = i16::from(byte & 0x7f);
    if byte & 0x80 == 0 {
        magnitude
    } else {
        -magnitude
    }
}

pub fn decode_wmo_i16(bytes: &[u8]) -> Option<i16> {
    let raw = u16::from_be_bytes(bytes.get(..2)?.try_into().ok()?);
    let magnitude = (raw & 0x7fff) as i16;
    Some(if raw & 0x8000 == 0 {
        magnitude
    } else {
        -magnitude
    })
}

pub fn decode_wmo_i24(bytes: &[u8]) -> Option<i32> {
    let raw = read_u24_be(bytes)?;
    let magnitude = (raw & 0x7f_ffff) as i32;
    Some(if raw & 0x80_0000 == 0 {
        magnitude
    } else {
        -magnitude
    })
}

pub fn decode_wmo_i32(bytes: &[u8]) -> Option<i32> {
    let raw = u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?);
    let magnitude = (raw & 0x7fff_ffff) as i32;
    Some(if raw & 0x8000_0000 == 0 {
        magnitude
    } else {
        -magnitude
    })
}

pub fn encode_wmo_i8(value: i16) -> Option<u8> {
    let magnitude = checked_magnitude(value, WMO_I8_MAX)?;
    Some(if value < 0 {
        0x80 | magnitude as u8
    } else {
        magnitude as u8
    })
}

pub fn encode_wmo_i16(value: i16) -> Option<[u8; 2]> {
    let magnitude = checked_magnitude(value, WMO_I16_MAX)? as u16;
    Some(if value < 0 {
        (0x8000 | magnitude).to_be_bytes()
    } else {
        magnitude.to_be_bytes()
    })
}

pub fn encode_wmo_i24(value: i32) -> Option<[u8; 3]> {
    let magnitude = checked_magnitude(value, WMO_I24_MAX)? as u32;
    let raw = if value < 0 {
        0x80_0000 | magnitude
    } else {
        magnitude
    };
    Some([
        ((raw >> 16) & 0xff) as u8,
        ((raw >> 8) & 0xff) as u8,
        (raw & 0xff) as u8,
    ])
}

pub fn encode_wmo_i32(value: i32) -> Option<[u8; 4]> {
    let magnitude = checked_magnitude(value, WMO_I32_MAX)? as u32;
    Some(if value < 0 {
        (0x8000_0000 | magnitude).to_be_bytes()
    } else {
        magnitude.to_be_bytes()
    })
}

pub fn decode_ibm_f32(bytes: [u8; 4]) -> f32 {
    if bytes == [0, 0, 0, 0] {
        return 0.0;
    }

    let sign = if bytes[0] & 0x80 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from(bytes[0] & 0x7f) - 64;
    let mantissa = (u32::from(bytes[1]) << 16) | (u32::from(bytes[2]) << 8) | u32::from(bytes[3]);
    let value = sign * f64::from(mantissa) / IBM_FRACTION_SCALE * 16f64.powi(exponent);
    value as f32
}

pub fn decode_ibm_f32_slice(bytes: &[u8]) -> Option<f32> {
    Some(decode_ibm_f32(bytes.get(..4)?.try_into().ok()?))
}

pub fn encode_ibm_f32(value: f32) -> Option<[u8; 4]> {
    if value == 0.0 {
        return Some([0, 0, 0, 0]);
    }
    if !value.is_finite() {
        return None;
    }

    let sign = if value.is_sign_negative() { 0x80 } else { 0x00 };
    let mut fraction = f64::from(value.abs());
    let mut exponent = 64i32;

    while fraction < 0.0625 {
        fraction *= 16.0;
        exponent -= 1;
        if exponent < 0 {
            return None;
        }
    }

    while fraction >= 1.0 {
        fraction /= 16.0;
        exponent += 1;
        if exponent > 0x7f {
            return None;
        }
    }

    let mut mantissa = (fraction * IBM_FRACTION_SCALE).round() as u32;
    if mantissa == 0 {
        return Some([0, 0, 0, 0]);
    }
    if mantissa >= 0x0100_0000 {
        mantissa >>= 4;
        exponent += 1;
        if exponent > 0x7f {
            return None;
        }
    }

    Some([
        sign | exponent as u8,
        ((mantissa >> 16) & 0xff) as u8,
        ((mantissa >> 8) & 0xff) as u8,
        (mantissa & 0xff) as u8,
    ])
}

fn reserve_bytes(out: &mut Vec<u8>, additional: usize) -> Result<()> {
    out.try_reserve(additional)
        .map_err(|e| Error::Other(format!("failed to reserve {additional} bytes: {e}")))
}

fn checked_magnitude<T>(value: T, max: i64) -> Option<i64>
where
    T: Into<i64>,
{
    let value = value.into();
    let magnitude = value.checked_abs()?;
    (magnitude <= max).then_some(magnitude)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_ibm_f32, decode_ibm_f32_slice, decode_wmo_i16, decode_wmo_i24, decode_wmo_i32,
        decode_wmo_i8, encode_ibm_f32, encode_wmo_i16, encode_wmo_i24, encode_wmo_i32,
        encode_wmo_i8, read_u24_be, write_u16_be, write_u24_be, write_u32_be, write_u64_be,
        write_u8_be,
    };

    #[test]
    fn writes_big_endian_unsigned_integers() {
        let mut out = Vec::new();
        write_u8_be(&mut out, 0x12).unwrap();
        write_u16_be(&mut out, 0x3456).unwrap();
        write_u24_be(&mut out, 0x789abc).unwrap();
        write_u32_be(&mut out, 0xdef0_1234).unwrap();
        write_u64_be(&mut out, 0x5678_9abc_def0_1234).unwrap();

        assert_eq!(
            out,
            [
                0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
                0xde, 0xf0, 0x12, 0x34
            ]
        );
    }

    #[test]
    fn rejects_unsigned_24_bit_overflow() {
        let mut out = Vec::new();
        assert!(write_u24_be(&mut out, 0x0100_0000).is_err());
        assert!(out.is_empty());
    }

    #[test]
    fn reads_unsigned_24_bit_integers() {
        assert_eq!(read_u24_be(&[0x12, 0x34, 0x56]), Some(0x12_3456));
        assert_eq!(read_u24_be(&[0x12, 0x34]), None);
    }

    #[test]
    fn roundtrips_wmo_signed_integers() {
        assert_eq!(decode_wmo_i8(encode_wmo_i8(-5).unwrap()), -5);
        assert_eq!(decode_wmo_i16(&encode_wmo_i16(-500).unwrap()), Some(-500));
        assert_eq!(
            decode_wmo_i24(&encode_wmo_i24(-500_000).unwrap()),
            Some(-500_000)
        );
        assert_eq!(
            decode_wmo_i32(&encode_wmo_i32(-500_000_000).unwrap()),
            Some(-500_000_000)
        );
    }

    #[test]
    fn matches_known_wmo_signed_byte_patterns() {
        assert_eq!(decode_wmo_i8(0x85), -5);
        assert_eq!(decode_wmo_i16(&[0x80, 0x05]), Some(-5));
        assert_eq!(decode_wmo_i24(&[0x80, 0x00, 0x05]), Some(-5));
        assert_eq!(decode_wmo_i32(&[0x80, 0x00, 0x00, 0x05]), Some(-5));
        assert_eq!(encode_wmo_i8(5), Some(0x05));
        assert_eq!(encode_wmo_i16(5), Some([0x00, 0x05]));
        assert_eq!(encode_wmo_i24(5), Some([0x00, 0x00, 0x05]));
        assert_eq!(encode_wmo_i32(5), Some([0x00, 0x00, 0x00, 0x05]));
    }

    #[test]
    fn rejects_wmo_signed_integer_overflow() {
        assert_eq!(encode_wmo_i8(128), None);
        assert_eq!(encode_wmo_i16(i16::MIN), None);
        assert_eq!(encode_wmo_i24(0x80_0000), None);
        assert_eq!(encode_wmo_i32(i32::MIN), None);
        assert_eq!(decode_wmo_i16(&[0x00]), None);
        assert_eq!(decode_wmo_i24(&[0x00, 0x00]), None);
        assert_eq!(decode_wmo_i32(&[0x00, 0x00, 0x00]), None);
    }

    #[test]
    fn decodes_known_ibm_float_patterns() {
        assert_eq!(decode_ibm_f32([0x00, 0x00, 0x00, 0x00]), 0.0);
        assert_eq!(decode_ibm_f32([0x41, 0x10, 0x00, 0x00]), 1.0);
        assert_eq!(decode_ibm_f32([0xc1, 0x20, 0x00, 0x00]), -2.0);
        assert_eq!(decode_ibm_f32([0x41, 0xa0, 0x00, 0x00]), 10.0);
        assert_eq!(decode_ibm_f32_slice(&[0x41, 0x10, 0x00, 0x00]), Some(1.0));
        assert_eq!(decode_ibm_f32_slice(&[0x41, 0x10, 0x00]), None);
    }

    #[test]
    fn roundtrips_finite_ibm_float_values() {
        for value in [0.0_f32, 0.25, 0.5, 1.0, -2.0, 10.0, 16.0, 1234.5] {
            let encoded = encode_ibm_f32(value).unwrap();
            let decoded = decode_ibm_f32(encoded);
            let tolerance = value.abs().max(1.0) * 1.0e-6;
            assert!(
                (decoded - value).abs() <= tolerance,
                "value {value} encoded as {encoded:02x?} decoded as {decoded}"
            );
        }
    }

    #[test]
    fn encodes_known_ibm_float_patterns() {
        assert_eq!(encode_ibm_f32(0.0), Some([0x00, 0x00, 0x00, 0x00]));
        assert_eq!(encode_ibm_f32(1.0), Some([0x41, 0x10, 0x00, 0x00]));
        assert_eq!(encode_ibm_f32(-2.0), Some([0xc1, 0x20, 0x00, 0x00]));
        assert_eq!(encode_ibm_f32(10.0), Some([0x41, 0xa0, 0x00, 0x00]));
    }

    #[test]
    fn rejects_non_finite_ibm_float_values() {
        assert_eq!(encode_ibm_f32(f32::NAN), None);
        assert_eq!(encode_ibm_f32(f32::INFINITY), None);
        assert_eq!(encode_ibm_f32(f32::NEG_INFINITY), None);
    }
}
