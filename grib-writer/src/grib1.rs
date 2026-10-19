use grib_core::binary::{
    encode_ibm_f32, encode_wmo_i16, encode_wmo_i24, write_u16_be, write_u24_be, write_u8_be,
    U24_MAX,
};
use grib_core::grib1::ProductDefinition as Grib1ProductDefinition;
use grib_core::{DataRepresentation, GridDefinition};

use crate::packing::PackedField;
use crate::{Error, Result};

pub(crate) fn write_grib1_product_section(
    out: &mut Vec<u8>,
    product: &Grib1ProductDefinition,
) -> Result<()> {
    let (year_of_century, century) = grib1_reference_year_fields(product.reference_time.year)?;

    write_u24_be(out, 28)?;
    write_u8_be(out, product.table_version)?;
    write_u8_be(out, product.center_id)?;
    write_u8_be(out, product.generating_process_id)?;
    write_u8_be(out, product.grid_id)?;
    let mut flags = 0b1000_0000;
    if product.has_bitmap {
        flags |= 0b0100_0000;
    }
    write_u8_be(out, flags)?;
    write_u8_be(out, product.parameter_number)?;
    write_u8_be(out, product.level_type)?;
    write_u16_be(out, product.level_value)?;
    write_u8_be(out, year_of_century)?;
    write_u8_be(out, product.reference_time.month)?;
    write_u8_be(out, product.reference_time.day)?;
    write_u8_be(out, product.reference_time.hour)?;
    write_u8_be(out, product.reference_time.minute)?;
    write_u8_be(out, product.forecast_time_unit)?;
    write_u8_be(out, product.p1)?;
    write_u8_be(out, product.p2)?;
    write_u8_be(out, product.time_range_indicator)?;
    write_u16_be(out, product.average_count)?;
    write_u8_be(out, product.missing_count)?;
    write_u8_be(out, century)?;
    write_u8_be(out, product.subcenter_id)?;
    out.extend_from_slice(
        &encode_wmo_i16(product.decimal_scale)
            .ok_or_else(|| Error::Other("decimal scale does not fit GRIB signed i16".into()))?,
    );
    Ok(())
}

pub(crate) fn write_grib1_grid_section(out: &mut Vec<u8>, grid: &GridDefinition) -> Result<()> {
    let Some(grid) = grid.as_lat_lon() else {
        return Err(Error::UnsupportedGridTemplate(grid.template_number()));
    };

    write_u24_be(out, 32)?;
    write_u8_be(out, 0)?;
    write_u8_be(out, 255)?;
    write_u8_be(out, 0)?;
    write_u16_be(out, checked_grib1_grid_dimension(grid.ni, "Ni")?)?;
    write_u16_be(out, checked_grib1_grid_dimension(grid.nj, "Nj")?)?;
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lat_first,
        "latitude of first grid point",
    )?);
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lon_first,
        "longitude of first grid point",
    )?);
    write_u8_be(out, 0x80)?;
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lat_last,
        "latitude of last grid point",
    )?);
    out.extend_from_slice(&encode_grib1_coordinate(
        grid.lon_last,
        "longitude of last grid point",
    )?);
    write_u16_be(
        out,
        checked_grib1_increment(grid.di, "i direction increment")?,
    )?;
    write_u16_be(
        out,
        checked_grib1_increment(grid.dj, "j direction increment")?,
    )?;
    write_u8_be(out, grid.scanning_mode)?;
    out.extend_from_slice(&[0; 4]);
    Ok(())
}

pub(crate) fn write_grib1_bitmap_section(
    out: &mut Vec<u8>,
    bitmap_payload: &[u8],
    num_points: usize,
) -> Result<()> {
    let length = checked_grib1_u24_length(6usize + bitmap_payload.len(), 3)?;
    write_u24_be(out, length)?;
    write_u8_be(out, unused_bits_for_width(num_points, 1)?)?;
    write_u16_be(out, 0)?;
    out.extend_from_slice(bitmap_payload);
    Ok(())
}

pub(crate) fn write_grib1_predefined_bitmap_section(
    out: &mut Vec<u8>,
    table_reference: u16,
) -> Result<()> {
    if table_reference == 0 {
        return Err(Error::Other(
            "GRIB1 predefined bitmap table reference must be nonzero".into(),
        ));
    }
    write_u24_be(out, 6)?;
    write_u8_be(out, 0)?;
    write_u16_be(out, table_reference)?;
    Ok(())
}

