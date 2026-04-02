#![allow(dead_code)]

pub fn build_grib2_message(values: &[u8]) -> Vec<u8> {
    assemble_grib2_message(&[
        build_identification(),
        build_grid(2, 2, 0),
        build_product(0, 0),
        build_simple_representation(values.len(), 8),
        build_data(values),
    ])
}

pub fn build_grib2_complex_packing_message() -> Vec<u8> {
    let mut payload = BitPacker::new();
    for group_reference in [3u64, 5] {
        payload.push(group_reference, 3);
    }
    payload.align_to_byte();
    for group_width_delta in [1u64, 3] {
        payload.push(group_width_delta, 2);
    }
    payload.align_to_byte();
    for (value, bits) in [(0u64, 1usize), (1, 1), (0, 3), (4, 3)] {
        payload.push(value, bits);
    }

    assemble_grib2_message(&[
        build_identification(),
        build_grid(2, 2, 0),
        build_product(0, 0),
        build_complex_representation(ComplexRepresentationSpec {
            encoded_values: 4,
            template: 2,
            group_reference_bits: 3,
            missing_value_management: 0,
            num_groups: 2,
            group_width_reference: 0,
            group_width_bits: 2,
            group_length_reference: 2,
            group_length_increment: 1,
            true_length_last_group: 2,
            scaled_group_length_bits: 0,
            spatial_order: None,
            descriptor_octets: None,
        }),
        build_data(&payload.finish()),
    ])
}

pub fn build_grib2_complex_packing_message_with_missing() -> Vec<u8> {
    let mut payload = BitPacker::new();
    for group_reference in [7u64, 9] {
        payload.push(group_reference, 4);
    }
    payload.align_to_byte();
    for group_width_delta in [2u64, 1] {
        payload.push(group_width_delta, 2);
    }
    payload.align_to_byte();
    for (value, bits) in [(0u64, 2usize), (3, 2), (0, 1), (1, 1)] {
        payload.push(value, bits);
    }

    assemble_grib2_message(&[
        build_identification(),
        build_grid(2, 2, 0),
        build_product(0, 0),
        build_complex_representation(ComplexRepresentationSpec {
            encoded_values: 4,
            template: 2,
            group_reference_bits: 4,
            missing_value_management: 1,
            num_groups: 2,
            group_width_reference: 0,
            group_width_bits: 2,
            group_length_reference: 2,
            group_length_increment: 1,
            true_length_last_group: 2,
            scaled_group_length_bits: 0,
            spatial_order: None,
            descriptor_octets: None,
        }),
        build_data(&payload.finish()),
    ])
}

pub fn build_grib2_spatial_differencing_message() -> Vec<u8> {
    let mut payload = BitPacker::new();
    for descriptor in [10u64, 2] {
        payload.push(descriptor, 16);
    }
    for group_reference in [0u64, 1] {
        payload.push(group_reference, 2);
    }
    payload.align_to_byte();
    for group_width_delta in [0u64, 1] {
        payload.push(group_width_delta, 1);
    }
    payload.align_to_byte();
    for (value, bits) in [(0u64, 1usize), (1, 1)] {
        payload.push(value, bits);
    }

    assemble_grib2_message(&[
        build_identification(),
        build_grid(2, 2, 0),
        build_product(0, 0),
        build_complex_representation(ComplexRepresentationSpec {
            encoded_values: 4,
            template: 3,
            group_reference_bits: 2,
            missing_value_management: 0,
            num_groups: 2,
            group_width_reference: 0,
            group_width_bits: 1,
            group_length_reference: 2,
            group_length_increment: 1,
            true_length_last_group: 2,
            scaled_group_length_bits: 0,
            spatial_order: Some(1),
            descriptor_octets: Some(2),
        }),
        build_data(&payload.finish()),
    ])
}

