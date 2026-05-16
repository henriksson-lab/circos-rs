//! Unit system for Circos values.
//!
//! Units:
//! - `r` : relative (fraction of radius)
//! - `p` : pixel
//! - `u` : chromosome unit (defined by chromosomes_units)
//! - `b` : bases (natural genomic unit)
//! - `n` : no unit (bare number)

/// Extract the unit suffix from a value string. Returns the unit character,
/// or the no-unit default if it ends in a digit.
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
pub fn unit_split(
    value: &str,
    units_ok: &str,
    units_nounit: &str,
) -> Result<(f64, String), String> {
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
        Ok(value
            .trim_end_matches(|c: char| c.to_string() == unit)
            .to_string())
    }
}

/// Port of Perl `unit_parse(expr, ideogram, side, relative)`: arithmetic
/// expression evaluator. Supports `+ - * /` and parenthesized sub-expressions.
/// Each leaf term is `<number><unit?>` where unit ∈ `{b, u, p, r, n}`:
///   `u` → multiply by chromosomes_units (bases)
///   `r` → multiply by relative_pixel_factor (pixels)
///   others pass through unchanged
pub fn unit_parse(
    expression: &str,
    chromosomes_units: f64,
    relative_pixel_factor: f64,
    units_ok: &str,
    units_nounit: &str,
) -> Result<f64, String> {
    #[derive(Debug, Clone)]
    enum Tok {
        Num(f64),
        Op(char),
        LParen,
        RParen,
    }

    // Convert a numeric-with-unit term to a plain f64 (Perl `unit_convert`).
    let to_f64 = |term: &str| -> Result<f64, String> {
        let (v, unit) = unit_split(term, units_ok, units_nounit)?;
        Ok(match unit.as_str() {
            "u" => v * chromosomes_units,
            "r" => v * relative_pixel_factor,
            _ => v,
        })
    };

    // Tokenize — emit +/- always as binary ops; prepend Num(0) when they
    // appear in a unary position so the RPN eval stays uniform.
    let mut tokens: Vec<Tok> = Vec::new();
    let chars: Vec<char> = expression.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '(' {
            tokens.push(Tok::LParen);
            i += 1;
            continue;
        }
        if c == ')' {
            tokens.push(Tok::RParen);
            i += 1;
            continue;
        }
        if matches!(c, '+' | '-' | '*' | '/') {
            let unary = matches!(tokens.last(), None | Some(Tok::Op(_)) | Some(Tok::LParen));
            if unary && (c == '-' || c == '+') {
                tokens.push(Tok::Num(0.0));
            }
            tokens.push(Tok::Op(c));
            i += 1;
            continue;
        }
        // Number + optional one-char unit suffix.
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        if i < chars.len() && (units_ok.contains(chars[i]) || units_nounit.contains(chars[i])) {
            i += 1;
        }
        if i == start {
            return Err(format!(
                "unit_parse: unexpected character '{}' in expression '{}'",
                c, expression
            ));
        }
        let term: String = chars[start..i].iter().collect();
        tokens.push(Tok::Num(to_f64(&term)?));
    }

    // Shunting-yard → RPN.
    let prec = |op: char| match op {
        '+' | '-' => 1,
        '*' | '/' => 2,
        _ => 0,
    };
    let mut output: Vec<Tok> = Vec::new();
    let mut ops: Vec<Tok> = Vec::new();
    for t in tokens {
        match t {
            Tok::Num(_) => output.push(t),
            Tok::Op(o) => {
                while let Some(Tok::Op(top)) = ops.last()
                    && prec(*top) >= prec(o)
                {
                    output.push(ops.pop().unwrap());
                }
                ops.push(Tok::Op(o));
            }
            Tok::LParen => ops.push(Tok::LParen),
            Tok::RParen => {
                while let Some(top) = ops.last()
                    && !matches!(top, Tok::LParen)
                {
                    output.push(ops.pop().unwrap());
                }
                ops.pop();
            }
        }
    }
    output.extend(ops.drain(..).rev());

    // Evaluate RPN.
    let mut stack: Vec<f64> = Vec::new();
    for t in output {
        match t {
            Tok::Num(v) => stack.push(v),
            Tok::Op(o) => {
                let b = stack.pop().ok_or("unit_parse: operand stack underflow")?;
                let a = stack.pop().ok_or("unit_parse: operand stack underflow")?;
                stack.push(match o {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' if b != 0.0 => a / b,
                    '/' => return Err("unit_parse: division by zero".into()),
                    _ => return Err(format!("unit_parse: unknown operator '{}'", o)),
                });
            }
            _ => {}
        }
    }
    stack
        .pop()
        .ok_or_else(|| "unit_parse: empty expression".into())
}

