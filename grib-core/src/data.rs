//! Data Representation Section (Section 5) shared model.

use crate::binary::decode_wmo_i16;
use crate::error::{Error, Result};

/// Data representation template number and parameters.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DataRepresentation {
    /// Template 5.0: Simple packing.
    SimplePacking(ScaledPackingParams),
    /// Template 5.2/5.3: Complex packing with optional spatial differencing.
    ComplexPacking(ComplexPackingParams),
    /// Template 5.40: JPEG 2000 code stream packing.
    Jpeg2000Packing(Jpeg2000PackingParams),
    /// Template 5.41: PNG image packing.
    PngPacking(PngPackingParams),
    /// Template 5.42: CCSDS 121.0 adaptive entropy coding.
    CcsdsPacking(CcsdsPackingParams),
    /// Unsupported template.
    Unsupported(u16),
}

/// Scaling parameters shared by GRIB2 grid-point packing templates.
///
/// Decoded values use `(reference + packed * 2^binary_scale) *
/// 10^-decimal_scale`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaledPackingParams {
    pub encoded_values: usize,
    pub reference_value: f32,
    pub binary_scale: i16,
    pub decimal_scale: i16,
    pub bits_per_value: u8,
    pub original_field_type: u8,
}

/// Parameters for JPEG 2000 code stream packing (Template 5.40).
#[derive(Debug, Clone, PartialEq)]
pub struct Jpeg2000PackingParams {
    pub packing: ScaledPackingParams,
    pub compression_type: u8,
    pub target_compression_ratio: u8,
}

/// Parameters for PNG image packing (Template 5.41).
#[derive(Debug, Clone, PartialEq)]
pub struct PngPackingParams {
    pub packing: ScaledPackingParams,
}

/// CCSDS/AEC option mask stored by data representation template 5.42.
///
/// The bit assignments match the `libaec` API referenced by the WMO template.
/// Bit 7 is reserved and therefore rejected by [`CcsdsFlags::from_bits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CcsdsFlags(u8);

impl CcsdsFlags {
    /// No optional codec behavior.
    pub const NONE: Self = Self(0);
    /// Treat sample bit patterns as signed during AEC preprocessing.
    ///
    /// GRIB reconstruction still interprets the decoded samples as unsigned
    /// scaled differences from the field reference value.
    pub const SIGNED: Self = Self(1 << 0);
    /// Samples with 17 through 24 bits use three-byte containers.
    pub const THREE_BYTE: Self = Self(1 << 1);
    /// Sample containers store their most-significant byte first.
    pub const MSB: Self = Self(1 << 2);
    /// Apply the CCSDS unit-delay preprocessor.
    pub const PREPROCESS: Self = Self(1 << 3);
    /// Use the restricted code-option set for samples no wider than four bits.
    pub const RESTRICTED: Self = Self(1 << 4);
    /// Decode legacy streams whose reference sample intervals are byte padded.
    pub const PAD_RSI: Self = Self(1 << 5);
    /// Permit non-standard even block sizes.
    pub const NOT_ENFORCE: Self = Self(1 << 6);

    const KNOWN_BITS: u8 = Self::SIGNED.0
        | Self::THREE_BYTE.0
        | Self::MSB.0
        | Self::PREPROCESS.0
        | Self::RESTRICTED.0
        | Self::PAD_RSI.0
        | Self::NOT_ENFORCE.0;

    /// ecCodes' canonical template 5.42 options.
    pub const DEFAULT: Self = Self(Self::THREE_BYTE.0 | Self::MSB.0 | Self::PREPROCESS.0);

    /// Constructs flags when no reserved bits are set.
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !Self::KNOWN_BITS == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    /// Returns the template 5.42 wire representation.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Returns whether all bits in `other` are present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl Default for CcsdsFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl std::ops::BitOr for CcsdsFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for CcsdsFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Number of samples in a CCSDS/AEC coding block.
///
/// Standard streams use 8, 16, 32, or 64 samples. Other non-zero even sizes
/// are representable because `libaec` permits them when
/// [`CcsdsFlags::NOT_ENFORCE`] is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CcsdsBlockSize(u8);

impl CcsdsBlockSize {
    pub const EIGHT: Self = Self(8);
    pub const SIXTEEN: Self = Self(16);
    pub const THIRTY_TWO: Self = Self(32);
    pub const SIXTY_FOUR: Self = Self(64);