pub fn build_grib2_message_with_forecast(values: &[u8], forecast_time: u32) -> Vec<u8> {
    assemble_grib2_message(&[
        build_identification(),
        build_grid(2, 2, 0),
        build_product_with_forecast(0, 0, forecast_time),
        build_simple_representation(values.len(), 8),
        build_data(values),
    ])
}

pub fn build_grib2_multifield_message() -> Vec<u8> {
    assemble_grib2_message(&[
        build_identification(),
        build_grid(2, 2, 0),
        build_product(0, 0),
        build_simple_representation(4, 8),
        build_data(&[1, 2, 3, 4]),
        build_product(0, 2),
        build_simple_representation(4, 8),
        build_data(&[5, 6, 7, 8]),
    ])
}

pub fn build_bitmap_prefixed_stream() -> Vec<u8> {
    let mut bytes = b"junkGRIB\x00\x00\x00\x02not-a-real-message".to_vec();
    bytes.extend_from_slice(&build_grib2_message(&[9, 8, 7, 6]));
    bytes
}

pub fn build_truncated_grib2_message() -> Vec<u8> {
    let message = build_grib2_message(&[1, 2, 3, 4]);
    message[..message.len() - 2].to_vec()
}

pub fn build_grib1_bitmap_message() -> Vec<u8> {
    build_grib1_message_with_bitmap(&[9, 7], 3, 1, Some(&[0b1011_1111]))
}

pub fn build_grib1_message(values: &[u8]) -> Vec<u8> {
    build_grib1_message_with_bitmap(values, 2, 2, None)
}

pub fn build_grib1_message_with_bitmap(
    values: &[u8],
    ni: u16,
    nj: u16,
    bitmap_payload: Option<&[u8]>,
) -> Vec<u8> {
    let mut pds = vec![0u8; 28];
    pds[..3].copy_from_slice(&[0, 0, 28]);
    pds[3] = 2;
    pds[4] = 7;
    pds[5] = 255;
    pds[6] = 0;
    pds[7] = 0b1000_0000
        | if bitmap_payload.is_some() {
            0b0100_0000
        } else {
            0
        };
    pds[8] = 11;
    pds[9] = 100;
    pds[10..12].copy_from_slice(&850u16.to_be_bytes());
    pds[12] = 26;
    pds[13] = 3;
    pds[14] = 20;
    pds[15] = 12;
    pds[16] = 0;
    pds[17] = 1;
    pds[18] = 0;
    pds[19] = 0;
    pds[20] = 0;
    pds[24] = 21;
    pds[25] = 0;

    let mut gds = vec![0u8; 32];
    gds[..3].copy_from_slice(&[0, 0, 32]);
    gds[5] = 0;
    gds[6..8].copy_from_slice(&ni.to_be_bytes());
    gds[8..10].copy_from_slice(&nj.to_be_bytes());
    gds[10..13].copy_from_slice(&[0x01, 0x4d, 0x50]);
    gds[13..16].copy_from_slice(&[0x81, 0xd4, 0xc0]);
    gds[16] = 0x80;
    gds[17..20].copy_from_slice(&[0x01, 0x49, 0x68]);
    gds[20..23].copy_from_slice(&[0x81, 0xd0, 0xd8]);
    gds[23..25].copy_from_slice(&1000u16.to_be_bytes());
    gds[25..27].copy_from_slice(&1000u16.to_be_bytes());

    let mut bds = vec![0u8; 11 + values.len()];
    let len = bds.len() as u32;
    bds[..3].copy_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
    bds[3] = 0;
    bds[10] = 8;
    bds[11..].copy_from_slice(values);

    let bitmap = bitmap_payload.map(|payload| {
        let mut section = vec![0u8; payload.len() + 6];
        let len = section.len() as u32;
        section[..3].copy_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
        section[4..6].copy_from_slice(&0u16.to_be_bytes());
        section[6..].copy_from_slice(payload);
        section
    });

    let total_len = 8 + pds.len() + gds.len() + bitmap.as_ref().map_or(0, Vec::len) + bds.len() + 4;
    let mut message = Vec::new();
    message.extend_from_slice(b"GRIB");
    message.extend_from_slice(&[
        ((total_len >> 16) & 0xff) as u8,
        ((total_len >> 8) & 0xff) as u8,
        (total_len & 0xff) as u8,
        1,
    ]);
    message.extend_from_slice(&pds);
    message.extend_from_slice(&gds);
    if let Some(bitmap) = bitmap {
        message.extend_from_slice(&bitmap);
    }
    message.extend_from_slice(&bds);
    message.extend_from_slice(b"7777");
    message
}