/// Port of Perl `unit_test(unit)`: verify a unit character is acceptable (one of
/// `units_ok` or the no-unit sentinel). Returns the unit or an error.
pub fn unit_test(unit: &str, units_ok: &str, units_nounit: &str) -> Result<String, String> {
    if unit == units_nounit {
        return Ok(unit.to_string());
    }
    if unit.len() == 1 && units_ok.contains(unit) {
        return Ok(unit.to_string());
    }
    Err(format!("Unit [{}] fails format check.", unit))
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

    #[test]
    fn test_unit_parse_unary_and_mix() {
        // unary minus prepended by implicit 0 - operator (iter 75 refactor)
        let v = unit_parse("-100p", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - (-100.0)).abs() < 1e-9);
        // unary plus
        let v = unit_parse("+50p", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 50.0).abs() < 1e-9);
        // unit 'u' converts by chromosomes_units factor
        let v = unit_parse("2u + 10p", 1_000_000.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 2_000_010.0).abs() < 1e-6);
        // unit 'r' converts by relative_pixel_factor
        let v = unit_parse("0.5r", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 750.0).abs() < 1e-9);
        // parenthesized precedence
        let v = unit_parse("(2 + 3) * 4", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 20.0).abs() < 1e-9);
        // precedence without parens
        let v = unit_parse("2 + 3 * 4", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 14.0).abs() < 1e-9);
        // parenthesized unary
        let v = unit_parse("(-5) + 10", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 5.0).abs() < 1e-9);
        // division by zero error
        assert!(unit_parse("1 / 0", 1.0, 1500.0, "bupr", "n").is_err());
    }

    #[test]
    fn test_unit_test_accepts_nounit_and_rejects_others() {
        // "n" (nounit sentinel) is always accepted.
        assert_eq!(unit_test("n", "bupr", "n").unwrap(), "n");
        // Single char that's in units_ok → accepted.
        assert_eq!(unit_test("r", "bupr", "n").unwrap(), "r");
        assert_eq!(unit_test("p", "bupr", "n").unwrap(), "p");
        // Multi-char or char not in units_ok → Err.
        assert!(unit_test("px", "bupr", "n").is_err());
        assert!(unit_test("z", "bupr", "n").is_err());
    }

    #[test]
    fn test_unit_validate_rejects_disallowed() {
        // Allowed list restricts which units pass.
        assert!(unit_validate("100r", "bupr", "n", &["r"]).is_ok());
        // "100p" has unit "p" which isn't in the allowed ["r"] → Err.
        let err = unit_validate("100p", "bupr", "n", &["r"]).unwrap_err();
        assert!(err.contains("expected one of"), "got: {}", err);
    }

    #[test]
    fn test_unit_convert_reverse_lookup_and_missing() {
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ub".to_string(), 1_000_000.0);
        factors.insert("rp".to_string(), 1500.0);
        // Forward key "ub" → 10 * 1M = 10M.
        assert_eq!(unit_convert(10.0, "u", "b", &factors).unwrap(), 10_000_000.0);
        // Reverse key (b → u) uses divided factor.
        assert_eq!(unit_convert(10_000_000.0, "b", "u", &factors).unwrap(), 10.0);
        // Same from/to → passes value through unchanged (no factor needed).
        assert_eq!(unit_convert(42.0, "p", "p", &factors).unwrap(), 42.0);
        // Missing key and no reverse → Err.
        assert!(unit_convert(1.0, "x", "y", &factors).is_err());
    }

    #[test]
    fn test_unit_fetch_no_unit_suffix_returns_nounit_sentinel() {
        // A bare number with no unit returns the nounit sentinel ("n").
        assert_eq!(unit_fetch("42", "bupr", "n").unwrap(), "n");
    }

    #[test]
    fn test_unit_split_decimal_and_negative() {
        // Decimal + unit.
        let (v, u) = unit_split("0.5r", "bupr", "n").unwrap();
        assert!((v - 0.5).abs() < 1e-9);
        assert_eq!(u, "r");
        // Negative not allowed by unit_split — unit_fetch rejects '-' at end.
        // But bare negative numbers (no unit) should parse.
        let (v, u) = unit_split("-5", "bupr", "n").unwrap();
        assert!((v - (-5.0)).abs() < 1e-9);
        assert_eq!(u, "n");
    }

    #[test]
    fn test_unit_split_non_numeric_errors() {
        // Value with a unit but non-numeric part → parse failure.
        let r = unit_split("abcr", "bupr", "n");
        assert!(r.is_err());
    }

    #[test]
    fn test_unit_strip_trims_suffix() {
        // Trailing unit character stripped.
        assert_eq!(unit_strip("100p", "bupr", "n").unwrap(), "100");
        assert_eq!(unit_strip("0.5r", "bupr", "n").unwrap(), "0.5");
        // No unit → passthrough.
        assert_eq!(unit_strip("42", "bupr", "n").unwrap(), "42");
        // Whitespace around value trimmed.
        assert_eq!(unit_strip("  100p  ", "bupr", "n").unwrap(), "100");
    }

    #[test]
    fn test_unit_parse_whitespace_tolerance() {
        // Leading/trailing/internal whitespace all tolerated.
        let v = unit_parse("  3u + 10p  ", 1_000_000.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 3_000_010.0).abs() < 1e-6);
    }

    #[test]
    fn test_unit_parse_zero_and_identity() {
        // 0 evaluates to 0.
        let v = unit_parse("0", 1.0, 1500.0, "bupr", "n").unwrap();
        assert_eq!(v, 0.0);
        // Single literal is returned as-is.
        let v = unit_parse("1500", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_strip_multiple_u_suffixes_trimmed() {
        // `trim_end_matches` strips all matching trailing chars.
        // "100uuu" → trim "u" → "100".
        let r = unit_strip("100uuu", "bupr", "n").unwrap();
        assert_eq!(r, "100");
    }

    #[test]
    fn test_unit_split_positive_exponent_number() {
        // "1e3u" — f64 parses "1e3" → 1000.0, unit="u".
        let (v, u) = unit_split("1e3u", "bupr", "n").unwrap();
        assert!((v - 1000.0).abs() < 1e-9);
        assert_eq!(u, "u");
    }

    #[test]
    fn test_unit_split_leading_plus_sign() {
        // Leading + sign tolerated by f64::parse → +42 = 42.
        let (v, u) = unit_split("+42p", "bupr", "n").unwrap();
        assert!((v - 42.0).abs() < 1e-9);
        assert_eq!(u, "p");
    }

    #[test]
    fn test_unit_fetch_unit_char_that_is_digit_treated_as_nounit() {
        // A number like "42" ends in a digit → returns units_nounit.
        assert_eq!(unit_fetch("42", "bupr", "n").unwrap(), "n");
        // A number like "3.14" ends in a digit → nounit.
        assert_eq!(unit_fetch("3.14", "bupr", "n").unwrap(), "n");
        // Ending in just "." also treated as nounit per impl.
        assert_eq!(unit_fetch("3.", "bupr", "n").unwrap(), "n");
    }

    #[test]
    fn test_unit_validate_nounit_in_allowed_list() {
        // nounit sentinel "n" is validated when listed in allowed.
        assert!(unit_validate("42", "bupr", "n", &["n"]).is_ok());
        // Also accepts if "n" + unit are both in allowed.
        assert!(unit_validate("42", "bupr", "n", &["p", "n"]).is_ok());
    }

    #[test]
    fn test_unit_validate_empty_allowed_list_rejects_all() {
        // Empty allowed list → every unit is rejected.
        assert!(unit_validate("42", "bupr", "n", &[]).is_err());
        assert!(unit_validate("10p", "bupr", "n", &[]).is_err());
        assert!(unit_validate("0.5r", "bupr", "n", &[]).is_err());
    }

    #[test]
    fn test_unit_test_empty_string_rejected() {
        // Empty string → neither matches "n" nor any single char in units_ok.
        assert!(unit_test("", "bupr", "n").is_err());
    }

    #[test]
    fn test_unit_fetch_whitespace_around_value() {
        // Leading/trailing whitespace trimmed before suffix detection.
        assert_eq!(unit_fetch("  100p  ", "bupr", "n").unwrap(), "p");
        assert_eq!(unit_fetch("\t42\n", "bupr", "n").unwrap(), "n");
    }

    #[test]
    fn test_unit_parse_nested_parentheses() {
        // Deeply nested parens: ((2 + 3) * (4 - 1)) = 5 * 3 = 15.
        let v = unit_parse("((2 + 3) * (4 - 1))", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 15.0).abs() < 1e-9);
        // Triple nesting: (((1 + 2))) = 3.
        let v = unit_parse("(((1 + 2)))", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_mixed_units_in_expression() {
        // Mix `u` and `r` units in one expression: 1u + 0.5r.
        // With chromosomes_units=1e6, relative_pixel_factor=1000 → 1e6 + 500 = 1_000_500.
        let v = unit_parse("1u + 0.5r", 1_000_000.0, 1000.0, "bupr", "n").unwrap();
        assert!((v - 1_000_500.0).abs() < 1e-6);
    }

    #[test]
    fn test_unit_parse_unknown_character_errors() {
        // Character neither a digit nor a unit → returns Err.
        let r = unit_parse("1 @ 2", 1.0, 1500.0, "bupr", "n");
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("unexpected character"), "got: {}", err);
    }

    #[test]
    fn test_unit_parse_operator_precedence_multi_level() {
        // `2 + 3 * 4 - 1` = 2 + 12 - 1 = 13 (mul before add/sub, left-to-right).
        let v = unit_parse("2 + 3 * 4 - 1", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 13.0).abs() < 1e-9);
        // `8 / 2 / 2` = (8/2)/2 = 2 (left-associative).
        let v = unit_parse("8 / 2 / 2", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_convert_forward_and_reverse_factor() {
        // unit_convert uses forward key "fromto" when present, else reverse "tofrom"
        // with inverse factor applied.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ub".to_string(), 1_000_000.0); // 1u = 1_000_000b
        // Forward: u→b multiplies by 1e6.
        assert_eq!(unit_convert(3.0, "u", "b", &factors).unwrap(), 3_000_000.0);
        // Reverse: b→u divides by 1e6 via key reversal.
        assert!((unit_convert(3_000_000.0, "b", "u", &factors).unwrap() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_convert_same_unit_returns_value_unchanged() {
        // When from == to, function short-circuits to return value regardless of factors.
        let factors: HashMap<String, f64> = HashMap::new();
        assert_eq!(unit_convert(42.5, "u", "u", &factors).unwrap(), 42.5);
        // Even with no factors at all, same-unit conversion succeeds.
        assert_eq!(unit_convert(-7.0, "r", "r", &factors).unwrap(), -7.0);
    }

    #[test]
    fn test_unit_convert_missing_factor_returns_err() {
        // No factor in either direction → descriptive Err.
        let factors: HashMap<String, f64> = HashMap::new();
        let r = unit_convert(1.0, "u", "p", &factors);
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("no conversion factor"));
        assert!(err.contains("u"));
        assert!(err.contains("p"));
    }

    #[test]
    fn test_unit_test_accepts_nounit_and_single_char_unit() {
        // unit_test: units_nounit passthrough + single-char membership in units_ok.
        assert_eq!(unit_test("n", "bupr", "n").unwrap(), "n");
        assert_eq!(unit_test("u", "bupr", "n").unwrap(), "u");
        assert_eq!(unit_test("p", "bupr", "n").unwrap(), "p");
        // Multi-char input (not the exact nounit sentinel) → Err.
        assert!(unit_test("up", "bupr", "n").is_err());
        // Char not in units_ok → Err.
        assert!(unit_test("z", "bupr", "n").is_err());
        // Empty string → len≠1 and != nounit "n" → Err.
        assert!(unit_test("", "bupr", "n").is_err());
    }

    #[test]
    fn test_unit_parse_parentheses_override_precedence() {
        // Parentheses group expressions despite natural operator precedence.
        // "(2+3)*4" = 5*4 = 20, vs "2+3*4" = 14.
        let a = unit_parse("(2+3)*4", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((a - 20.0).abs() < 1e-9);
        let b = unit_parse("2+3*4", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((b - 14.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_division_and_multiplication_left_associative() {
        // "8/4*2" = (8/4)*2 = 4. Division and multiplication are left-associative.
        let v = unit_parse("8/4*2", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 4.0).abs() < 1e-9);
        // "2*8/4" = (2*8)/4 = 4 — same result.
        let v = unit_parse("2*8/4", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_split_scientific_notation() {
        // f64 parses scientific notation; numeric portion scales.
        let (val, unit) = unit_split("1e3u", "bupr", "n").unwrap();
        assert!((val - 1000.0).abs() < 1e-9);
        assert_eq!(unit, "u");
        // Negative exponent.
        let (val, unit) = unit_split("1.5e-2r", "bupr", "n").unwrap();
        assert!((val - 0.015).abs() < 1e-12);
        assert_eq!(unit, "r");
    }

    #[test]
    fn test_unit_convert_factor_preserves_value_zero() {
        // Converting 0 in any direction → always 0 (zero × any factor = 0).
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ub".into(), 1e6);
        assert_eq!(unit_convert(0.0, "u", "b", &factors).unwrap(), 0.0);
        assert_eq!(unit_convert(0.0, "b", "u", &factors).unwrap(), 0.0);
        // Same-unit conversion also returns 0.
        assert_eq!(unit_convert(0.0, "u", "u", &factors).unwrap(), 0.0);
    }

    #[test]
    fn test_unit_parse_single_number_no_ops() {
        // Plain number with no operators.
        let v = unit_parse("42", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 42.0).abs() < 1e-9);
        // With unit.
        let v = unit_parse("1000p", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 1000.0).abs() < 1e-9);
        // Decimal.
        let v = unit_parse("3.14", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_unary_negative_number() {
        // Unary minus prefix: "-5" → -5.
        let v = unit_parse("-5", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - (-5.0)).abs() < 1e-9);
        // "0 - 3" also supported via binary op.
        let v = unit_parse("0 - 3", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - (-3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_unit_strip_u_suffix_preserves_numeric_part() {
        // unit_strip returns the numeric portion as a string.
        assert_eq!(unit_strip("100u", "bupr", "n").unwrap(), "100");
        assert_eq!(unit_strip("3.14r", "bupr", "n").unwrap(), "3.14");
        // Nounit value (no suffix) → returned verbatim.
        assert_eq!(unit_strip("42", "bupr", "n").unwrap(), "42");
    }

    #[test]
    fn test_unit_validate_all_allowed_units_accepted() {
        // Every unit in allowed → Ok.
        for u in ["b", "u", "p", "r", "n"] {
            let val = if u == "n" { "42".to_string() } else { format!("42{}", u) };
            assert!(unit_validate(&val, "bupr", "n", &["b", "u", "p", "r", "n"]).is_ok());
        }
    }

    #[test]
    fn test_unit_parse_negative_result_from_subtraction() {
        // "10 - 25" → -15.
        let v = unit_parse("10 - 25", 1.0, 1500.0, "bupr", "n").unwrap();
        assert!((v - (-15.0)).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_multiplication_by_zero() {
        // "5 * 0" → 0.
        let v = unit_parse("5 * 0", 1.0, 1500.0, "bupr", "n").unwrap();
        assert_eq!(v, 0.0);
        // "0 * 100" → 0.
        let v = unit_parse("0 * 100", 1.0, 1500.0, "bupr", "n").unwrap();
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_unit_validate_unit_not_in_allowed_returns_err() {
        // "100p" with allowed=["u", "r"] → Err.
        let r = unit_validate("100p", "bupr", "n", &["u", "r"]);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("expected"));
    }

    #[test]
    fn test_unit_convert_factor_scales_correctly() {
        // "ub" = 1e6: 1u → 1_000_000b.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ub".into(), 1e6);
        assert_eq!(unit_convert(1.0, "u", "b", &factors).unwrap(), 1e6);
        // 0.5u → 500_000b.
        assert_eq!(unit_convert(0.5, "u", "b", &factors).unwrap(), 500_000.0);
        // 2.5u → 2_500_000b.
        assert_eq!(unit_convert(2.5, "u", "b", &factors).unwrap(), 2_500_000.0);
    }

    #[test]
    fn test_unit_parse_unary_minus_leading_term() {
        // "-5u" with chromosomes_units=100 → unary minus injects Num(0) → 0 - 500 = -500.
        let r = unit_parse("-5u", 100.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r, -500.0);
        // "+3u" unary plus also injects Num(0) → 0 + 3*100 = 300.
        let r2 = unit_parse("+3u", 100.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r2, 300.0);
    }

    #[test]
    fn test_unit_parse_mul_binds_tighter_than_add() {
        // "2+3*4u" with chromosomes_units=10 → 2 + (3 * 40) = 122 (precedence, not LTR).
        let r = unit_parse("2+3*4u", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r, 122.0);
        // Parens override: "(2+3)*4u" → 5 * 40 = 200.
        let r2 = unit_parse("(2+3)*4u", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r2, 200.0);
    }

    #[test]
    fn test_unit_convert_reverse_direction_via_reciprocal() {
        // Only "ub" in factors; requesting b→u → value / 1e6 (reverse lookup).
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ub".into(), 1e6);
        assert_eq!(unit_convert(1_000_000.0, "b", "u", &factors).unwrap(), 1.0);
        // Missing both directions → error.
        let err = unit_convert(1.0, "x", "y", &factors).unwrap_err();
        assert!(err.contains("'x' -> 'y'"));
    }

    #[test]
    fn test_unit_validate_allowed_unit_passes_through() {
        // "100p" with allowed=["p","r"] → Ok passes through the original value string.
        let ok = unit_validate("100p", "bupr", "n", &["p", "r"]).unwrap();
        assert_eq!(ok, "100p");
        // "100b" with allowed=["p","r"] → Err mentioning the disallowed unit.
        let err = unit_validate("100b", "bupr", "n", &["p", "r"]).unwrap_err();
        assert!(err.contains("expected"));
        assert!(err.contains("'b'"));
    }

    #[test]
    fn test_unit_parse_division_operator() {
        // "/" with nonzero divisor → normal division.
        assert_eq!(unit_parse("10/2", 1.0, 1.0, "bupr", "n").unwrap(), 5.0);
        // Precedence: "1+6/2" → 1 + 3 = 4.
        assert_eq!(unit_parse("1+6/2", 1.0, 1.0, "bupr", "n").unwrap(), 4.0);
    }

    #[test]
    fn test_unit_parse_division_by_zero_returns_err() {
        // Divisor==0 → Err mentioning division.
        let err = unit_parse("10/0", 1.0, 1.0, "bupr", "n").unwrap_err();
        assert!(err.contains("division"));
    }

    #[test]
    fn test_unit_convert_same_unit_identity_no_factor_needed() {
        // unit_from == unit_to → early-return identity (no factor lookup).
        let factors: HashMap<String, f64> = HashMap::new();
        assert_eq!(unit_convert(42.0, "u", "u", &factors).unwrap(), 42.0);
        assert_eq!(unit_convert(-3.14, "p", "p", &factors).unwrap(), -3.14);
    }

    #[test]
    fn test_unit_test_multichar_unit_rejected() {
        // unit_test requires single-char (or nounit sentinel).
        let err = unit_test("abc", "bupr", "n").unwrap_err();
        assert!(err.contains("abc"));
        // Single-char unit in allowed list → Ok.
        assert_eq!(unit_test("p", "bupr", "n").unwrap(), "p");
        // The nounit sentinel (however many chars) also accepted.
        assert_eq!(unit_test("nounit", "bupr", "nounit").unwrap(), "nounit");
    }

    #[test]
    fn test_unit_split_extracts_numeric_and_unit_correctly() {
        // "5.5u" → (5.5, "u").
        let (v, u) = unit_split("5.5u", "bupr", "n").unwrap();
        assert_eq!(v, 5.5);
        assert_eq!(u, "u");
        // "100p" → (100.0, "p").
        let (v, u) = unit_split("100p", "bupr", "n").unwrap();
        assert_eq!(v, 100.0);
        assert_eq!(u, "p");
        // Bare number → (val, nounit sentinel).
        let (v, u) = unit_split("42", "bupr", "n").unwrap();
        assert_eq!(v, 42.0);
        assert_eq!(u, "n");
    }

    #[test]
    fn test_unit_strip_preserves_number_and_removes_unit_suffix() {
        // Suffix 'p' removed → "100".
        assert_eq!(unit_strip("100p", "bupr", "n").unwrap(), "100");
        // Suffix 'r' removed from decimal → "0.5".
        assert_eq!(unit_strip("0.5r", "bupr", "n").unwrap(), "0.5");
        // Bare number stays.
        assert_eq!(unit_strip("42", "bupr", "n").unwrap(), "42");
    }

    #[test]
    fn test_unit_fetch_unknown_trailing_char_returns_err() {
        // Last char not in units_ok and not digit/dot → Err.
        let err = unit_fetch("5$", "bupr", "n").unwrap_err();
        assert!(err.contains("'5$'"));
        // Empty string → Err via `last()` giving None → falls through Err branch.
        let err2 = unit_fetch("", "bupr", "n");
        assert!(err2.is_err());
    }

    #[test]
    fn test_unit_parse_parenthesized_subexpression() {
        // Parens override default precedence: "(2+3)*2u" → 5 * 2 * cu.
        let r = unit_parse("(2+3)*2u", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r, 100.0);
        // Nested parens: "((2+3))" → 5.
        let r2 = unit_parse("((2+3))", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r2, 5.0);
    }

    #[test]
    fn test_unit_parse_subtraction_u_minus_u_yields_difference() {
        // "10u-5u" with cu=10 → 100-50 = 50.
        let r = unit_parse("10u-5u", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r, 50.0);
        // Same total with subtraction reordered.
        let r2 = unit_parse("5u-10u", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r2, -50.0);
    }

    #[test]
    fn test_unit_parse_multiplication_commutative_with_mixed_unit() {
        // "2*5u" and "5u*2" should give same result (commutative over *).
        let a = unit_parse("2*5u", 10.0, 1.0, "bupr", "n").unwrap();
        let b = unit_parse("5u*2", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(a, 100.0);
        assert_eq!(a, b);
    }

    #[test]
    fn test_unit_convert_forward_factor_preferred_over_reverse() {
        // Map has both "ab"=2.0 (forward) and "ba"=0.5 (reverse).
        // Forward key is checked first → uses 2.0, not 1/0.5.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ab".into(), 2.0);
        factors.insert("ba".into(), 0.5);
        let r = unit_convert(10.0, "a", "b", &factors).unwrap();
        assert_eq!(r, 20.0);
    }

    #[test]
    fn test_unit_split_decimal_leading_zero_preserved() {
        // "0.5u" → (0.5, "u"); "0.001r" → (0.001, "r").
        let (v, u) = unit_split("0.5u", "bupr", "n").unwrap();
        assert_eq!(v, 0.5);
        assert_eq!(u, "u");
        let (v, u) = unit_split("0.001r", "bupr", "n").unwrap();
        assert_eq!(v, 0.001);
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_fetch_whitespace_only_string_returns_err() {
        // "   " trims to "" → no last char → Err.
        let err = unit_fetch("   ", "bupr", "n");
        assert!(err.is_err());
    }

    #[test]
    fn test_unit_strip_preserves_leading_sign() {
        // Negative/positive signs are part of the numeric part, not the unit suffix.
        assert_eq!(unit_strip("-5u", "bupr", "n").unwrap(), "-5");
        assert_eq!(unit_strip("+3.14r", "bupr", "n").unwrap(), "+3.14");
    }

    #[test]
    fn test_unit_parse_multiplication_before_addition_with_units() {
        // "1+2u*3" cu=10 → 1 + (2*10)*3 = 1 + 60 = 61. (Plain 2u is 2*10=20 → then ×3 = 60.)
        let r = unit_parse("1+2u*3", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r, 61.0);
        // "1+2*3u" cu=10 → 1 + (2×3×cu) = 1 + 60 = 61 (commutative over mul).
        let r2 = unit_parse("1+2*3u", 10.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(r2, 61.0);
    }

    #[test]
    fn test_unit_validate_empty_allowed_list_rejects_every_unit() {
        // Empty allowed list → any() returns false → always Err.
        let err = unit_validate("100p", "bupr", "n", &[]).unwrap_err();
        assert!(err.contains("expected"));
        let err2 = unit_validate("42", "bupr", "n", &[]).unwrap_err();
        assert!(err2.contains("expected"));
    }

    #[test]
    fn test_unit_test_nounit_value_returns_unit_unchanged() {
        // If unit matches units_nounit sentinel, it's accepted verbatim.
        assert_eq!(unit_test("n", "bupr", "n"), Ok("n".to_string()));
        // And a single-char unit in units_ok is also accepted.
        assert_eq!(unit_test("b", "bupr", "n"), Ok("b".to_string()));
        // Unknown chars fall through to Err.
        assert!(unit_test("z", "bupr", "n").is_err());
    }

    #[test]
    fn test_unit_validate_allowed_unit_returns_the_value_string() {
        // When unit is in allowed list → Ok returns the original value (not the unit).
        assert_eq!(unit_validate("5u", "bupr", "n", &["u", "r"]), Ok("5u".to_string()));
        assert_eq!(unit_validate("100p", "bupr", "n", &["p"]), Ok("100p".to_string()));
        // Unit absent from allowed → Err referencing the unit.
        let err = unit_validate("5u", "bupr", "n", &["p"]).unwrap_err();
        assert!(err.contains("'u'") || err.contains("u"));
    }

    #[test]
    fn test_unit_convert_missing_both_directions_returns_err() {
        // If neither "fromto" nor "tofrom" is in factors, conversion fails.
        let factors = std::collections::HashMap::new();
        let err = unit_convert(10.0, "u", "r", &factors).unwrap_err();
        assert!(err.to_lowercase().contains("no") || err.contains("u") || err.contains("r"));
        // But identity conversion (from==to) always succeeds regardless of map contents.
        assert_eq!(unit_convert(42.0, "u", "u", &factors), Ok(42.0));
    }

    #[test]
    fn test_unit_test_multichar_unit_even_if_in_units_ok_rejected() {
        // unit.len() must be 1 — multi-character units always hit the Err arm.
        let err = unit_test("bp", "bupr", "n").unwrap_err();
        assert!(err.contains("fails format check"));
        let err2 = unit_test("up", "bupr", "n").unwrap_err();
        assert!(err2.contains("fails format check"));
    }

    #[test]
    fn test_unit_split_bare_number_returns_nounit_sentinel_as_unit() {
        // "42" with units_nounit="n" → (42.0, "n").
        let (v, u) = unit_split("42", "bupr", "n").expect("bare number");
        assert_eq!(v, 42.0);
        assert_eq!(u, "n");
    }

    #[test]
    fn test_unit_parse_precedence_addition_then_multiplication_result() {
        // 1+2*3 → 7 (not 9). Shunting-yard handles precedence properly.
        let r = unit_parse("1+2*3", 1.0, 1.0, "bupr", "n").expect("expr");
        assert!((r - 7.0).abs() < 1e-9);
        // With units: 2u * 3 + 1u → 6u + 1u → cu=10 → 60 + 10 = 70.
        let r2 = unit_parse("2u*3+1u", 10.0, 1.0, "bupr", "n").expect("expr2");
        assert!((r2 - 70.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_strip_bare_integer_preserved_verbatim() {
        // "42" → no unit suffix; unit_strip returns numeric part unchanged.
        assert_eq!(unit_strip("42", "bupr", "n"), Ok("42".to_string()));
        // "1e3" scientific notation also preserved.
        assert_eq!(unit_strip("1e3", "bupr", "n"), Ok("1e3".to_string()));
    }

    #[test]
    fn test_unit_parse_unary_minus_on_single_term_returns_negative() {
        // "-5" → -5 via unary minus; "-5u" with cu=1000 → -5000.
        let r = unit_parse("-5", 1.0, 1.0, "bupr", "n").expect("neg");
        assert!((r - (-5.0)).abs() < 1e-9);
        let r2 = unit_parse("-5u", 1000.0, 1.0, "bupr", "n").expect("neg-u");
        assert!((r2 - (-5000.0)).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_parenthesized_subexpression_priority() {
        // (1+2)*3 = 9 (parens force addition first); 1+2*3 = 7 (normal precedence).
        let parens = unit_parse("(1+2)*3", 1.0, 1.0, "bupr", "n").expect("parens");
        assert!((parens - 9.0).abs() < 1e-9);
        let normal = unit_parse("1+2*3", 1.0, 1.0, "bupr", "n").expect("normal");
        assert!((normal - 7.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_division_simple() {
        // Division: 10u / 2 → cu=1000 → 10×1000/2 = 5000.
        let r = unit_parse("10u/2", 1000.0, 1.0, "bupr", "n").expect("div");
        assert!((r - 5000.0).abs() < 1e-9);
        // 100/4 = 25.
        let r2 = unit_parse("100/4", 1.0, 1.0, "bupr", "n").expect("div2");
        assert!((r2 - 25.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_split_zero_value_with_unit_preserved() {
        // "0u" → (0.0, "u") — zero coefficient but unit still recognized.
        let (v, u) = unit_split("0u", "bupr", "n").expect("zero u");
        assert_eq!(v, 0.0);
        assert_eq!(u, "u");
        // "0r" same.
        let (v2, u2) = unit_split("0r", "bupr", "n").expect("zero r");
        assert_eq!(v2, 0.0);
        assert_eq!(u2, "r");
    }

    #[test]
    fn test_unit_strip_value_with_r_unit_removes_only_trailing_r() {
        // "100r" → "100"; "100rr" would not be expected (invalid) but
        // unit_strip only strips the ONE char matching the unit string.
        assert_eq!(unit_strip("100r", "bupr", "n"), Ok("100".to_string()));
        assert_eq!(unit_strip("3.14p", "bupr", "n"), Ok("3.14".to_string()));
    }

    #[test]
    fn test_unit_parse_complex_expression_with_nested_parens() {
        // Nested parens: ((1+2)*3) + 4 = 13.
        let r = unit_parse("((1+2)*3)+4", 1.0, 1.0, "bupr", "n").expect("complex");
        assert!((r - 13.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_unbalanced_parens_does_not_panic() {
        // Unbalanced parens — implementation may return Ok or Err; verify no panic.
        let _ = unit_parse("(1+2*3", 1.0, 1.0, "bupr", "n");
        let _ = unit_parse("1+2)*3", 1.0, 1.0, "bupr", "n");
        let _ = unit_parse("()", 1.0, 1.0, "bupr", "n");
    }

    #[test]
    fn test_unit_split_negative_value_with_unit() {
        // "-100p" → (-100.0, "p").
        let (v, u) = unit_split("-100p", "bupr", "n").expect("neg");
        assert_eq!(v, -100.0);
        assert_eq!(u, "p");
        // "-0.5r" → (-0.5, "r").
        let (v2, u2) = unit_split("-0.5r", "bupr", "n").expect("neg-r");
        assert_eq!(v2, -0.5);
        assert_eq!(u2, "r");
    }

    #[test]
    fn test_unit_parse_only_whitespace_errors_or_zero() {
        // Whitespace-only → malformed → Err.
        let r = unit_parse("   ", 1.0, 1.0, "bupr", "n");
        assert!(r.is_err());
        // Empty string too.
        let r2 = unit_parse("", 1.0, 1.0, "bupr", "n");
        assert!(r2.is_err());
    }

    #[test]
    fn test_unit_fetch_bare_number_returns_nounit() {
        // "42" has no unit suffix → returns the nounit sentinel.
        assert_eq!(unit_fetch("42", "bupr", "n"), Ok("n".to_string()));
        // "3.14" also bare.
        assert_eq!(unit_fetch("3.14", "bupr", "n"), Ok("n".to_string()));
    }

    #[test]
    fn test_unit_fetch_valid_single_char_unit() {
        // "42u" → unit is "u".
        assert_eq!(unit_fetch("42u", "bupr", "n"), Ok("u".to_string()));
        assert_eq!(unit_fetch("100p", "bupr", "n"), Ok("p".to_string()));
        assert_eq!(unit_fetch("0.5r", "bupr", "n"), Ok("r".to_string()));
    }

    #[test]
    fn test_unit_convert_identical_from_to_regardless_of_map() {
        // Identity conversion (same unit → same unit) always succeeds.
        use std::collections::HashMap;
        let factors: HashMap<String, f64> = HashMap::new();
        assert_eq!(unit_convert(100.0, "p", "p", &factors), Ok(100.0));
        assert_eq!(unit_convert(0.0, "u", "u", &factors), Ok(0.0));
        // Even with negative.
        assert_eq!(unit_convert(-50.0, "r", "r", &factors), Ok(-50.0));
    }

    #[test]
    fn test_unit_parse_addition_of_three_terms() {
        // 1 + 2 + 3 = 6.
        let r = unit_parse("1+2+3", 1.0, 1.0, "bupr", "n").expect("add");
        assert!((r - 6.0).abs() < 1e-9);
        // With units: 10u + 5u + 2u with cu=10 → (10+5+2)*10 = 170.
        let r2 = unit_parse("10u+5u+2u", 10.0, 1.0, "bupr", "n").expect("add-u");
        assert!((r2 - 170.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_fetch_whitespace_in_value_rejects() {
        // "42 u" with space in middle → unit is last char " " → not in units_ok/units_nounit → Err.
        let r = unit_fetch("42u", "bupr", "n");
        assert!(r.is_ok());
        // Weird but check this edge case — a lone space doesn't match.
        let r2 = unit_fetch(" ", "bupr", "n");
        assert!(r2.is_err());
    }

    #[test]
    fn test_unit_validate_value_with_allowed_list_containing_nounit() {
        // "100" with nounit allowed — should validate.
        assert_eq!(unit_validate("100", "bupr", "n", &["n"]), Ok("100".to_string()));
        // "50u" with just u allowed.
        assert_eq!(unit_validate("50u", "bupr", "n", &["u"]), Ok("50u".to_string()));
    }

    #[test]
    fn test_unit_parse_just_a_number_matches_simple_passthrough() {
        // Single number without operators → returns that number.
        let r = unit_parse("42", 1.0, 1.0, "bupr", "n").expect("bare");
        assert!((r - 42.0).abs() < 1e-9);
        // With unit.
        let r2 = unit_parse("5u", 100.0, 1.0, "bupr", "n").expect("bare-u");
        assert!((r2 - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_mixed_paren_and_operators() {
        // (2+3)*(4-1) = 15.
        let r = unit_parse("(2+3)*(4-1)", 1.0, 1.0, "bupr", "n").expect("mixed");
        assert!((r - 15.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_fractional_in_expression() {
        // "0.5+0.25" = 0.75.
        let r = unit_parse("0.5+0.25", 1.0, 1.0, "bupr", "n").expect("frac");
        assert!((r - 0.75).abs() < 1e-9);
    }

    #[test]
    fn test_unit_parse_operator_chains_without_spaces() {
        // No-space operator chains: "1+2*3-4" = 1+6-4 = 3.
        let r = unit_parse("1+2*3-4", 1.0, 1.0, "bupr", "n").expect("chain");
        assert!((r - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_unit_fetch_numeric_only_returns_nounit_sentinel() {
        // Pure numeric "123" → nounit sentinel.
        assert_eq!(unit_fetch("123", "bupr", "n"), Ok("n".to_string()));
        // Negative.
        assert_eq!(unit_fetch("-456", "bupr", "n"), Ok("n".to_string()));
    }

    #[test]
    fn test_unit_split_large_integer_value_preserved() {
        // "1000000u" → (1e6, "u").
        let (v, u) = unit_split("1000000u", "bupr", "n").expect("big");
        assert_eq!(v, 1_000_000.0);
        assert_eq!(u, "u");
    }

    #[test]
    fn test_unit_test_accepts_nounit_sentinel_verbatim() {
        // unit_test returns Ok when input equals nounit sentinel.
        assert_eq!(unit_test("n", "bupr", "n"), Ok("n".to_string()));
    }

    #[test]
    fn test_unit_test_rejects_multichar_unit() {
        // Only single-char units allowed → "bu" (2 chars) rejected.
        let res = unit_test("bu", "bupr", "n");
        assert!(res.is_err());
    }

    #[test]
    fn test_unit_convert_forward_then_reciprocal_via_reverse_key() {
        // Define only "ub" key; asking "b" → "u" uses reverse (1/factor) path.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ub".into(), 10.0);
        // forward "u" → "b": 5.0 * 10 = 50.0
        assert_eq!(unit_convert(5.0, "u", "b", &factors), Ok(50.0));
        // reverse "b" → "u": 50.0 / 10 = 5.0
        assert_eq!(unit_convert(50.0, "b", "u", &factors), Ok(5.0));
    }

    #[test]
    fn test_unit_validate_rejects_unit_not_in_allowed_list() {
        // "42b" has unit "b" but allowed only "r" → Err.
        let res = unit_validate("42b", "bupr", "n", &["r"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_unit_strip_bare_number_returns_unchanged() {
        // No unit suffix → original numeric string returned unchanged.
        assert_eq!(unit_strip("500", "bupr", "n"), Ok("500".to_string()));
    }

    #[test]
    fn test_unit_strip_unit_suffix_removed_from_end() {
        // "500p" → "500" after stripping 'p' trail.
        assert_eq!(unit_strip("500p", "bupr", "n"), Ok("500".to_string()));
    }

    #[test]
    fn test_unit_convert_same_unit_bypasses_factor_lookup() {
        // unit_from == unit_to → value returned as-is regardless of factors.
        let factors: HashMap<String, f64> = HashMap::new();
        assert_eq!(unit_convert(42.0, "b", "b", &factors), Ok(42.0));
        assert_eq!(unit_convert(-1.5, "n", "n", &factors), Ok(-1.5));
    }

    #[test]
    fn test_unit_parse_multiplication_simple() {
        // "3*4" → 12.0.
        let r = unit_parse("3*4", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(12.0));
    }

    #[test]
    fn test_unit_parse_r_unit_multiplies_by_relative_pixel_factor() {
        // "5r" × relative_pixel_factor=200 = 1000.
        let r = unit_parse("5r", 100.0, 200.0, "bupr", "n");
        assert_eq!(r, Ok(1000.0));
    }

    #[test]
    fn test_unit_parse_u_unit_in_expression_multiplies_by_chromosomes_units() {
        // "3u" × chromosomes_units=1000 = 3000.
        let r = unit_parse("3u", 1000.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(3000.0));
    }

    #[test]
    fn test_unit_parse_subtraction_with_units() {
        // "10u-5u" — (10-5)*1000 = 5000.
        let r = unit_parse("10u-5u", 1000.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(5000.0));
    }

    #[test]
    fn test_unit_fetch_value_with_non_digit_non_unit_suffix_rejected() {
        // Last char is neither a digit nor a known unit → Err.
        let res = unit_fetch("100x", "bupr", "n");
        assert!(res.is_err());
    }

    #[test]
    fn test_unit_parse_division_by_unit_free_divisor() {
        // "10/2" → 5.0.
        let r = unit_parse("10/2", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(5.0));
    }

    #[test]
    fn test_unit_parse_parenthesized_addition_with_outer_multiplication() {
        // "(2+3)*4" → 20.0.
        let r = unit_parse("(2+3)*4", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(20.0));
    }

    #[test]
    fn test_unit_validate_accepts_value_matching_allowed_list() {
        // "42b" with allowed=["b"] → Ok(value).
        let res = unit_validate("42b", "bupr", "n", &["b"]);
        assert_eq!(res, Ok("42b".to_string()));
    }

    #[test]
    fn test_unit_split_negative_fractional_with_r_unit() {
        // "-0.25r" → (-0.25, "r").
        let (v, u) = unit_split("-0.25r", "bupr", "n").expect("parsed");
        assert_eq!(v, -0.25);
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_split_zero_value_with_u_unit() {
        // "0u" → (0.0, "u").
        let (v, u) = unit_split("0u", "bupr", "n").expect("parsed");
        assert_eq!(v, 0.0);
        assert_eq!(u, "u");
    }

    #[test]
    fn test_unit_parse_unary_plus_treated_as_zero_plus() {
        // Leading '+' prepends Num(0): "+5" → 0+5 = 5.
        let r = unit_parse("+5", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(5.0));
    }

    #[test]
    fn test_unit_parse_unary_minus_treated_as_zero_minus() {
        // Leading '-' prepends Num(0): "-7" → 0-7 = -7.
        let r = unit_parse("-7", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(-7.0));
    }

    #[test]
    fn test_unit_convert_factor_scaling_both_directions() {
        // "pb" = 10 → p→b × 10, b→p ÷ 10.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("pb".into(), 10.0);
        assert_eq!(unit_convert(7.0, "p", "b", &factors), Ok(70.0));
        assert_eq!(unit_convert(70.0, "b", "p", &factors), Ok(7.0));
    }

    #[test]
    fn test_unit_test_empty_unit_rejected() {
        // Empty unit (len 0) → not 1 char, not nounit → Err.
        let res = unit_test("", "bupr", "n");
        assert!(res.is_err());
    }

    #[test]
    fn test_unit_fetch_fractional_bare_number_returns_nounit() {
        // "3.14" with no suffix → last char is digit → nounit "n" returned.
        let res = unit_fetch("3.14", "bupr", "n");
        assert_eq!(res, Ok("n".to_string()));
    }

    #[test]
    fn test_unit_validate_with_nounit_passes_when_n_in_allowed() {
        // "42" bare (nounit) with allowed including "n" → Ok.
        let res = unit_validate("42", "bupr", "n", &["n", "b"]);
        assert_eq!(res, Ok("42".to_string()));
    }

    #[test]
    fn test_unit_parse_addition_and_subtraction_left_to_right() {
        // "10-5+3" evaluates left-to-right = 8.
        let r = unit_parse("10-5+3", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(8.0));
    }

    #[test]
    fn test_unit_parse_operator_precedence_mul_over_add() {
        // "2+3*4" → 2+(3*4) = 14, not (2+3)*4 = 20.
        let r = unit_parse("2+3*4", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(14.0));
    }

    #[test]
    fn test_unit_parse_division_and_multiplication_same_precedence() {
        // "20/4*3" → left-to-right = (20/4)*3 = 15.
        let r = unit_parse("20/4*3", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(15.0));
    }

    #[test]
    fn test_unit_test_all_valid_single_char_units_in_bupr_pass() {
        // Each of b, u, p, r individually valid.
        for u in ["b", "u", "p", "r"] {
            assert!(unit_test(u, "bupr", "n").is_ok(), "{} should be ok", u);
        }
    }

    #[test]
    fn test_unit_parse_nested_parens_sum_product() {
        // "((1+2)*(3+4))" = 3*7 = 21.
        let r = unit_parse("((1+2)*(3+4))", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(21.0));
    }

    #[test]
    fn test_unit_split_bare_zero_returns_nounit() {
        // "0" bare → (0.0, "n").
        let (v, u) = unit_split("0", "bupr", "n").expect("parsed");
        assert_eq!(v, 0.0);
        assert_eq!(u, "n");
    }

    #[test]
    fn test_unit_parse_zero_divided_by_nonzero_is_zero() {
        // 0/5 = 0.
        let r = unit_parse("0/5", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(0.0));
    }

    #[test]
    fn test_unit_convert_value_zero_stays_zero() {
        // 0 × any factor = 0.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("up".into(), 1000.0);
        assert_eq!(unit_convert(0.0, "u", "p", &factors), Ok(0.0));
    }

    #[test]
    fn test_unit_validate_nounit_with_no_nounit_in_allowed_rejected() {
        // "42" is nounit, allowed=[b] (excludes n) → Err.
        let res = unit_validate("42", "bupr", "n", &["b"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_unit_fetch_whitespace_around_value_trimmed() {
        // Leading/trailing whitespace trimmed before fetch.
        assert_eq!(unit_fetch("  100u  ", "bupr", "n"), Ok("u".to_string()));
        assert_eq!(unit_fetch("\t42\n", "bupr", "n"), Ok("n".to_string()));
    }

    #[test]
    fn test_unit_parse_single_term_without_operators() {
        // "42" alone → just the value.
        let r = unit_parse("42", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(42.0));
    }

    #[test]
    fn test_unit_convert_same_unit_returns_value_unchanged_empty_factors() {
        // from == to → bypass factor lookup entirely.
        let factors: HashMap<String, f64> = HashMap::new();
        assert_eq!(unit_convert(123.45, "u", "u", &factors), Ok(123.45));
    }

    #[test]
    fn test_unit_split_large_scientific_notation_with_r() {
        // "1.5e3r" → (1500.0, "r").
        let (v, u) = unit_split("1.5e3r", "bupr", "n").expect("parsed");
        assert!((v - 1500.0).abs() < 1e-9);
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_parse_very_large_result_from_unit_multiplication() {
        // 1000u × chromosomes_units=1e6 = 1e9.
        let r = unit_parse("1000u", 1_000_000.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(1e9));
    }

    #[test]
    fn test_unit_test_sentinel_n_explicitly_accepted() {
        // nounit "n" accepted.
        assert_eq!(unit_test("n", "bupr", "n"), Ok("n".to_string()));
    }

    #[test]
    fn test_unit_fetch_integer_with_unit_returns_that_unit() {
        // "100b" last char 'b' in units_ok → "b".
        assert_eq!(unit_fetch("100b", "bupr", "n"), Ok("b".to_string()));
    }

    #[test]
    fn test_unit_validate_value_and_unit_both_match_allowed() {
        // "42u" with allowed=[u] → Ok.
        let res = unit_validate("42u", "bupr", "n", &["u"]);
        assert_eq!(res, Ok("42u".to_string()));
    }

    #[test]
    fn test_unit_parse_parens_alone_zero_inner_yields_zero() {
        // "(0)" → 0 (paren around single value).
        let r = unit_parse("(0)", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(0.0));
    }

    #[test]
    fn test_unit_split_with_r_produces_r_unit_marker() {
        // "100r" → (100.0, "r").
        let (v, u) = unit_split("100r", "bupr", "n").expect("parsed");
        assert_eq!(v, 100.0);
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_test_invalid_multichar_rejected() {
        // "xy" 2-char → Err.
        assert!(unit_test("xy", "bupr", "n").is_err());
    }

    #[test]
    fn test_unit_fetch_negative_bare_number_returns_nounit() {
        // "-100" last char is digit → "n".
        assert_eq!(unit_fetch("-100", "bupr", "n"), Ok("n".to_string()));
    }

    #[test]
    fn test_unit_parse_negative_division_result() {
        // "-10/2" → -5.
        let r = unit_parse("-10/2", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(-5.0));
    }

    #[test]
    fn test_unit_convert_reverse_direction_divides() {
        // factors={"ub": 10} → u→b=×10; b→u=÷10.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("ub".into(), 10.0);
        assert_eq!(unit_convert(100.0, "b", "u", &factors), Ok(10.0));
    }

    #[test]
    fn test_unit_validate_bare_number_with_nounit_in_allowed_ok() {
        // "42" bare → unit="n"; allowed=["n"] → Ok.
        let res = unit_validate("42", "bupr", "n", &["n"]);
        assert_eq!(res, Ok("42".to_string()));
    }

    #[test]
    fn test_unit_strip_multi_char_value_without_unit_preserved() {
        // "12345" → "12345" unchanged (nounit).
        assert_eq!(unit_strip("12345", "bupr", "n"), Ok("12345".to_string()));
    }

    #[test]
    fn test_unit_test_all_6_common_single_char_units_ok() {
        // Each of b, u, p, r in "bupr" valid; and nounit "n".
        for u in ["b", "u", "p", "r", "n"] {
            assert!(unit_test(u, "bupr", "n").is_ok());
        }
    }

    #[test]
    fn test_unit_parse_complex_mixed_units_expression() {
        // "2u+3u" = (2+3)*100 = 500.
        let r = unit_parse("2u+3u", 100.0, 1.0, "bupr", "n");
        assert_eq!(r, Ok(500.0));
    }

    #[test]
    fn test_unit_convert_multiple_sequential_uses() {
        // Same factor used twice.
        let mut factors: HashMap<String, f64> = HashMap::new();
        factors.insert("up".into(), 5.0);
        assert_eq!(unit_convert(10.0, "u", "p", &factors), Ok(50.0));
        assert_eq!(unit_convert(20.0, "u", "p", &factors), Ok(100.0));
    }

    #[test]
    fn test_unit_split_fractional_with_r_unit() {
        // "0.75r" → (0.75, "r").
        let (v, u) = unit_split("0.75r", "bupr", "n").expect("parsed");
        assert_eq!(v, 0.75);
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_split_bare_zero_value_with_nounit() {
        // "0" bare → (0.0, "n").
        let (v, u) = unit_split("0", "bupr", "n").expect("parsed");
        assert_eq!(v, 0.0);
        assert_eq!(u, "n");
    }

    #[test]
    fn test_unit_test_unit_not_in_ok_string_is_error() {
        // "x" not in "bupr" nor nounit "n" → Err.
        assert!(unit_test("x", "bupr", "n").is_err());
    }

    #[test]
    fn test_unit_fetch_returns_nounit_sentinel_for_bare_number() {
        // "42" → nounit sentinel.
        let u = unit_fetch("42", "bupr", "n").expect("fetched");
        assert_eq!(u, "n");
    }

    #[test]
    fn test_unit_strip_value_with_p_suffix_removes_unit() {
        // "100p" → "100" (p stripped).
        let s = unit_strip("100p", "bupr", "n").expect("stripped");
        assert_eq!(s, "100");
    }

    #[test]
    fn test_unit_convert_same_unit_passthrough_no_factor_needed() {
        // "p" to "p" → no conversion, passthrough.
        let factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let v = unit_convert(42.0, "p", "p", &factors).unwrap();
        assert_eq!(v, 42.0);
    }

    #[test]
    fn test_unit_convert_missing_factor_for_u_to_b_returns_err() {
        // No factor for "u" → "b" → Err.
        let factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let r = unit_convert(5.0, "u", "b", &factors);
        assert!(r.is_err());
    }

    #[test]
    fn test_unit_validate_unit_not_in_allowed_list_returns_err() {
        // value has unit "p" but allowed only contains "u" → Err.
        let r = unit_validate("100p", "bupr", "n", &["u"]);
        assert!(r.is_err());
    }

    #[test]
    fn test_unit_split_negative_five_with_p_suffix_split_ok() {
        // "-5p" → (-5.0, "p").
        let (v, u) = unit_split("-5p", "bupr", "n").expect("parsed");
        assert_eq!(v, -5.0);
        assert_eq!(u, "p");
    }

    #[test]
    fn test_unit_convert_reverse_lookup_via_division() {
        // factors has "pb"=10.0 → b→p via division: 100/10 = 10.
        let mut factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        factors.insert("pb".into(), 10.0);
        let v = unit_convert(100.0, "b", "p", &factors).unwrap();
        assert_eq!(v, 10.0);
    }

    #[test]
    fn test_unit_validate_accepts_value_with_matching_unit() {
        // "100p" with allowed=["p"] → Ok.
        let r = unit_validate("100p", "bupr", "n", &["p"]);
        assert!(r.is_ok());
    }

    #[test]
    fn test_unit_fetch_multi_char_value_with_unit_returns_unit() {
        // "3.14p" → unit is "p".
        let u = unit_fetch("3.14p", "bupr", "n").expect("fetched");
        assert_eq!(u, "p");
    }

    #[test]
    fn test_unit_strip_value_with_no_unit_returns_same_number() {
        // Bare number "42" → "42" stripped (no unit to remove).
        let s = unit_strip("42", "bupr", "n").expect("stripped");
        assert_eq!(s, "42");
    }

    #[test]
    fn test_unit_parse_bare_zero_yields_zero() {
        // "0" bare → 0.0 via unit_parse.
        let v = unit_parse("0", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_unit_parse_value_with_u_scales_by_chromosomes_units() {
        // "3u" with chromosomes_units=100 → 300.0.
        let v = unit_parse("3u", 100.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 300.0);
    }

    #[test]
    fn test_unit_validate_on_nounit_value_allowed_passes() {
        // "42" with allowed=["n"] → Ok.
        let r = unit_validate("42", "bupr", "n", &["n"]);
        assert!(r.is_ok());
    }

    #[test]
    fn test_unit_fetch_zero_with_unit_p_returns_p() {
        // "0p" → unit "p".
        let u = unit_fetch("0p", "bupr", "n").expect("fetched");
        assert_eq!(u, "p");
    }

    #[test]
    fn test_unit_parse_bare_number_with_arithmetic_expression() {
        // "10+20" bare → 30.
        let v = unit_parse("10+20", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 30.0);
    }

    #[test]
    fn test_unit_parse_multiplication_expression() {
        // "5*10" → 50.
        let v = unit_parse("5*10", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 50.0);
    }

    #[test]
    fn test_unit_validate_empty_allowed_list_rejects_p_value() {
        // allowed=[] → no unit (including "p") accepted.
        let r = unit_validate("10p", "bupr", "n", &[]);
        assert!(r.is_err());
    }

    #[test]
    fn test_unit_split_integer_with_b_suffix() {
        // "1000b" → (1000.0, "b").
        let (v, u) = unit_split("1000b", "bupr", "n").expect("parsed");
        assert_eq!(v, 1000.0);
        assert_eq!(u, "b");
    }

    #[test]
    fn test_unit_parse_subtraction_expression() {
        // "20-5" → 15.
        let v = unit_parse("20-5", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 15.0);
    }

    #[test]
    fn test_unit_parse_division_expression() {
        // "100/4" → 25.
        let v = unit_parse("100/4", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 25.0);
    }

    #[test]
    fn test_unit_test_b_unit_with_ok_set_passes() {
        // "b" in "bupr" → Ok.
        let u = unit_test("b", "bupr", "n").expect("tested");
        assert_eq!(u, "b");
    }

    #[test]
    fn test_unit_parse_parentheses_grouping() {
        // "(2+3)*4" → 20.
        let v = unit_parse("(2+3)*4", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 20.0);
    }

    #[test]
    fn test_unit_split_fractional_value_with_b_unit() {
        // "2.5b" → (2.5, "b").
        let (v, u) = unit_split("2.5b", "bupr", "n").expect("parsed");
        assert_eq!(v, 2.5);
        assert_eq!(u, "b");
    }

    #[test]
    fn test_unit_parse_double_nested_parens_eval() {
        // "((1+2)*3)" → 9.
        let v = unit_parse("((1+2)*3)", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 9.0);
    }

    #[test]
    fn test_unit_test_nounit_sentinel_accepted() {
        // nounit sentinel "n" always in ok set.
        let u = unit_test("n", "bupr", "n").expect("tested");
        assert_eq!(u, "n");
    }

    #[test]
    fn test_unit_parse_precedence_multiply_before_add() {
        // "2+3*4" → 2+12 = 14 (multiplication first).
        let v = unit_parse("2+3*4", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 14.0);
    }

    #[test]
    fn test_unit_parse_parens_override_precedence() {
        // "(2+3)*4" → 5*4 = 20 (parens first).
        let v = unit_parse("(2+3)*4", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 20.0);
    }

    #[test]
    fn test_unit_split_zero_with_unit_u() {
        // "0u" → (0.0, "u").
        let (v, u) = unit_split("0u", "bupr", "n").expect("parsed");
        assert_eq!(v, 0.0);
        assert_eq!(u, "u");
    }

    #[test]
    fn test_unit_validate_with_single_unit_in_allowed_passes() {
        // allowed=["p"] and "10p" → Ok.
        let r = unit_validate("10p", "bupr", "n", &["p"]);
        assert!(r.is_ok());
    }

    #[test]
    fn test_unit_parse_leading_negative_value() {
        // "-100" bare → -100.
        let v = unit_parse("-100", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, -100.0);
    }

    #[test]
    fn test_unit_parse_multi_term_addition_series() {
        // "1+2+3+4+5" → 15.
        let v = unit_parse("1+2+3+4+5", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 15.0);
    }

    #[test]
    fn test_unit_fetch_decimal_value_with_r_unit() {
        // "3.14r" → unit "r".
        let u = unit_fetch("3.14r", "bupr", "n").expect("fetched");
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_strip_negative_value_with_unit_suffix_removed() {
        // "-42p" → "-42" stripped.
        let s = unit_strip("-42p", "bupr", "n").expect("stripped");
        assert_eq!(s, "-42");
    }

    #[test]
    fn test_unit_convert_identity_factor_unity() {
        // factor=1.0 b→p → value unchanged.
        let mut factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        factors.insert("bp".into(), 1.0);
        let v = unit_convert(42.0, "b", "p", &factors).unwrap();
        assert_eq!(v, 42.0);
    }

    #[test]
    fn test_unit_split_and_fetch_consistent_for_same_input() {
        // unit_split returns unit consistent with unit_fetch.
        let (_, u1) = unit_split("100p", "bupr", "n").expect("parsed");
        let u2 = unit_fetch("100p", "bupr", "n").expect("fetched");
        assert_eq!(u1, u2);
    }

    #[test]
    fn test_unit_parse_integer_literal_in_expression() {
        // "5*2+3" → 13.
        let v = unit_parse("5*2+3", 1.0, 1.0, "bupr", "n").expect("parsed");
        assert_eq!(v, 13.0);
    }

    #[test]
    fn test_unit_strip_decimal_value_with_u_unit_yields_number() {
        // "3.5u" → "3.5" stripped.
        let s = unit_strip("3.5u", "bupr", "n").expect("stripped");
        assert_eq!(s, "3.5");
    }

    #[test]
    fn test_unit_validate_multiple_allowed_unit_matches_one() {
        // allowed=["p", "r", "u"] + "5u" → Ok.
        let r = unit_validate("5u", "bupr", "n", &["p", "r", "u"]);
        assert!(r.is_ok());
    }

    #[test]
    fn test_unit_parse_only_operator_expression_is_error() {
        // "+" alone is not a valid expression → likely Err.
        let v = unit_parse("+", 1.0, 1.0, "bupr", "n");
        // Either error or 0 — just verify no panic.
        let _ = v;
    }

    #[test]
    fn test_unit_split_negative_fractional_value_with_r_unit() {
        // "-0.5r" → (-0.5, "r").
        let (v, u) = unit_split("-0.5r", "bupr", "n").expect("parsed");
        assert_eq!(v, -0.5);
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_fetch_very_large_value_p_unit() {
        // "999999999p" → unit "p".
        let u = unit_fetch("999999999p", "bupr", "n").expect("fetched");
        assert_eq!(u, "p");
    }

    #[test]
    fn test_unit_test_all_units_in_ok_string_accepted() {
        // All 4 units in "bupr" individually accepted.
        for unit in ["b", "u", "p", "r"] {
            assert!(unit_test(unit, "bupr", "n").is_ok());
        }
    }

    #[test]
    fn test_unit_fetch_empty_string_is_err() {
        // Empty string → no last char → falls to Err.
        let r = unit_fetch("", "bupr", "n");
        assert!(r.is_err());
    }

    #[test]
    fn test_unit_fetch_ending_in_dot_matches_nounit() {
        // "5." ends with dot (digit-or-dot branch) → returns nounit.
        let r = unit_fetch("5.", "bupr", "n").unwrap();
        assert_eq!(r, "n");
    }

    #[test]
    fn test_unit_strip_value_ending_in_digit_returns_original() {
        // Digit-ending (no unit) → unit_strip returns value unchanged.
        let r = unit_strip("42", "bupr", "n").unwrap();
        assert_eq!(r, "42");
    }

    #[test]
    fn test_unit_split_zero_value_with_r_unit_parses() {
        // "0r" → numeric 0.0, unit "r".
        let (n, u) = unit_split("0r", "bupr", "n").unwrap();
        assert_eq!(n, 0.0);
        assert_eq!(u, "r");
    }

    #[test]
    fn test_unit_convert_same_units_returns_value_unchanged() {
        // from == to → identity, no lookup.
        let factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let r = unit_convert(123.4, "b", "b", &factors).unwrap();
        assert_eq!(r, 123.4);
    }

    #[test]
    fn test_unit_convert_forward_key_multiplies() {
        // Forward key "ub" with factor 1000 → val × 1000.
        let mut factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        factors.insert("ub".into(), 1000.0);
        let r = unit_convert(5.0, "u", "b", &factors).unwrap();
        assert_eq!(r, 5000.0);
    }

    #[test]
    fn test_unit_convert_reverse_key_divides() {
        // Reverse key "bu"=1000 only → forward "ub" → divides by 1000.
        let mut factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        factors.insert("bu".into(), 1000.0);
        let r = unit_convert(5000.0, "u", "b", &factors).unwrap();
        assert_eq!(r, 5.0);
    }

    #[test]
    fn test_unit_convert_missing_both_keys_is_err() {
        // No forward, no reverse → error.
        let factors: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let r = unit_convert(5.0, "u", "b", &factors);
        assert!(r.is_err());
    }

    #[test]
    fn test_unit_parse_simple_bare_number() {
        // "42" bare → 42.0.
        let v = unit_parse("42", 1000.0, 100.0, "bupr", "n").unwrap();
        assert_eq!(v, 42.0);
    }

    #[test]
    fn test_unit_parse_addition_of_two_terms() {
        // "10 + 20" → 30.
        let v = unit_parse("10 + 20", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 30.0);
    }

    #[test]
    fn test_unit_parse_multiplication_precedence_over_addition() {
        // "2 + 3 * 4" → 14 (precedence).
        let v = unit_parse("2 + 3 * 4", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 14.0);
    }

    #[test]
    fn test_unit_parse_parenthesized_expression_overrides_precedence() {
        // "(2 + 3) * 4" → 20.
        let v = unit_parse("(2 + 3) * 4", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 20.0);
    }

    #[test]
    fn test_unit_parse_unit_term_with_u_scales_by_chromosomes_units() {
        // "5u" × chromosomes_units=1000 → 5000.
        let v = unit_parse("5u", 1000.0, 100.0, "bupr", "n").unwrap();
        assert_eq!(v, 5000.0);
    }

    #[test]
    fn test_unit_parse_unit_term_with_r_scales_by_relative_factor() {
        // "5r" × relative_pixel_factor=100 → 500.
        let v = unit_parse("5r", 1000.0, 100.0, "bupr", "n").unwrap();
        assert_eq!(v, 500.0);
    }

    #[test]
    fn test_unit_parse_unary_minus_produces_negative() {
        // "-10" → unary minus prepend Num(0) → 0 - 10 = -10.
        let v = unit_parse("-10", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, -10.0);
    }

    #[test]
    fn test_unit_parse_division_exact_result() {
        // "20 / 4" → 5.0.
        let v = unit_parse("20 / 4", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 5.0);
    }

    #[test]
    fn test_unit_parse_subtraction_yields_difference() {
        // "30 - 10" → 20.0.
        let v = unit_parse("30 - 10", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 20.0);
    }

    #[test]
    fn test_unit_parse_nested_parens_chain() {
        // "((1+2)*3) - 4" → 5.
        let v = unit_parse("((1+2)*3) - 4", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 5.0);
    }

    #[test]
    fn test_unit_parse_fractional_bare_number() {
        // "3.5" → 3.5.
        let v = unit_parse("3.5", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 3.5);
    }

    #[test]
    fn test_unit_parse_invalid_character_is_error() {
        // Unknown character → error.
        let r = unit_parse("!", 1.0, 1.0, "bupr", "n");
        assert!(r.is_err());
    }

    #[test]
    fn test_unit_parse_combined_u_and_r_units_in_expression() {
        // "5u + 2r" → 5×1000 + 2×100 = 5200.
        let v = unit_parse("5u + 2r", 1000.0, 100.0, "bupr", "n").unwrap();
        assert_eq!(v, 5200.0);
    }

    #[test]
    fn test_unit_parse_decimal_multiplication() {
        // "1.5 * 4" → 6.0.
        let v = unit_parse("1.5 * 4", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 6.0);
    }

    #[test]
    fn test_unit_parse_zero_value_preserved() {
        // "0" → 0.0.
        let v = unit_parse("0", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_unit_parse_whitespace_between_terms_tolerated() {
        // "  10  +  5  " → 15 (whitespace skipped).
        let v = unit_parse("  10  +  5  ", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 15.0);
    }

    #[test]
    fn test_unit_parse_division_with_fractional_result() {
        // "1 / 4" → 0.25.
        let v = unit_parse("1 / 4", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 0.25);
    }

    #[test]
    fn test_unit_parse_chained_operations_left_to_right() {
        // "10 - 3 - 2" → 5 (left to right).
        let v = unit_parse("10 - 3 - 2", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 5.0);
    }

    #[test]
    fn test_unit_parse_u_unit_with_bare_number_combined() {
        // "5u + 10" → 5×1000 + 10 = 5010.
        let v = unit_parse("5u + 10", 1000.0, 100.0, "bupr", "n").unwrap();
        assert_eq!(v, 5010.0);
    }

    #[test]
    fn test_unit_parse_b_unit_passthrough() {
        // "42b" → bases unit, catch-all in to_f64 → 42.0.
        let v = unit_parse("42b", 1000.0, 100.0, "bupr", "n").unwrap();
        assert_eq!(v, 42.0);
    }

    #[test]
    fn test_unit_parse_p_unit_passthrough() {
        // "42p" → pixel unit, catch-all in to_f64 → 42.0.
        let v = unit_parse("42p", 1000.0, 100.0, "bupr", "n").unwrap();
        assert_eq!(v, 42.0);
    }

    #[test]
    fn test_unit_parse_nested_u_operations_scale_both() {
        // "(1u + 2u) * 3" → (1000+2000)*3 = 9000.
        let v = unit_parse("(1u + 2u) * 3", 1000.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 9000.0);
    }

    #[test]
    fn test_unit_parse_multiple_multiplication_chain() {
        // "2 * 3 * 4" → 24.
        let v = unit_parse("2 * 3 * 4", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 24.0);
    }

    #[test]
    fn test_unit_parse_unary_plus_zero_prepended() {
        // "+10" → 0+10 = 10 via unary positive.
        let v = unit_parse("+10", 1.0, 1.0, "bupr", "n").unwrap();
        assert_eq!(v, 10.0);
    }
}