    /// Constructs a block size accepted by the AEC codec.
    pub const fn new(samples: u8) -> Option<Self> {
        if samples != 0 && samples.is_multiple_of(2) {
            Some(Self(samples))
        } else {
            None
        }
    }

    pub const fn samples(self) -> u8 {
        self.0
    }

    pub const fn is_standard(self) -> bool {
        matches!(self.0, 8 | 16 | 32 | 64)
    }
}

impl Default for CcsdsBlockSize {
    fn default() -> Self {
        Self::THIRTY_TWO
    }
}

/// Validated parameters for CCSDS 121.0 packing (template 5.42).
#[derive(Debug, Clone, PartialEq)]
pub struct CcsdsPackingParams {
    packing: ScaledPackingParams,
    flags: CcsdsFlags,
    block_size: CcsdsBlockSize,
    reference_sample_interval: u16,
}

impl CcsdsPackingParams {
    /// Constructs a parameter set after validating the interdependent AEC
    /// width, flag, block-size, and reference-interval constraints.
    pub fn new(
        packing: ScaledPackingParams,
        flags: CcsdsFlags,
        block_size: CcsdsBlockSize,
        reference_sample_interval: u16,
    ) -> Result<Self> {
        validate_ccsds_parameters(
            packing.bits_per_value,
            flags,
            block_size,
            reference_sample_interval,
        )
        .map_err(Error::ValueOutOfRange)?;

        Ok(Self {
            packing,
            flags,
            block_size,
            reference_sample_interval,
        })
    }

    pub const fn packing(&self) -> &ScaledPackingParams {
        &self.packing
    }

    pub const fn flags(&self) -> CcsdsFlags {
        self.flags
    }

    pub const fn block_size(&self) -> CcsdsBlockSize {
        self.block_size
    }

    pub const fn reference_sample_interval(&self) -> u16 {
        self.reference_sample_interval
    }
}

/// Parameters for complex packing (Templates 5.2 and 5.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexPackingParams {
    pub encoded_values: usize,
    pub reference_value: f32,
    pub binary_scale: i16,
    pub decimal_scale: i16,
    pub group_reference_bits: u8,
    pub original_field_type: u8,
    pub group_splitting_method: u8,
    pub missing_value_management: u8,
    pub primary_missing_substitute: u32,
    pub secondary_missing_substitute: u32,
    pub num_groups: usize,
    pub group_width_reference: u8,
    pub group_width_bits: u8,
    pub group_length_reference: u32,
    pub group_length_increment: u8,
    pub true_length_last_group: u32,
    pub scaled_group_length_bits: u8,
    pub spatial_differencing: Option<SpatialDifferencingParams>,
}

/// Parameters specific to template 5.3 spatial differencing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialDifferencingParams {
    pub order: u8,
    pub descriptor_octets: u8,
}

impl DataRepresentation {
    pub fn parse(section_bytes: &[u8]) -> Result<Self> {
        if section_bytes.len() < 11 {
            return Err(Error::InvalidSection {
                section: 5,
                reason: format!("expected at least 11 bytes, got {}", section_bytes.len()),
            });
        }
        if section_bytes[4] != 5 {
            return Err(Error::InvalidSection {
                section: section_bytes[4],
                reason: "not a data representation section".into(),
            });
        }

        let template = u16::from_be_bytes(section_bytes[9..11].try_into().unwrap());
        match template {
            0 => parse_simple_packing(section_bytes),
            2 => parse_complex_packing(section_bytes, false),
            3 => parse_complex_packing(section_bytes, true),
            40 => parse_jpeg2000_packing(section_bytes),
            41 => parse_png_packing(section_bytes),
            42 => parse_ccsds_packing(section_bytes),
            _ => Ok(Self::Unsupported(template)),
        }
    }

    pub fn encoded_values(&self) -> Option<usize> {
        match self {
            Self::SimplePacking(params) => Some(params.encoded_values),
            Self::ComplexPacking(params) => Some(params.encoded_values),
            Self::Jpeg2000Packing(params) => Some(params.packing.encoded_values),
            Self::PngPacking(params) => Some(params.packing.encoded_values),
            Self::CcsdsPacking(params) => Some(params.packing().encoded_values),
            Self::Unsupported(_) => None,
        }
    }
}