fn build_indicator(total_len: usize, discipline: u8) -> Vec<u8> {
    let mut indicator = Vec::with_capacity(16);
    indicator.extend_from_slice(b"GRIB");
    indicator.extend_from_slice(&[0, 0]);
    indicator.push(discipline);
    indicator.push(2);
    indicator.extend_from_slice(&(total_len as u64).to_be_bytes());
    indicator
}

fn build_identification() -> Vec<u8> {
    let mut section = vec![0u8; 21];
    section[..4].copy_from_slice(&(21u32).to_be_bytes());
    section[4] = 1;
    section[5..7].copy_from_slice(&7u16.to_be_bytes());
    section[7..9].copy_from_slice(&0u16.to_be_bytes());
    section[9] = 35;
    section[10] = 1;
    section[11] = 1;
    section[12..14].copy_from_slice(&2026u16.to_be_bytes());
    section[14] = 3;
    section[15] = 20;
    section[16] = 12;
    section[17] = 0;
    section[18] = 0;
    section[19] = 0;
    section[20] = 1;
    section
}

fn build_product(parameter_category: u8, parameter_number: u8) -> Vec<u8> {
    build_product_with_forecast(parameter_category, parameter_number, 0)
}

fn build_product_with_forecast(
    parameter_category: u8,
    parameter_number: u8,
    forecast_time: u32,
) -> Vec<u8> {
    let mut section = vec![0u8; 34];
    section[..4].copy_from_slice(&(34u32).to_be_bytes());
    section[4] = 4;
    section[7..9].copy_from_slice(&0u16.to_be_bytes());
    section[9] = parameter_category;
    section[10] = parameter_number;
    section[11] = 2;
    section[17] = 1;
    section[18..22].copy_from_slice(&forecast_time.to_be_bytes());
    section[22] = 103;
    section[23] = 0;
    section[24..28].copy_from_slice(&850u32.to_be_bytes());
    section[28] = 255;
    section
}

fn build_simple_representation(encoded_values: usize, bits_per_value: u8) -> Vec<u8> {
    let mut section = vec![0u8; 21];
    section[..4].copy_from_slice(&(21u32).to_be_bytes());
    section[4] = 5;
    section[5..9].copy_from_slice(&(encoded_values as u32).to_be_bytes());
    section[9..11].copy_from_slice(&0u16.to_be_bytes());
    section[11..15].copy_from_slice(&0f32.to_be_bytes());
    section[19] = bits_per_value;
    section[20] = 0;
    section
}

fn build_complex_representation(spec: ComplexRepresentationSpec) -> Vec<u8> {
    let has_spatial = spec.template == 3;
    let mut section = vec![0u8; if has_spatial { 49 } else { 47 }];
    let len = section.len() as u32;
    section[..4].copy_from_slice(&len.to_be_bytes());
    section[4] = 5;
    section[5..9].copy_from_slice(&(spec.encoded_values as u32).to_be_bytes());
    section[9..11].copy_from_slice(&(spec.template as u16).to_be_bytes());
    section[11..15].copy_from_slice(&0f32.to_be_bytes());
    section[19] = spec.group_reference_bits;
    section[21] = 1;
    section[22] = spec.missing_value_management;
    section[23..27].copy_from_slice(&u32::MAX.to_be_bytes());
    section[27..31].copy_from_slice(&u32::MAX.to_be_bytes());
    section[31..35].copy_from_slice(&(spec.num_groups as u32).to_be_bytes());
    section[35] = spec.group_width_reference;
    section[36] = spec.group_width_bits;
    section[37..41].copy_from_slice(&spec.group_length_reference.to_be_bytes());
    section[41] = spec.group_length_increment;
    section[42..46].copy_from_slice(&spec.true_length_last_group.to_be_bytes());
    section[46] = spec.scaled_group_length_bits;
    if has_spatial {
        section[47] = spec.spatial_order.unwrap();
        section[48] = spec.descriptor_octets.unwrap();
    }
    section
}

