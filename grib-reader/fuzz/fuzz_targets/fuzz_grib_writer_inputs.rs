#![no_main]

use grib_core::metadata::ReferenceTime;
use grib_core::{
    AnalysisOrForecastTemplate, FixedSurface, GridDefinition, Identification, LatLonGrid,
    ProductDefinition, ProductDefinitionTemplate,
};
use grib_reader::GribFile;
use grib_writer::{
    Grib1FieldBuilder, Grib1ProductDefinition, Grib2Field, Grib2FieldBuilder, GribWriter,
    PackingStrategy,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut input = Input::new(data);
    let result = if input.bool() {
        write_generated_grib2(&mut input)
    } else {
        write_generated_grib1(&mut input)
    };

    if let Ok(bytes) = result {
        assert_readable_grib(bytes);
    }
});

fn write_generated_grib2(input: &mut Input<'_>) -> grib_writer::Result<Vec<u8>> {
    let grid = generated_grid(input);
    let values = generated_values(input, &grid);
    let first_identification = identification(input, false);
    let first = generated_grib2_field(input, first_identification, grid.clone(), values)?;
    let mut fields = vec![first];

    if input.bool() {
        let vary_identification = input.bool();
        let second_identification = identification(input, vary_identification);
        let second_grid = if input.bool() {
            grid
        } else {
            generated_grid(input)
        };
        let second_values = generated_values(input, &second_grid);
        fields.push(generated_grib2_field(
            input,
            second_identification,
            second_grid,
            second_values,
        )?);
    }

    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes).write_grib2_message(fields)?;
    Ok(bytes)
}

fn write_generated_grib1(input: &mut Input<'_>) -> grib_writer::Result<Vec<u8>> {
    let grid = generated_grid(input);
    let values = generated_values(input, &grid);
    let mut builder = Grib1FieldBuilder::new()
        .product(grib1_product(input))
        .grid(grid)
        .packing(PackingStrategy::SimpleAuto {
            decimal_scale: decimal_scale(input),
        })
        .values(&values);

    if input.bool() {
        let bitmap = generated_bitmap(input, values.len());
        builder = builder.bitmap(&bitmap);
    }

    let field = builder.build()?;
    let mut bytes = Vec::new();
    GribWriter::new(&mut bytes).write_grib1_message(field)?;
    Ok(bytes)
}

fn generated_grib2_field(
    input: &mut Input<'_>,
    identification: Identification,
    grid: GridDefinition,
    values: Vec<f64>,
) -> grib_writer::Result<Grib2Field> {
    let mut builder = Grib2FieldBuilder::new()
        .discipline(input.u8() % 3)
        .identification(identification)
        .grid(grid)
        .product(product(input))
        .packing(PackingStrategy::SimpleAuto {
            decimal_scale: decimal_scale(input),
        })
        .values(&values);

    if input.bool() {
        let bitmap = generated_bitmap(input, values.len());
        builder = builder.bitmap(&bitmap);
    }

    builder.build()
}

fn assert_readable_grib(bytes: Vec<u8>) {
    let file = GribFile::from_bytes(bytes)
        .unwrap_or_else(|err| panic!("writer emitted unreadable GRIB: {err:?}"));
    file.read_all_data_as_f64()
        .unwrap_or_else(|err| panic!("writer emitted undecodable GRIB: {err:?}"));
}

fn generated_grid(input: &mut Input<'_>) -> GridDefinition {
    let ni = u32::from(input.u8() % 16) + 1;
    let nj = u32::from(input.u8() % 16) + 1;
    let scanning_mode = match input.u8() % 8 {
        0 => 0b0000_0000,
        1 => 0b1000_0000,
        2 => 0b0100_0000,
        3 => 0b1100_0000,
        4 => 0b0001_0000,
        5 => 0b1001_0000,
        6 => 0b0010_0000,
        _ => input.u8(),
    };

    let lon_first = (i32::from(input.u8() % 120) - 60) * 1_000_000;
    let lat_first = (i32::from(input.u8() % 90) - 45) * 1_000_000;
    let di = (u32::from(input.u8() % 20) + 1) * 100_000;
    let dj = (u32::from(input.u8() % 20) + 1) * 100_000;
    let i_step = if scanning_mode & 0b1000_0000 == 0 {
        di as i32
    } else {
        -(di as i32)
    };
    let j_step = if scanning_mode & 0b0100_0000 != 0 {
        dj as i32
    } else {
        -(dj as i32)
    };

    GridDefinition::LatLon(LatLonGrid {
        ni,
        nj,
        lat_first,
        lon_first,
        lat_last: lat_first + (nj.saturating_sub(1) as i32) * j_step,
        lon_last: lon_first + (ni.saturating_sub(1) as i32) * i_step,
        di,
        dj,
        scanning_mode,
    })
}

