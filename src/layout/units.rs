/// Unit system for Circos values.
///
/// Units:
/// - `r` : relative (fraction of radius)
/// - `p` : pixel
/// - `u` : chromosome unit (defined by chromosomes_units)
/// - `b` : bases (natural genomic unit)
/// - `n` : no unit (bare number)

/// Extract the unit suffix from a value string.
/// Returns the unit character, or the no-unit default if it ends in a digit.
pub fn unit_fetch(value: &str, units_ok: &str, units_nounit: &str) -> Result<String, String> {
    let value = value.trim();
    if let Some(last) = value.chars().last() {
        if units_ok.contains(last) {
            return Ok(last.to_string());
        } else if last.is_ascii_digit() || last == '.' {
            return Ok(units_nounit.to_string());
        }
    }
    Err(format!("value '{}' has no recognized unit", value))
}

/// Split a value into its numeric part and unit.
pub fn unit_split(value: &str, units_ok: &str, units_nounit: &str) -> Result<(f64, String), String> {
    let unit = unit_fetch(value, units_ok, units_nounit)?;
    let numeric_str = unit_strip(value, units_ok, units_nounit)?;
    let numeric: f64 = numeric_str
        .parse()
        .map_err(|_| format!("cannot parse '{}' as number", numeric_str))?;
    Ok((numeric, unit))
}

/// Remove the unit suffix from a value string and return the numeric part as a string.
pub fn unit_strip(value: &str, units_ok: &str, units_nounit: &str) -> Result<String, String> {
    let unit = unit_fetch(value, units_ok, units_nounit)?;
    let value = value.trim();
    if unit == units_nounit {
        // No unit suffix to strip
        Ok(value.to_string())
    } else {
        Ok(value.trim_end_matches(|c: char| c.to_string() == unit).to_string())
    }
}

/// Validate that a value has one of the expected units.
pub fn unit_validate(
    value: &str,
    units_ok: &str,
    units_nounit: &str,
    allowed: &[&str],
) -> Result<String, String> {
    let unit = unit_fetch(value, units_ok, units_nounit)?;
    if allowed.iter().any(|&u| u == unit) {
        Ok(value.to_string())
    } else {
        Err(format!(
            "value '{}' has unit '{}', expected one of {:?}",
            value, unit, allowed
        ))
    }
}

/// Convert a numeric value between units using a conversion factor map.
///
/// `factors` maps "from_unit" + "to_unit" (e.g., "ub", "rp") to a factor.
pub fn unit_convert(
    value: f64,
    unit_from: &str,
    unit_to: &str,
    factors: &std::collections::HashMap<String, f64>,
) -> Result<f64, String> {
    if unit_from == unit_to {
        return Ok(value);
    }

    let key_forward = format!("{}{}", unit_from, unit_to);
    let key_reverse = format!("{}{}", unit_to, unit_from);

    if let Some(&factor) = factors.get(&key_forward) {
        Ok(value * factor)
    } else if let Some(&factor) = factors.get(&key_reverse) {
        Ok(value / factor)
    } else {
        Err(format!(
            "no conversion factor for '{}' -> '{}'",
            unit_from, unit_to
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_unit_fetch() {
        assert_eq!(unit_fetch("100p", "bupr", "n").unwrap(), "p");
        assert_eq!(unit_fetch("0.5r", "bupr", "n").unwrap(), "r");
        assert_eq!(unit_fetch("10u", "bupr", "n").unwrap(), "u");
        assert_eq!(unit_fetch("42", "bupr", "n").unwrap(), "n");
        assert_eq!(unit_fetch("1000b", "bupr", "n").unwrap(), "b");
    }

    #[test]
    fn test_unit_split() {
        let (val, unit) = unit_split("100p", "bupr", "n").unwrap();
        assert!((val - 100.0).abs() < 1e-10);
        assert_eq!(unit, "p");

        let (val, unit) = unit_split("0.85r", "bupr", "n").unwrap();
        assert!((val - 0.85).abs() < 1e-10);
        assert_eq!(unit, "r");

        let (val, unit) = unit_split("42", "bupr", "n").unwrap();
        assert!((val - 42.0).abs() < 1e-10);
        assert_eq!(unit, "n");
    }

    #[test]
    fn test_unit_strip() {
        assert_eq!(unit_strip("100p", "bupr", "n").unwrap(), "100");
        assert_eq!(unit_strip("0.85r", "bupr", "n").unwrap(), "0.85");
        assert_eq!(unit_strip("42", "bupr", "n").unwrap(), "42");
    }

    #[test]
    fn test_unit_convert() {
        let mut factors = HashMap::new();
        factors.insert("ub".to_string(), 1_000_000.0);
        factors.insert("rp".to_string(), 1500.0);

        assert!((unit_convert(10.0, "u", "b", &factors).unwrap() - 10_000_000.0).abs() < 1e-10);
        assert!((unit_convert(0.5, "r", "p", &factors).unwrap() - 750.0).abs() < 1e-10);
        assert!((unit_convert(42.0, "p", "p", &factors).unwrap() - 42.0).abs() < 1e-10);
    }

    #[test]
    fn test_unit_validate() {
        assert!(unit_validate("100p", "bupr", "n", &["p", "r"]).is_ok());
        assert!(unit_validate("0.5r", "bupr", "n", &["p", "r"]).is_ok());
        assert!(unit_validate("10u", "bupr", "n", &["p", "r"]).is_err());
    }
}