fn build_grid(ni: u32, nj: u32, scanning_mode: u8) -> Vec<u8> {
    let mut section = vec![0u8; 72];
    section[..4].copy_from_slice(&(72u32).to_be_bytes());
    section[4] = 3;
    section[6..10].copy_from_slice(&(ni * nj).to_be_bytes());
    section[12..14].copy_from_slice(&0u16.to_be_bytes());
    section[30..34].copy_from_slice(&ni.to_be_bytes());
    section[34..38].copy_from_slice(&nj.to_be_bytes());
    section[46..50].copy_from_slice(&50_000_000u32.to_be_bytes());
    section[50..54].copy_from_slice(&(0x8000_0000u32 | 120_000_000).to_be_bytes());
    section[55..59].copy_from_slice(&49_000_000u32.to_be_bytes());
    section[59..63].copy_from_slice(&(0x8000_0000u32 | 119_000_000).to_be_bytes());
    section[63..67].copy_from_slice(&1_000_000u32.to_be_bytes());
    section[67..71].copy_from_slice(&1_000_000u32.to_be_bytes());
    section[71] = scanning_mode;
    section
}

fn build_data(payload: &[u8]) -> Vec<u8> {
    let mut section = vec![0u8; payload.len() + 5];
    section[..4].copy_from_slice(&((payload.len() + 5) as u32).to_be_bytes());
    section[4] = 7;
    section[5..].copy_from_slice(payload);
    section
}

fn assemble_grib2_message(sections: &[Vec<u8>]) -> Vec<u8> {
    let total_len = 16 + sections.iter().map(|section| section.len()).sum::<usize>() + 4;
    let mut message = build_indicator(total_len, 0);
    for section in sections {
        message.extend_from_slice(section);
    }
    message.extend_from_slice(b"7777");
    message
}

struct ComplexRepresentationSpec {
    encoded_values: usize,
    template: u8,
    group_reference_bits: u8,
    missing_value_management: u8,
    num_groups: usize,
    group_width_reference: u8,
    group_width_bits: u8,
    group_length_reference: u32,
    group_length_increment: u8,
    true_length_last_group: u32,
    scaled_group_length_bits: u8,
    spatial_order: Option<u8>,
    descriptor_octets: Option<u8>,
}

struct BitPacker {
    bytes: Vec<u8>,
    current: u8,
    used_bits: u8,
}

impl BitPacker {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            current: 0,
            used_bits: 0,
        }
    }

    fn push(&mut self, value: u64, bit_count: usize) {
        for shift in (0..bit_count).rev() {
            let bit = ((value >> shift) & 1) as u8;
            self.current = (self.current << 1) | bit;
            self.used_bits += 1;
            if self.used_bits == 8 {
                self.bytes.push(self.current);
                self.current = 0;
                self.used_bits = 0;
            }
        }
    }

    fn align_to_byte(&mut self) {
        if self.used_bits == 0 {
            return;
        }

        self.current <<= 8 - self.used_bits;
        self.bytes.push(self.current);
        self.current = 0;
        self.used_bits = 0;
    }

    fn finish(mut self) -> Vec<u8> {
        self.align_to_byte();
        self.bytes
    }
}
