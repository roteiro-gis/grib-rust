pub fn grib_i8(byte: u8) -> i16 {
    let magnitude = (byte & 0x7f) as i16;
    if byte & 0x80 == 0 {
        magnitude
    } else {
        -magnitude
    }
}

pub fn grib_i24(bytes: &[u8]) -> Option<i32> {
    let raw = ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32);
    let magnitude = (raw & 0x7f_ffff) as i32;
    Some(if raw & 0x80_0000 == 0 {
        magnitude
    } else {
        -magnitude
    })
}

pub fn grib_i16(bytes: &[u8]) -> Option<i16> {
    let raw = u16::from_be_bytes(bytes.try_into().ok()?);
    let magnitude = (raw & 0x7fff) as i16;
    Some(if raw & 0x8000 == 0 {
        magnitude
    } else {
        -magnitude
    })
}

pub fn grib_i32(bytes: &[u8]) -> Option<i32> {
    let raw = u32::from_be_bytes(bytes.try_into().ok()?);
    let magnitude = (raw & 0x7fff_ffff) as i32;
    Some(if raw & 0x8000_0000 == 0 {
        magnitude
    } else {
        -magnitude
    })
}

pub fn encode_grib_i8(value: i16) -> Option<u8> {
    let magnitude = checked_magnitude(value, 0x7f)?;
    Some(if value < 0 {
        0x80 | magnitude as u8
    } else {
        magnitude as u8
    })
}

pub fn encode_grib_i16(value: i16) -> Option<[u8; 2]> {
    let magnitude = checked_magnitude(value, 0x7fff)? as u16;
    Some(if value < 0 {
        (0x8000 | magnitude).to_be_bytes()
    } else {
        magnitude.to_be_bytes()
    })
}

pub fn encode_grib_i24(value: i32) -> Option<[u8; 3]> {
    let magnitude = checked_magnitude(value, 0x7f_ffff)? as u32;
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

pub fn encode_grib_i32(value: i32) -> Option<[u8; 4]> {
    let magnitude = checked_magnitude(value, 0x7fff_ffff)? as u32;
    Some(if value < 0 {
        (0x8000_0000 | magnitude).to_be_bytes()
    } else {
        magnitude.to_be_bytes()
    })
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
        encode_grib_i16, encode_grib_i24, encode_grib_i32, encode_grib_i8, grib_i16, grib_i24,
        grib_i32, grib_i8,
    };

    #[test]
    fn parses_signed_grib_i8() {
        assert_eq!(grib_i8(0x05), 5);
        assert_eq!(grib_i8(0x85), -5);
    }

    #[test]
    fn parses_signed_grib_i16() {
        assert_eq!(grib_i16(&0x0005u16.to_be_bytes()), Some(5));
        assert_eq!(grib_i16(&0x8005u16.to_be_bytes()), Some(-5));
    }

    #[test]
    fn parses_signed_grib_i24() {
        assert_eq!(grib_i24(&[0x00, 0x00, 0x05]), Some(5));
        assert_eq!(grib_i24(&[0x80, 0x00, 0x05]), Some(-5));
    }

    #[test]
    fn parses_signed_grib_i32() {
        assert_eq!(grib_i32(&0x0000_0005u32.to_be_bytes()), Some(5));
        assert_eq!(grib_i32(&0x8000_0005u32.to_be_bytes()), Some(-5));
    }

    #[test]
    fn encodes_signed_grib_integers() {
        assert_eq!(encode_grib_i8(5), Some(0x05));
        assert_eq!(encode_grib_i8(-5), Some(0x85));
        assert_eq!(encode_grib_i16(5), Some([0x00, 0x05]));
        assert_eq!(encode_grib_i16(-5), Some([0x80, 0x05]));
        assert_eq!(encode_grib_i24(5), Some([0x00, 0x00, 0x05]));
        assert_eq!(encode_grib_i24(-5), Some([0x80, 0x00, 0x05]));
        assert_eq!(encode_grib_i32(5), Some([0x00, 0x00, 0x00, 0x05]));
        assert_eq!(encode_grib_i32(-5), Some([0x80, 0x00, 0x00, 0x05]));
    }

    #[test]
    fn rejects_signed_grib_magnitude_overflow() {
        assert_eq!(encode_grib_i8(128), None);
        assert_eq!(encode_grib_i16(i16::MIN), None);
        assert_eq!(encode_grib_i24(0x80_0000), None);
        assert_eq!(encode_grib_i32(i32::MIN), None);
    }
}
