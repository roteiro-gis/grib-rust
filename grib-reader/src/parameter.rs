//! WMO parameter tables (Code Table 4.2) for GRIB2.
//!
//! Maps (discipline, parameter_category, parameter_number) to human-readable names.

/// Look up a GRIB1 parameter short name.
pub fn grib1_parameter_name(table_version: u8, number: u8) -> &'static str {
    match (table_version, number) {
        (_, 1) => "PRES",
        (_, 2) => "PRMSL",
        (_, 7) => "HGT",
        (_, 11) => "TMP",
        (_, 17) => "DPT",
        (_, 33) => "UGRD",
        (_, 34) => "VGRD",
        (_, 39) => "VVEL",
        (_, 52) => "RH",
        (_, 54) => "PWAT",
        (_, 61) => "APCP",
        (_, 71) => "TCDC",
        _ => "unknown",
    }
}

/// Look up a GRIB1 parameter description.
pub fn grib1_parameter_description(table_version: u8, number: u8) -> &'static str {
    match (table_version, number) {
        (_, 1) => "Pressure",
        (_, 2) => "Pressure reduced to mean sea level",
        (_, 7) => "Geopotential height",
        (_, 11) => "Temperature",
        (_, 17) => "Dew point temperature",
        (_, 33) => "U-component of wind",
        (_, 34) => "V-component of wind",
        (_, 39) => "Vertical velocity",
        (_, 52) => "Relative humidity",
        (_, 54) => "Precipitable water",
        (_, 61) => "Total precipitation",
        (_, 71) => "Total cloud cover",
        _ => "Unknown parameter",
    }
}

/// Look up a parameter short name from WMO code tables.
pub fn parameter_name(discipline: u8, category: u8, number: u8) -> &'static str {
    match (discipline, category, number) {
        // Discipline 0: Meteorological products
        // Category 0: Temperature
        (0, 0, 0) => "TMP",  // Temperature
        (0, 0, 1) => "VTMP", // Virtual temperature
        (0, 0, 2) => "POT",  // Potential temperature
        (0, 0, 4) => "TMAX", // Maximum temperature
        (0, 0, 5) => "TMIN", // Minimum temperature
        (0, 0, 6) => "DPT",  // Dew point temperature
        // Category 1: Moisture
        (0, 1, 0) => "SPFH", // Specific humidity
        (0, 1, 1) => "RH",   // Relative humidity
        (0, 1, 3) => "PWAT", // Precipitable water
        (0, 1, 8) => "APCP", // Total precipitation
        // Category 2: Momentum
        (0, 2, 0) => "WDIR",  // Wind direction
        (0, 2, 1) => "WIND",  // Wind speed
        (0, 2, 2) => "UGRD",  // U-component of wind
        (0, 2, 3) => "VGRD",  // V-component of wind
        (0, 2, 22) => "GUST", // Wind gust
        // Category 3: Mass
        (0, 3, 0) => "PRES",  // Pressure
        (0, 3, 1) => "PRMSL", // Pressure reduced to MSL
        (0, 3, 5) => "HGT",   // Geopotential height
        // Category 4: Short-wave radiation
        (0, 4, 7) => "DSWRF", // Downward short-wave radiation flux
        // Category 5: Long-wave radiation
        (0, 5, 3) => "DLWRF", // Downward long-wave radiation flux
        // Category 6: Cloud
        (0, 6, 1) => "TCDC", // Total cloud cover
        // Category 7: Thermodynamic stability
        (0, 7, 6) => "CAPE", // Convective available potential energy
        (0, 7, 7) => "CIN",  // Convective inhibition

        // Discipline 10: Oceanographic products
        // Category 0: Waves
        (10, 0, 3) => "HTSGW", // Significant wave height
        (10, 0, 4) => "WVDIR", // Wave direction
        (10, 0, 5) => "WVPER", // Wave period
        // Category 3: Surface properties
        (10, 3, 0) => "WTMP", // Water temperature

        _ => "unknown",
    }
}

/// Look up a human-readable parameter description.
pub fn parameter_description(discipline: u8, category: u8, number: u8) -> &'static str {
    match (discipline, category, number) {
        (0, 0, 0) => "Temperature",
        (0, 0, 1) => "Virtual temperature",
        (0, 0, 2) => "Potential temperature",
        (0, 0, 4) => "Maximum temperature",
        (0, 0, 5) => "Minimum temperature",
        (0, 0, 6) => "Dew point temperature",
        (0, 1, 0) => "Specific humidity",
        (0, 1, 1) => "Relative humidity",
        (0, 1, 3) => "Precipitable water",
        (0, 1, 8) => "Total precipitation",
        (0, 2, 0) => "Wind direction",
        (0, 2, 1) => "Wind speed",
        (0, 2, 2) => "U-component of wind",
        (0, 2, 3) => "V-component of wind",
        (0, 2, 22) => "Wind gust",
        (0, 3, 0) => "Pressure",
        (0, 3, 1) => "Pressure reduced to MSL",
        (0, 3, 5) => "Geopotential height",
        (0, 4, 7) => "Downward short-wave radiation flux",
        (0, 5, 3) => "Downward long-wave radiation flux",
        (0, 6, 1) => "Total cloud cover",
        (0, 7, 6) => "Convective available potential energy",
        (0, 7, 7) => "Convective inhibition",
        (10, 0, 3) => "Significant height of combined wind waves and swell",
        (10, 0, 4) => "Direction of wind waves",
        (10, 0, 5) => "Mean period of wind waves",
        (10, 3, 0) => "Water temperature",
        _ => "Unknown parameter",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_parameters() {
        assert_eq!(grib1_parameter_name(2, 11), "TMP");
        assert_eq!(grib1_parameter_name(2, 33), "UGRD");
        assert_eq!(parameter_name(0, 0, 0), "TMP");
        assert_eq!(parameter_name(0, 2, 2), "UGRD");
        assert_eq!(parameter_name(0, 3, 5), "HGT");
        assert_eq!(parameter_name(10, 0, 3), "HTSGW");
        assert_eq!(parameter_description(0, 0, 2), "Potential temperature");
        assert_eq!(parameter_description(10, 3, 0), "Water temperature");
    }

    #[test]
    fn unknown_parameter() {
        assert_eq!(parameter_name(255, 255, 255), "unknown");
    }
}