pub(crate) fn write_grib1_data_section(
    out: &mut Vec<u8>,
    packed: &PackedField,
    flags: u8,
) -> Result<()> {
    validate_grib1_binary_data_flags(flags)?;
    let DataRepresentation::SimplePacking(params) = &packed.representation else {
        return Err(Error::UnsupportedDataTemplate(1004));
    };

    let length = checked_grib1_u24_length(11usize + packed.data_payload.len(), 4)?;
    write_u24_be(out, length)?;
    let unused_bits = unused_bits_for_width(params.encoded_values, params.bits_per_value)?;
    write_u8_be(out, (flags << 4) | unused_bits)?;
    out.extend_from_slice(
        &encode_wmo_i16(params.binary_scale)
            .ok_or_else(|| Error::Other("binary scale does not fit GRIB signed i16".into()))?,
    );
    out.extend_from_slice(
        &encode_ibm_f32(params.reference_value)
            .ok_or_else(|| Error::Other("reference value does not fit GRIB1 IBM float".into()))?,
    );
    write_u8_be(out, params.bits_per_value)?;
    out.extend_from_slice(&packed.data_payload);
    Ok(())
}

pub(crate) fn validate_grib1_binary_data_flags(flags: u8) -> Result<()> {
    if flags == 0 {
        return Ok(());
    }
    if flags > 0x0f {
        return Err(Error::Other(
            "GRIB1 binary data flags must fit in four bits".into(),
        ));
    }
    let template = if flags & 0b1000 != 0 {
        1004
    } else if flags & 0b0100 != 0 {
        1005
    } else if flags & 0b0010 != 0 {
        1006
    } else {
        1007
    };
    Err(Error::UnsupportedDataTemplate(template))
}

fn unused_bits_for_width(values: usize, bits_per_value: u8) -> Result<u8> {
    let bits = values
        .checked_mul(usize::from(bits_per_value))
        .ok_or_else(|| Error::Other("packed bit count overflow".into()))?;
    Ok(((8 - (bits % 8)) % 8) as u8)
}

fn grib1_reference_year_fields(year: u16) -> Result<(u8, u8)> {
    if year == 0 {
        return Err(Error::Other(
            "GRIB1 reference year 0 cannot be encoded".into(),
        ));
    }

    let century = ((year - 1) / 100) + 1;
    let year_of_century = year - ((century - 1) * 100);
    Ok((
        u8::try_from(year_of_century)
            .map_err(|_| Error::Other("GRIB1 year of century exceeds u8".into()))?,
        u8::try_from(century).map_err(|_| Error::Other("GRIB1 century exceeds u8".into()))?,
    ))
}

fn encode_grib1_coordinate(value: i32, name: &str) -> Result<[u8; 3]> {
    if value.rem_euclid(1_000) != 0 {
        return Err(Error::Other(format!(
            "{name} must be representable in GRIB1 millidegrees"
        )));
    }
    encode_wmo_i24(value / 1_000)
        .ok_or_else(|| Error::Other(format!("{name} does not fit GRIB signed i24")))
}

fn checked_grib1_grid_dimension(value: u32, name: &str) -> Result<u16> {
    u16::try_from(value).map_err(|_| Error::Other(format!("{name} exceeds GRIB1 u16 limit")))
}

fn checked_grib1_increment(value: u32, name: &str) -> Result<u16> {
    if !value.is_multiple_of(1_000) {
        return Err(Error::Other(format!(
            "{name} must be representable in GRIB1 millidegrees"
        )));
    }
    u16::try_from(value / 1_000)
        .map_err(|_| Error::Other(format!("{name} exceeds GRIB1 u16 millidegree limit")))
}

pub(crate) fn checked_grib1_u24_length(length: usize, section: u8) -> Result<u32> {
    let length = u32::try_from(length).map_err(|_| Error::InvalidSection {
        section,
        reason: "GRIB1 length exceeds unsigned 24-bit limit".into(),
    })?;
    if length > U24_MAX {
        return Err(Error::InvalidSection {
            section,
            reason: format!("GRIB1 length {length} exceeds unsigned 24-bit limit"),
        });
    }
    Ok(length)
}

pub(crate) fn validate_supported_grib1_grid(grid: &GridDefinition) -> Result<()> {
    let Some(grid) = grid.as_lat_lon() else {
        return Err(Error::UnsupportedGridTemplate(grid.template_number()));
    };
    grid.validate_supported_scan_order()?;
    checked_grib1_grid_dimension(grid.ni, "Ni")?;
    checked_grib1_grid_dimension(grid.nj, "Nj")?;
    checked_grib1_increment(grid.di, "i direction increment")?;
    checked_grib1_increment(grid.dj, "j direction increment")?;
    encode_grib1_coordinate(grid.lat_first, "latitude of first grid point")?;
    encode_grib1_coordinate(grid.lon_first, "longitude of first grid point")?;
    encode_grib1_coordinate(grid.lat_last, "latitude of last grid point")?;
    encode_grib1_coordinate(grid.lon_last, "longitude of last grid point")?;
    Ok(())
}
