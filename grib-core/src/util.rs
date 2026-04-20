pub fn grib_i8(byte: u8) -> i16 {
    crate::binary::decode_wmo_i8(byte)
}

pub fn grib_i24(bytes: &[u8]) -> Option<i32> {
    crate::binary::decode_wmo_i24(bytes)
}

pub fn grib_i16(bytes: &[u8]) -> Option<i16> {
    crate::binary::decode_wmo_i16(bytes)
}

pub fn grib_i32(bytes: &[u8]) -> Option<i32> {
    crate::binary::decode_wmo_i32(bytes)
}

pub fn encode_grib_i8(value: i16) -> Option<u8> {
    crate::binary::encode_wmo_i8(value)
}

pub fn encode_grib_i16(value: i16) -> Option<[u8; 2]> {
    crate::binary::encode_wmo_i16(value)
}

pub fn encode_grib_i24(value: i32) -> Option<[u8; 3]> {
    crate::binary::encode_wmo_i24(value)
}

pub fn encode_grib_i32(value: i32) -> Option<[u8; 4]> {
    crate::binary::encode_wmo_i32(value)
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
