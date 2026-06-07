//! WMO parameter tables (Code Table 4.2) and local table overlays for GRIB2.
//!
//! Maps (discipline, parameter_category, parameter_number) to human-readable names.

use std::path::Path;

use crate::metadata::{Parameter, ParameterTableSource};
use crate::{Error, Result};

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

/// Header for the line-oriented local parameter table CSV format.
pub const LOCAL_PARAMETER_TABLE_CSV_HEADER: &str =
    "center_id,subcenter_id,local_table_version,discipline,category,number,short_name,description";

/// Owned local table entry for authoring or loading table files.
///
/// Convert a [`LocalParameterTable`] into borrowed [`LocalParameterEntry`]
/// overlays with [`LocalParameterTable::entries`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedLocalParameterEntry {
    pub center_id: u16,
    pub subcenter_id: Option<u16>,
    pub local_table_version: Option<u8>,
    pub discipline: u8,
    pub category: u8,
    pub number: u8,
    pub short_name: String,
    pub description: String,
}

impl OwnedLocalParameterEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        center_id: u16,
        subcenter_id: Option<u16>,
        local_table_version: Option<u8>,
        discipline: u8,
        category: u8,
        number: u8,
        short_name: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self> {
        let entry = Self {
            center_id,
            subcenter_id,
            local_table_version,
            discipline,
            category,
            number,
            short_name: short_name.into(),
            description: description.into(),
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn as_entry(&self) -> LocalParameterEntry<'_> {
        LocalParameterEntry {
            center_id: self.center_id,
            subcenter_id: self.subcenter_id,
            local_table_version: self.local_table_version,
            discipline: self.discipline,
            category: self.category,
            number: self.number,
            short_name: &self.short_name,
            description: &self.description,
        }
    }

    fn validate(&self) -> Result<()> {
        if !is_local_parameter_code(self.category) && !is_local_parameter_code(self.number) {
            return Err(Error::Other(format!(
                "local parameter entry {}.{}.{} must use a local category or parameter number (192..=254)",
                self.discipline, self.category, self.number
            )));
        }
        validate_local_parameter_text("short_name", &self.short_name)?;
        validate_local_parameter_text("description", &self.description)?;
        if self.short_name.contains(',') {
            return Err(Error::Other(
                "local parameter short_name cannot contain a comma".into(),
            ));
        }
        Ok(())
    }
}

/// Owned GRIB2 local parameter table with file-authoring helpers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalParameterTable {
    entries: Vec<OwnedLocalParameterEntry>,
}

impl LocalParameterTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_entries(
        entries: impl IntoIterator<Item = OwnedLocalParameterEntry>,
    ) -> Result<Self> {
        let mut table = Self::new();
        for entry in entries {
            table.push(entry)?;
        }
        Ok(table)
    }

    pub fn from_csv_str(input: &str) -> Result<Self> {
        let mut table = Self::new();
        for (line_index, raw_line) in input.lines().enumerate() {
            let line_number = line_index + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if is_local_parameter_csv_header(line) {
                continue;
            }
            table.push(parse_local_parameter_csv_record(line, line_number)?)?;
        }
        Ok(table)
    }

    pub fn from_csv_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let input = std::fs::read_to_string(path)
            .map_err(|err| Error::Io(err, path.display().to_string()))?;
        Self::from_csv_str(&input)
    }

    pub fn push(&mut self, entry: OwnedLocalParameterEntry) -> Result<()> {
        entry.validate()?;
        if self
            .entries
            .iter()
            .any(|existing| local_parameter_keys_overlap(existing, &entry))
        {
            return Err(Error::Other(format!(
                "duplicate local parameter entry for center {} discipline {} category {} number {}",
                entry.center_id, entry.discipline, entry.category, entry.number
            )));
        }
        self.entries.push(entry);
        Ok(())
    }

    pub fn authored_entries(&self) -> &[OwnedLocalParameterEntry] {
        &self.entries
    }

    pub fn entries(&self) -> Vec<LocalParameterEntry<'_>> {
        let mut entries = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (index, entry.as_entry()))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(index, entry)| {
            (
                std::cmp::Reverse(local_parameter_entry_specificity(*entry)),
                *index,
            )
        });
        entries.into_iter().map(|(_index, entry)| entry).collect()
    }

    pub fn to_csv_string(&self) -> String {
        let mut out = String::from(LOCAL_PARAMETER_TABLE_CSV_HEADER);
        out.push('\n');
        for entry in &self.entries {
            out.push_str(&entry.center_id.to_string());
            out.push(',');
            if let Some(subcenter_id) = entry.subcenter_id {
                out.push_str(&subcenter_id.to_string());
            }
            out.push(',');
            if let Some(version) = entry.local_table_version {
                out.push_str(&version.to_string());
            }
            out.push(',');
            out.push_str(&entry.discipline.to_string());
            out.push(',');
            out.push_str(&entry.category.to_string());
            out.push(',');
            out.push_str(&entry.number.to_string());
            out.push(',');
            out.push_str(&entry.short_name);
            out.push(',');
            out.push_str(&entry.description);
            out.push('\n');
        }
        out
    }
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