fn generated_values(input: &mut Input<'_>, grid: &GridDefinition) -> Vec<f64> {
    let expected = grid.num_points();
    let len = match input.u8() % 8 {
        0 => expected.saturating_sub(1),
        1 => expected.saturating_add(1),
        _ => expected,
    };

    (0..len)
        .map(|index| match input.u8() % 32 {
            0 => f64::NAN,
            1 => f64::INFINITY,
            2 => f64::NEG_INFINITY,
            _ => f64::from(input.i16()) / 10.0 + index as f64 * 0.01,
        })
        .collect()
}

fn generated_bitmap(input: &mut Input<'_>, value_count: usize) -> Vec<bool> {
    let len = match input.u8() % 6 {
        0 => value_count.saturating_sub(1),
        1 => value_count.saturating_add(1),
        _ => value_count,
    };
    (0..len).map(|_| input.bool()).collect()
}

fn identification(input: &mut Input<'_>, vary: bool) -> Identification {
    Identification {
        center_id: if vary { u16::from(input.u8()) } else { 7 },
        subcenter_id: 0,
        master_table_version: 35,
        local_table_version: 1,
        significance_of_reference_time: 1,
        reference_year: 2026,
        reference_month: 3,
        reference_day: 20,
        reference_hour: 12,
        reference_minute: 0,
        reference_second: 0,
        production_status: 0,
        processed_data_type: 1,
    }
}

fn product(input: &mut Input<'_>) -> ProductDefinition {
    let (parameter_category, parameter_number) = match input.u8() % 4 {
        0 => (0, 0),
        1 => (0, 2),
        2 => (0, 4),
        _ => (input.u8(), input.u8()),
    };
    ProductDefinition {
        parameter_category,
        parameter_number,
        template: ProductDefinitionTemplate::AnalysisOrForecast(AnalysisOrForecastTemplate {
            generating_process: 2,
            forecast_time_unit: 1,
            forecast_time: u32::from(input.u8()),
            first_surface: Some(FixedSurface {
                surface_type: 103,
                scale_factor: 0,
                scaled_value: 850,
            }),
            second_surface: None,
        }),
    }
}

fn grib1_product(input: &mut Input<'_>) -> Grib1ProductDefinition {
    Grib1ProductDefinition {
        table_version: 2,
        center_id: 7,
        generating_process_id: 255,
        grid_id: 0,
        has_grid_definition: true,
        has_bitmap: false,
        parameter_number: 11,
        level_type: 100,
        level_value: 850,
        reference_time: ReferenceTime {
            year: 2026,
            month: 3,
            day: 20,
            hour: 12,
            minute: 0,
            second: 0,
        },
        forecast_time_unit: 1,
        p1: input.u8(),
        p2: 0,
        time_range_indicator: 0,
        average_count: 0,
        missing_count: 0,
        century: 21,
        subcenter_id: 0,
        decimal_scale: 0,
    }
}

fn decimal_scale(input: &mut Input<'_>) -> i16 {
    i16::from(input.u8() % 11) - 5
}

struct Input<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Input<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn bool(&mut self) -> bool {
        self.u8() & 1 != 0
    }

    fn i16(&mut self) -> i16 {
        i16::from_be_bytes([self.u8(), self.u8()])
    }

    fn u8(&mut self) -> u8 {
        let value = self.data.get(self.position).copied().unwrap_or(0);
        self.position = self.position.saturating_add(1);
        value
    }
}