fn parse_simple_packing(data: &[u8]) -> Result<DataRepresentation> {
    Ok(DataRepresentation::SimplePacking(
        parse_scaled_packing_base(data, 0, 21)?,
    ))
}

fn parse_scaled_packing_base(
    data: &[u8],
    template: u16,
    required: usize,
) -> Result<ScaledPackingParams> {
    if data.len() < required {
        return Err(Error::InvalidSection {
            section: 5,
            reason: format!(
                "template 5.{template} requires {required} bytes, got {}",
                data.len()
            ),
        });
    }

    Ok(ScaledPackingParams {
        encoded_values: u32::from_be_bytes(data[5..9].try_into().unwrap()) as usize,
        reference_value: f32::from_be_bytes(data[11..15].try_into().unwrap()),
        binary_scale: decode_wmo_i16(&data[15..17]).unwrap(),
        decimal_scale: decode_wmo_i16(&data[17..19]).unwrap(),
        bits_per_value: data[19],
        original_field_type: data[20],
    })
}

fn parse_jpeg2000_packing(data: &[u8]) -> Result<DataRepresentation> {
    Ok(DataRepresentation::Jpeg2000Packing(Jpeg2000PackingParams {
        packing: parse_scaled_packing_base(data, 40, 23)?,
        compression_type: data[21],
        target_compression_ratio: data[22],
    }))
}

fn parse_png_packing(data: &[u8]) -> Result<DataRepresentation> {
    Ok(DataRepresentation::PngPacking(PngPackingParams {
        packing: parse_scaled_packing_base(data, 41, 21)?,
    }))
}

fn parse_ccsds_packing(data: &[u8]) -> Result<DataRepresentation> {
    let packing = parse_scaled_packing_base(data, 42, 25)?;
    let flags = CcsdsFlags::from_bits(data[21]).ok_or_else(|| Error::InvalidSection {
        section: 5,
        reason: format!(
            "template 5.42 has reserved CCSDS flag bits set: 0x{:02x}",
            data[21]
        ),
    })?;
    let block_size = CcsdsBlockSize::new(data[22]).ok_or_else(|| Error::InvalidSection {
        section: 5,
        reason: format!(
            "template 5.42 block size must be a non-zero even value, got {}",
            data[22]
        ),
    })?;
    let reference_sample_interval = u16::from_be_bytes(data[23..25].try_into().unwrap());

    validate_ccsds_parameters(
        packing.bits_per_value,
        flags,
        block_size,
        reference_sample_interval,
    )
    .map_err(|reason| Error::InvalidSection {
        section: 5,
        reason: format!("invalid template 5.42 parameters: {reason}"),
    })?;

    Ok(DataRepresentation::CcsdsPacking(CcsdsPackingParams {
        packing,
        flags,
        block_size,
        reference_sample_interval,
    }))
}

fn validate_ccsds_parameters(
    bits_per_value: u8,
    flags: CcsdsFlags,
    block_size: CcsdsBlockSize,
    reference_sample_interval: u16,
) -> std::result::Result<(), String> {
    if bits_per_value > 32 {
        return Err(format!(
            "CCSDS bits per value must be at most 32, got {bits_per_value}"
        ));
    }
    if !block_size.is_standard() && !flags.contains(CcsdsFlags::NOT_ENFORCE) {
        return Err(format!(
            "CCSDS block size {} requires the NOT_ENFORCE flag",
            block_size.samples()
        ));
    }
    if !(1..=4096).contains(&reference_sample_interval) {
        return Err(format!(
            "CCSDS reference sample interval must be between 1 and 4096, got {reference_sample_interval}"
        ));
    }
    if flags.contains(CcsdsFlags::RESTRICTED) && bits_per_value > 4 {
        return Err(format!(
            "CCSDS RESTRICTED flag requires at most 4 bits per value, got {bits_per_value}"
        ));
    }
    Ok(())
}