fn parse_local_parameter_csv_record(
    line: &str,
    line_number: usize,
) -> Result<OwnedLocalParameterEntry> {
    let fields = line.splitn(8, ',').map(str::trim).collect::<Vec<_>>();
    if fields.len() != 8 {
        return Err(Error::Other(format!(
            "local parameter CSV line {line_number} has {} fields, expected 8",
            fields.len()
        )));
    }

    OwnedLocalParameterEntry::new(
        parse_required_u16_field(fields[0], "center_id", line_number)?,
        parse_optional_u16_field(fields[1], "subcenter_id", line_number)?,
        parse_optional_u8_field(fields[2], "local_table_version", line_number)?,
        parse_required_u8_field(fields[3], "discipline", line_number)?,
        parse_required_u8_field(fields[4], "category", line_number)?,
        parse_required_u8_field(fields[5], "number", line_number)?,
        fields[6],
        fields[7],
    )
}

fn is_local_parameter_csv_header(line: &str) -> bool {
    line.split(',').map(str::trim).collect::<Vec<_>>().join(",") == LOCAL_PARAMETER_TABLE_CSV_HEADER
}

fn parse_required_u16_field(value: &str, name: &str, line_number: usize) -> Result<u16> {
    if value.is_empty() {
        return Err(Error::Other(format!(
            "local parameter CSV line {line_number} missing {name}"
        )));
    }
    value.parse::<u16>().map_err(|err| {
        Error::Other(format!(
            "local parameter CSV line {line_number} invalid {name}: {err}"
        ))
    })
}

fn parse_required_u8_field(value: &str, name: &str, line_number: usize) -> Result<u8> {
    if value.is_empty() {
        return Err(Error::Other(format!(
            "local parameter CSV line {line_number} missing {name}"
        )));
    }
    value.parse::<u8>().map_err(|err| {
        Error::Other(format!(
            "local parameter CSV line {line_number} invalid {name}: {err}"
        ))
    })
}

fn parse_optional_u16_field(value: &str, name: &str, line_number: usize) -> Result<Option<u16>> {
    if is_wildcard_local_parameter_field(value) {
        Ok(None)
    } else {
        parse_required_u16_field(value, name, line_number).map(Some)
    }
}

fn parse_optional_u8_field(value: &str, name: &str, line_number: usize) -> Result<Option<u8>> {
    if is_wildcard_local_parameter_field(value) {
        Ok(None)
    } else {
        parse_required_u8_field(value, name, line_number).map(Some)
    }
}

fn is_wildcard_local_parameter_field(value: &str) -> bool {
    value.is_empty() || value == "*" || value.eq_ignore_ascii_case("any")
}

