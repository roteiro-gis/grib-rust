//! WMO parameter tables (Code Table 4.2) and local table overlays for GRIB2.
//!
//! Maps (discipline, parameter_category, parameter_number) to human-readable names.

use crate::metadata::{Parameter, ParameterTableSource};

/// A user-authored or built-in GRIB2 local table entry.
///
/// `subcenter_id` and `local_table_version` may be `None` to match any value
/// for a center, but correctness-sensitive built-in entries should prefer exact
/// local table versions when known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalParameterEntry<'a> {
    pub center_id: u16,
    pub subcenter_id: Option<u16>,
    pub local_table_version: Option<u8>,
    pub discipline: u8,
    pub category: u8,
    pub number: u8,
    pub short_name: &'a str,
    pub description: &'a str,
}

impl LocalParameterEntry<'_> {
    fn matches(
        &self,
        discipline: u8,
        category: u8,
        number: u8,
        center_id: u16,
        subcenter_id: u16,
        local_table_version: u8,
    ) -> bool {
        self.center_id == center_id
            && self
                .subcenter_id
                .map_or(true, |expected| expected == subcenter_id)
            && self
                .local_table_version
                .map_or(true, |expected| expected == local_table_version)
            && self.discipline == discipline
            && self.category == category
            && self.number == number
    }
}

/// Built-in local table entries shipped with the crate.
pub const BUILTIN_LOCAL_PARAMETERS: &[LocalParameterEntry<'static>] = &[LocalParameterEntry {
    center_id: 7,
    subcenter_id: None,
    local_table_version: Some(1),
    discipline: 0,
    category: 16,
    number: 196,
    short_name: "REFC",
    description: "Composite reflectivity",
}];

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
    wmo_parameter_definition(discipline, category, number)
        .map(|(short_name, _description)| short_name)
        .unwrap_or("unknown")
}

/// Look up a human-readable parameter description from WMO code tables.
pub fn parameter_description(discipline: u8, category: u8, number: u8) -> &'static str {
    wmo_parameter_definition(discipline, category, number)
        .map(|(_short_name, description)| description)
        .unwrap_or("Unknown parameter")
}

/// Look up a GRIB2 parameter using WMO tables plus built-in local tables.
pub fn lookup_parameter(
    discipline: u8,
    category: u8,
    number: u8,
    center_id: u16,
    subcenter_id: u16,
    local_table_version: u8,
) -> Parameter {
    lookup_parameter_with_local_entries(
        discipline,
        category,
        number,
        center_id,
        subcenter_id,
        local_table_version,
        &[],
    )
}