fn parse_complex_packing(
    data: &[u8],
    with_spatial_differencing: bool,
) -> Result<DataRepresentation> {
    let required = if with_spatial_differencing { 49 } else { 47 };
    if data.len() < required {
        return Err(Error::InvalidSection {
            section: 5,
            reason: format!(
                "template 5.{} requires {required} bytes, got {}",
                if with_spatial_differencing { 3 } else { 2 },
                data.len()
            ),
        });
    }

    let group_splitting_method = data[21];
    if group_splitting_method != 1 {
        return Err(Error::UnsupportedGroupSplittingMethod(
            group_splitting_method,
        ));
    }

    let missing_value_management = data[22];
    if missing_value_management > 2 {
        return Err(Error::UnsupportedMissingValueManagement(
            missing_value_management,
        ));
    }

    let spatial_differencing = if with_spatial_differencing {
        let order = data[47];
        if !matches!(order, 1 | 2) {
            return Err(Error::UnsupportedSpatialDifferencingOrder(order));
        }
        Some(SpatialDifferencingParams {
            order,
            descriptor_octets: data[48],
        })
    } else {
        None
    };

    Ok(DataRepresentation::ComplexPacking(ComplexPackingParams {
        encoded_values: u32::from_be_bytes(data[5..9].try_into().unwrap()) as usize,
        reference_value: f32::from_be_bytes(data[11..15].try_into().unwrap()),
        binary_scale: decode_wmo_i16(&data[15..17]).unwrap(),
        decimal_scale: decode_wmo_i16(&data[17..19]).unwrap(),
        group_reference_bits: data[19],
        original_field_type: data[20],
        group_splitting_method,
        missing_value_management,
        primary_missing_substitute: u32::from_be_bytes(data[23..27].try_into().unwrap()),
        secondary_missing_substitute: u32::from_be_bytes(data[27..31].try_into().unwrap()),
        num_groups: u32::from_be_bytes(data[31..35].try_into().unwrap()) as usize,
        group_width_reference: data[35],
        group_width_bits: data[36],
        group_length_reference: u32::from_be_bytes(data[37..41].try_into().unwrap()),
        group_length_increment: data[41],
        true_length_last_group: u32::from_be_bytes(data[42..46].try_into().unwrap()),
        scaled_group_length_bits: data[46],
        spatial_differencing,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        CcsdsBlockSize, CcsdsFlags, CcsdsPackingParams, DataRepresentation, Jpeg2000PackingParams,
        PngPackingParams, ScaledPackingParams,
    };
    use crate::binary::encode_wmo_i16;

    #[test]
    fn parses_simple_packing_template() {
        let mut section = vec![0u8; 21];
        section[..4].copy_from_slice(&(21u32).to_be_bytes());
        section[4] = 5;
        section[5..9].copy_from_slice(&3u32.to_be_bytes());
        section[9..11].copy_from_slice(&0u16.to_be_bytes());
        section[11..15].copy_from_slice(&10.0f32.to_be_bytes());
        section[15..17].copy_from_slice(&2i16.to_be_bytes());
        section[17..19].copy_from_slice(&1i16.to_be_bytes());
        section[19] = 8;
        section[20] = 0;

        assert_eq!(
            DataRepresentation::parse(&section).unwrap(),
            DataRepresentation::SimplePacking(ScaledPackingParams {
                encoded_values: 3,
                reference_value: 10.0,
                binary_scale: 2,
                decimal_scale: 1,
                bits_per_value: 8,
                original_field_type: 0,
            })
        );
    }

    #[test]
    fn parses_jpeg2000_packing_template() {
        let mut section = vec![0u8; 23];
        section[..4].copy_from_slice(&(23u32).to_be_bytes());
        section[4] = 5;
        section[5..9].copy_from_slice(&4u32.to_be_bytes());
        section[9..11].copy_from_slice(&40u16.to_be_bytes());
        section[11..15].copy_from_slice(&3.5f32.to_be_bytes());
        section[15..17].copy_from_slice(&encode_wmo_i16(-1).unwrap());
        section[17..19].copy_from_slice(&2i16.to_be_bytes());
        section[19] = 12;
        section[20] = 0;
        section[21] = 1;
        section[22] = 255;

        assert_eq!(
            DataRepresentation::parse(&section).unwrap(),
            DataRepresentation::Jpeg2000Packing(Jpeg2000PackingParams {
                packing: ScaledPackingParams {
                    encoded_values: 4,
                    reference_value: 3.5,
                    binary_scale: -1,
                    decimal_scale: 2,
                    bits_per_value: 12,
                    original_field_type: 0,
                },
                compression_type: 1,
                target_compression_ratio: 255,
            })
        );
    }

    #[test]
    fn parses_png_packing_template() {
        let mut section = vec![0u8; 21];
        section[..4].copy_from_slice(&(21u32).to_be_bytes());
        section[4] = 5;
        section[5..9].copy_from_slice(&6u32.to_be_bytes());
        section[9..11].copy_from_slice(&41u16.to_be_bytes());
        section[11..15].copy_from_slice(&1.25f32.to_be_bytes());
        section[15..17].copy_from_slice(&1i16.to_be_bytes());
        section[17..19].copy_from_slice(&encode_wmo_i16(-2).unwrap());
        section[19] = 16;
        section[20] = 1;

        assert_eq!(
            DataRepresentation::parse(&section).unwrap(),
            DataRepresentation::PngPacking(PngPackingParams {
                packing: ScaledPackingParams {
                    encoded_values: 6,
                    reference_value: 1.25,
                    binary_scale: 1,
                    decimal_scale: -2,
                    bits_per_value: 16,
                    original_field_type: 1,
                },
            })
        );
    }

    #[test]
    fn parses_ccsds_packing_template() {
        let section = ccsds_section(20, CcsdsFlags::DEFAULT.bits(), 32, 128);

        assert_eq!(
            DataRepresentation::parse(&section).unwrap(),
            DataRepresentation::CcsdsPacking(
                CcsdsPackingParams::new(
                    ScaledPackingParams {
                        encoded_values: 6,
                        reference_value: 1.25,
                        binary_scale: -1,
                        decimal_scale: 2,
                        bits_per_value: 20,
                        original_field_type: 0,
                    },
                    CcsdsFlags::DEFAULT,
                    CcsdsBlockSize::THIRTY_TWO,
                    128,
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn parses_unenforced_even_ccsds_block_size() {
        let flags = CcsdsFlags::DEFAULT | CcsdsFlags::NOT_ENFORCE;
        let section = ccsds_section(8, flags.bits(), 12, 1);
        let parsed = DataRepresentation::parse(&section).unwrap();
        let DataRepresentation::CcsdsPacking(params) = parsed else {
            panic!("expected CCSDS representation");
        };

        assert_eq!(params.flags(), flags);
        assert_eq!(params.block_size().samples(), 12);
        assert_eq!(params.reference_sample_interval(), 1);
    }

    #[test]
    fn rejects_reserved_ccsds_flag_bit() {
        let error = DataRepresentation::parse(&ccsds_section(8, 0x80, 32, 128)).unwrap_err();
        assert!(matches!(
            error,
            crate::Error::InvalidSection { section: 5, .. }
        ));
        assert!(error.to_string().contains("reserved CCSDS flag bits"));
    }

    #[test]
    fn rejects_nonstandard_ccsds_block_without_flag() {
        let error = DataRepresentation::parse(&ccsds_section(8, 0, 12, 128)).unwrap_err();
        assert!(error.to_string().contains("requires the NOT_ENFORCE flag"));
    }

    #[test]
    fn rejects_restricted_ccsds_width_above_four_bits() {
        let error =
            DataRepresentation::parse(&ccsds_section(5, CcsdsFlags::RESTRICTED.bits(), 32, 128))
                .unwrap_err();
        assert!(error.to_string().contains("requires at most 4 bits"));
    }

    #[test]
    fn rejects_ccsds_reference_interval_outside_codec_range() {
        for interval in [0, 4097] {
            let error = DataRepresentation::parse(&ccsds_section(8, 0, 32, interval)).unwrap_err();
            assert!(error
                .to_string()
                .contains("reference sample interval must be between 1 and 4096"));
        }
    }

    fn ccsds_section(bits_per_value: u8, flags: u8, block_size: u8, rsi: u16) -> Vec<u8> {
        let mut section = vec![0u8; 25];
        section[..4].copy_from_slice(&25u32.to_be_bytes());
        section[4] = 5;
        section[5..9].copy_from_slice(&6u32.to_be_bytes());
        section[9..11].copy_from_slice(&42u16.to_be_bytes());
        section[11..15].copy_from_slice(&1.25f32.to_be_bytes());
        section[15..17].copy_from_slice(&encode_wmo_i16(-1).unwrap());
        section[17..19].copy_from_slice(&2i16.to_be_bytes());
        section[19] = bits_per_value;
        section[20] = 0;
        section[21] = flags;
        section[22] = block_size;
        section[23..25].copy_from_slice(&rsi.to_be_bytes());
        section
    }
}