fn validate_local_parameter_text(field_name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Other(format!(
            "local parameter {field_name} cannot be empty"
        )));
    }
    if value.chars().any(|ch| ch == '\n' || ch == '\r') {
        return Err(Error::Other(format!(
            "local parameter {field_name} cannot contain newlines"
        )));
    }
    Ok(())
}

fn local_parameter_keys_overlap(
    left: &OwnedLocalParameterEntry,
    right: &OwnedLocalParameterEntry,
) -> bool {
    left.center_id == right.center_id
        && optional_u16_matches(left.subcenter_id, right.subcenter_id)
        && optional_u8_matches(left.local_table_version, right.local_table_version)
        && left.discipline == right.discipline
        && left.category == right.category
        && left.number == right.number
}

fn optional_u16_matches(left: Option<u16>, right: Option<u16>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

fn optional_u8_matches(left: Option<u8>, right: Option<u8>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

fn local_parameter_entry_specificity(entry: LocalParameterEntry<'_>) -> u8 {
    u8::from(entry.subcenter_id.is_some()) + u8::from(entry.local_table_version.is_some())
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
    description: "Maximum/Composite radar reflectivity",
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
        assert_eq!(
            parameter.description,
            "Maximum/Composite radar reflectivity"
        );
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

    #[test]
    fn authored_local_table_csv_resolves_local_use_codes() {
        let table = LocalParameterTable::from_csv_str(
            r#"
            # center-defined local GRIB2 table
            center_id,subcenter_id,local_table_version,discipline,category,number,short_name,description
            42,5,3,0,192,1,XFOO,Example local parameter
            42,,3,0,16,196,LREFC,Local composite reflectivity
            "#,
        )
        .unwrap();
        let entries = table.entries();

        let parameter = lookup_parameter_with_local_entries(0, 192, 1, 42, 5, 3, &entries);
        assert_eq!(parameter.short_name, "XFOO");
        assert_eq!(parameter.description, "Example local parameter");

        let parameter = lookup_parameter_with_local_entries(0, 16, 196, 42, 99, 3, &entries);
        assert_eq!(parameter.short_name, "LREFC");
        assert_eq!(parameter.description, "Local composite reflectivity");
    }

    #[test]
    fn authored_local_table_csv_roundtrips() {
        let entry = OwnedLocalParameterEntry::new(
            42,
            None,
            Some(3),
            0,
            16,
            196,
            "LREFC",
            "Local composite reflectivity, experimental",
        )
        .unwrap();
        let table = LocalParameterTable::from_entries([entry]).unwrap();

        let encoded = table.to_csv_string();
        assert!(encoded.starts_with(LOCAL_PARAMETER_TABLE_CSV_HEADER));

        let reparsed = LocalParameterTable::from_csv_str(&encoded).unwrap();
        assert_eq!(reparsed.authored_entries(), table.authored_entries());
    }

    #[test]
    fn authored_local_table_rejects_ambiguous_overlaps() {
        let first = OwnedLocalParameterEntry::new(
            42,
            None,
            Some(3),
            0,
            16,
            196,
            "LREFC",
            "Local composite reflectivity",
        )
        .unwrap();
        let second = OwnedLocalParameterEntry::new(
            42,
            Some(5),
            Some(3),
            0,
            16,
            196,
            "LREFC5",
            "Subcenter local composite reflectivity",
        )
        .unwrap();

        let err = LocalParameterTable::from_entries([first, second]).unwrap_err();
        assert!(matches!(err, Error::Other(message) if message.contains("duplicate")));
    }

    #[test]
    fn authored_local_table_rejects_standard_only_codes() {
        let err = OwnedLocalParameterEntry::new(
            42,
            Some(5),
            Some(3),
            0,
            0,
            0,
            "BADTMP",
            "Bad local temperature",
        )
        .unwrap_err();

        assert!(
            matches!(err, Error::Other(message) if message.contains("local category or parameter number"))
        );
    }
}