/// Look up a GRIB2 parameter using WMO tables plus user-provided local entries.
///
/// The WMO table is always checked first for standard parameters. User-provided
/// local entries are checked before built-in local entries, allowing callers to
/// add or override center-specific local-use definitions without changing the
/// global WMO table.
pub fn lookup_parameter_with_local_entries(
    discipline: u8,
    category: u8,
    number: u8,
    center_id: u16,
    subcenter_id: u16,
    local_table_version: u8,
    local_entries: &[LocalParameterEntry<'_>],
) -> Parameter {
    if let Some((short_name, description)) = wmo_parameter_definition(discipline, category, number)
    {
        return Parameter::new_grib2_with_source(
            discipline,
            category,
            number,
            short_name,
            description,
            ParameterTableSource::Wmo,
        );
    }

    if is_local_parameter_code(category) || is_local_parameter_code(number) {
        if local_table_version != 0 {
            if let Some(entry) = local_entries
                .iter()
                .chain(BUILTIN_LOCAL_PARAMETERS.iter())
                .find(|entry| {
                    entry.matches(
                        discipline,
                        category,
                        number,
                        center_id,
                        subcenter_id,
                        local_table_version,
                    )
                })
            {
                return Parameter::new_grib2_with_source(
                    discipline,
                    category,
                    number,
                    entry.short_name.to_owned(),
                    entry.description.to_owned(),
                    ParameterTableSource::Local {
                        center_id,
                        subcenter_id,
                        local_table_version,
                    },
                );
            }
        }

        return Parameter::new_grib2_with_source(
            discipline,
            category,
            number,
            "unknown",
            "Unknown parameter",
            ParameterTableSource::UnknownLocal {
                center_id,
                subcenter_id,
                local_table_version,
            },
        );
    }

    Parameter::new_grib2_with_source(
        discipline,
        category,
        number,
        "unknown",
        "Unknown parameter",
        ParameterTableSource::Unknown,
    )
}

fn is_local_parameter_code(code: u8) -> bool {
    (192..=254).contains(&code)
}

fn wmo_parameter_definition(
    discipline: u8,
    category: u8,
    number: u8,
) -> Option<(&'static str, &'static str)> {
    match (discipline, category, number) {
        // Discipline 0: Meteorological products
        // Category 0: Temperature
        (0, 0, 0) => Some(("TMP", "Temperature")),
        (0, 0, 1) => Some(("VTMP", "Virtual temperature")),
        (0, 0, 2) => Some(("POT", "Potential temperature")),
        (0, 0, 4) => Some(("TMAX", "Maximum temperature")),
        (0, 0, 5) => Some(("TMIN", "Minimum temperature")),
        (0, 0, 6) => Some(("DPT", "Dew point temperature")),
        // Category 1: Moisture
        (0, 1, 0) => Some(("SPFH", "Specific humidity")),
        (0, 1, 1) => Some(("RH", "Relative humidity")),
        (0, 1, 3) => Some(("PWAT", "Precipitable water")),
        (0, 1, 8) => Some(("APCP", "Total precipitation")),
        // Category 2: Momentum
        (0, 2, 0) => Some(("WDIR", "Wind direction")),
        (0, 2, 1) => Some(("WIND", "Wind speed")),
        (0, 2, 2) => Some(("UGRD", "U-component of wind")),
        (0, 2, 3) => Some(("VGRD", "V-component of wind")),
        (0, 2, 22) => Some(("GUST", "Wind gust")),
        // Category 3: Mass
        (0, 3, 0) => Some(("PRES", "Pressure")),
        (0, 3, 1) => Some(("PRMSL", "Pressure reduced to MSL")),
        (0, 3, 5) => Some(("HGT", "Geopotential height")),
        // Category 4: Short-wave radiation
        (0, 4, 7) => Some(("DSWRF", "Downward short-wave radiation flux")),
        // Category 5: Long-wave radiation
        (0, 5, 3) => Some(("DLWRF", "Downward long-wave radiation flux")),
        // Category 6: Cloud
        (0, 6, 1) => Some(("TCDC", "Total cloud cover")),
        // Category 7: Thermodynamic stability
        (0, 7, 6) => Some(("CAPE", "Convective available potential energy")),
        (0, 7, 7) => Some(("CIN", "Convective inhibition")),
        // Category 16: Forecast radar imagery
        (0, 16, 5) => Some(("REFC", "Composite reflectivity")),
        // Category 19: Physical atmospheric properties
        (0, 19, 1) => Some(("ALBDO", "Albedo")),

        // Discipline 10: Oceanographic products
        // Category 0: Waves
        (10, 0, 3) => Some((
            "HTSGW",
            "Significant height of combined wind waves and swell",
        )),
        (10, 0, 4) => Some(("WVDIR", "Direction of wind waves")),
        (10, 0, 5) => Some(("WVPER", "Mean period of wind waves")),
        // Category 3: Surface properties
        (10, 3, 0) => Some(("WTMP", "Water temperature")),

        _ => None,
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
        assert_eq!(parameter_name(0, 16, 5), "REFC");
        assert_eq!(parameter_name(0, 19, 1), "ALBDO");
        assert_eq!(parameter_name(10, 0, 3), "HTSGW");
        assert_eq!(parameter_description(0, 0, 2), "Potential temperature");
        assert_eq!(parameter_description(0, 16, 5), "Composite reflectivity");
        assert_eq!(parameter_description(0, 19, 1), "Albedo");
        assert_eq!(parameter_description(10, 3, 0), "Water temperature");
    }

    #[test]
    fn unknown_parameter() {
        assert_eq!(parameter_name(255, 255, 255), "unknown");
        assert_eq!(parameter_name(0, 16, 196), "unknown");
        assert_eq!(parameter_description(0, 16, 196), "Unknown parameter");
    }

    #[test]
    fn ncep_refc_uses_local_table_context() {
        let parameter = lookup_parameter(0, 16, 196, 7, 0, 1);

        assert_eq!(parameter.short_name, "REFC");
        assert_eq!(parameter.description, "Composite reflectivity");
        assert_eq!(
            parameter.source,
            ParameterTableSource::Local {
                center_id: 7,
                subcenter_id: 0,
                local_table_version: 1,
            }
        );
    }

    #[test]
    fn unknown_local_parameter_remains_local_unknown() {
        let parameter = lookup_parameter(0, 16, 196, 8, 0, 1);

        assert_eq!(parameter.short_name, "unknown");
        assert_eq!(parameter.description, "Unknown parameter");
        assert_eq!(
            parameter.source,
            ParameterTableSource::UnknownLocal {
                center_id: 8,
                subcenter_id: 0,
                local_table_version: 1,
            }
        );
    }

    #[test]
    fn local_table_version_must_match_local_entry() {
        let parameter = lookup_parameter(0, 16, 196, 7, 0, 2);

        assert_eq!(parameter.short_name, "unknown");
        assert_eq!(parameter.description, "Unknown parameter");
        assert_eq!(
            parameter.source,
            ParameterTableSource::UnknownLocal {
                center_id: 7,
                subcenter_id: 0,
                local_table_version: 2,
            }
        );
    }

    #[test]
    fn user_local_entries_resolve_local_use_codes() {
        let entries = [LocalParameterEntry {
            center_id: 42,
            subcenter_id: Some(5),
            local_table_version: Some(3),
            discipline: 0,
            category: 192,
            number: 1,
            short_name: "XFOO",
            description: "Example local parameter",
        }];

        let parameter = lookup_parameter_with_local_entries(0, 192, 1, 42, 5, 3, &entries);

        assert_eq!(parameter.short_name, "XFOO");
        assert_eq!(parameter.description, "Example local parameter");
        assert_eq!(
            parameter.source,
            ParameterTableSource::Local {
                center_id: 42,
                subcenter_id: 5,
                local_table_version: 3,
            }
        );
    }

    #[test]
    fn wmo_parameters_win_over_local_entries() {
        let entries = [LocalParameterEntry {
            center_id: 7,
            subcenter_id: None,
            local_table_version: Some(1),
            discipline: 0,
            category: 0,
            number: 0,
            short_name: "BADTMP",
            description: "Bad local temperature",
        }];

        let parameter = lookup_parameter_with_local_entries(0, 0, 0, 7, 0, 1, &entries);

        assert_eq!(parameter.short_name, "TMP");
        assert_eq!(parameter.description, "Temperature");
        assert_eq!(parameter.source, ParameterTableSource::Wmo);
    }
}
