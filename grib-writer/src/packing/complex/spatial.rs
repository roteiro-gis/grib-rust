use grib_core::bit::BitWriter;
use grib_core::SpatialDifferencingParams;

use super::super::SpatialDifferencingOrder;
use crate::{Error, Result};

impl SpatialDifferencingOrder {
    const fn grib_order(self) -> u8 {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }

    const fn min_values(self) -> usize {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct SpatialPacking {
    pub(super) params: SpatialDifferencingParams,
    descriptors: SpatialDescriptors,
    pub(super) values: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
struct SpatialDescriptors {
    first_value: i64,
    second_value: Option<i64>,
    overall_minimum: i64,
}

pub(super) fn spatially_difference_values(
    values: &[u64],
    order: SpatialDifferencingOrder,
) -> Result<SpatialPacking> {
    if values.len() < order.min_values() {
        return Err(Error::DataLengthMismatch {
            expected: order.min_values(),
            actual: values.len(),
        });
    }

    let values = values
        .iter()
        .copied()
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| Error::Other("spatial differencing value exceeds i64".into()))
        })
        .collect::<Result<Vec<_>>>()?;

    let (descriptors, differenced) = match order {
        SpatialDifferencingOrder::First => first_order_spatial_difference(&values)?,
        SpatialDifferencingOrder::Second => second_order_spatial_difference(&values)?,
    };
    let descriptor_octets = spatial_descriptor_octets(&descriptors)?;

    Ok(SpatialPacking {
        params: SpatialDifferencingParams {
            order: order.grib_order(),
            descriptor_octets,
        },
        descriptors,
        values: differenced,
    })
}

fn first_order_spatial_difference(values: &[i64]) -> Result<(SpatialDescriptors, Vec<u64>)> {
    let mut differences = Vec::with_capacity(values.len().saturating_sub(1));
    for pair in values.windows(2) {
        differences.push(
            pair[1]
                .checked_sub(pair[0])
                .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?,
        );
    }
    let overall_minimum = differences.iter().copied().min().unwrap_or(0);

    let mut differenced = Vec::with_capacity(values.len());
    differenced.push(0);
    for difference in differences {
        differenced.push(spatial_difference_delta(difference, overall_minimum)?);
    }

    Ok((
        SpatialDescriptors {
            first_value: values[0],
            second_value: None,
            overall_minimum,
        },
        differenced,
    ))
}

fn second_order_spatial_difference(values: &[i64]) -> Result<(SpatialDescriptors, Vec<u64>)> {
    let first_value = values[0];
    let second_value = values[1];
    let mut previous_difference = second_value
        .checked_sub(first_value)
        .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
    let mut second_differences = Vec::with_capacity(values.len().saturating_sub(2));

    for index in 2..values.len() {
        let difference = values[index]
            .checked_sub(values[index - 1])
            .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
        second_differences.push(
            difference
                .checked_sub(previous_difference)
                .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?,
        );
        previous_difference = difference;
    }

    let overall_minimum = second_differences.iter().copied().min().unwrap_or(0);
    let mut differenced = Vec::with_capacity(values.len());
    differenced.push(0);
    differenced.push(0);
    for second_difference in second_differences {
        differenced.push(spatial_difference_delta(
            second_difference,
            overall_minimum,
        )?);
    }

    Ok((
        SpatialDescriptors {
            first_value,
            second_value: Some(second_value),
            overall_minimum,
        },
        differenced,
    ))
}

fn spatial_difference_delta(value: i64, overall_minimum: i64) -> Result<u64> {
    let delta = value
        .checked_sub(overall_minimum)
        .ok_or_else(|| Error::Other("spatial differencing overflow".into()))?;
    u64::try_from(delta)
        .map_err(|_| Error::Other("spatial differencing produced negative delta".into()))
}

fn spatial_descriptor_octets(descriptors: &SpatialDescriptors) -> Result<u8> {
    let values = [
        Some(descriptors.first_value),
        descriptors.second_value,
        Some(descriptors.overall_minimum),
    ];
    for octets in 1..=8 {
        if values
            .iter()
            .flatten()
            .all(|value| signed_magnitude_fits(*value, octets))
        {
            return Ok(octets);
        }
    }

    Err(Error::Other(
        "spatial differencing descriptor exceeds signed-magnitude range".into(),
    ))
}

fn signed_magnitude_fits(value: i64, octets: u8) -> bool {
    signed_magnitude_bits(value, octets).is_ok()
}

pub(super) fn write_spatial_descriptors(
    writer: &mut BitWriter,
    spatial: &SpatialPacking,
) -> Result<()> {
    let bit_count = usize::from(spatial.params.descriptor_octets) * 8;
    writer.write(
        signed_magnitude_bits(
            spatial.descriptors.first_value,
            spatial.params.descriptor_octets,
        )?,
        bit_count,
    )?;
    if let Some(second_value) = spatial.descriptors.second_value {
        writer.write(
            signed_magnitude_bits(second_value, spatial.params.descriptor_octets)?,
            bit_count,
        )?;
    }
    writer.write(
        signed_magnitude_bits(
            spatial.descriptors.overall_minimum,
            spatial.params.descriptor_octets,
        )?,
        bit_count,
    )
}

fn signed_magnitude_bits(value: i64, octets: u8) -> Result<u64> {
    let bit_count = u32::from(octets) * 8;
    if bit_count == 0 || bit_count > u64::BITS {
        return Err(Error::Other(
            "spatial differencing descriptor width must be 1..=8 octets".into(),
        ));
    }
    let magnitude = value
        .checked_abs()
        .ok_or_else(|| Error::Other("spatial differencing descriptor magnitude overflow".into()))?
        as u64;
    let magnitude_bits = bit_count - 1;
    let max_magnitude = if magnitude_bits == u64::BITS {
        u64::MAX
    } else {
        (1u64 << magnitude_bits) - 1
    };
    if magnitude > max_magnitude {
        return Err(Error::Other(
            "spatial differencing descriptor exceeds signed-magnitude range".into(),
        ));
    }

    let sign_bit = if value < 0 {
        1u64 << (bit_count - 1)
    } else {
        0
    };
    Ok(sign_bit | magnitude)
}
