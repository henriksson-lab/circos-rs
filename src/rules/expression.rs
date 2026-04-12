use std::collections::HashMap;

use crate::data::types::Link;

/// Evaluate a condition expression against a link.
///
/// Supports variable substitution:
/// - `_CHR_`, `_CHR1_`, `_CHR2_`: chromosome names
/// - `_START_`, `_START1_`, `_START2_`: start positions
/// - `_END_`, `_END1_`, `_END2_`: end positions
/// - `_SIZE_`, `_SIZE1_`, `_SIZE2_`: span sizes (end - start + 1)
/// - `_INTERCHR_`: true if chromosomes differ
/// - `_INTRACHR_`: true if all chromosomes same
///
/// Supports operators: &&, ||, !, >, <, >=, <=, ==, !=, eq, ne
/// Supports functions: max(), min(), abs()
/// Supports unit suffixes: kb, Mb, Gb
pub fn evaluate_link_condition(condition: &str, link: &Link) -> bool {
    let substituted = substitute_link_vars(condition, link);
    let expanded = expand_units(&substituted);
    evaluate_expression(&expanded)
}

/// Evaluate a condition expression against a single datum's variables.
pub fn evaluate_condition(
    condition: &str,
    vars: &HashMap<String, String>,
) -> bool {
    let substituted = substitute_vars(condition, vars);
    let expanded = expand_units(&substituted);
    evaluate_expression(&expanded)
}

/// Substitute _VAR_ patterns with link data.
fn substitute_link_vars(condition: &str, link: &Link) -> String {
    let mut result = condition.to_string();

    // _INTERCHR_ and _INTRACHR_
    let all_same_chr = link
        .points
        .windows(2)
        .all(|w| w[0].chr == w[1].chr);
    result = result.replace("_INTERCHR_", if all_same_chr { "0" } else { "1" });
    result = result.replace("_INTRACHR_", if all_same_chr { "1" } else { "0" });

    // Indexed variables: _CHR1_, _START1_, _END1_, _SIZE1_, etc.
    for (i, point) in link.points.iter().enumerate() {
        let idx = i + 1; // 1-based indexing
        let size = point.end - point.start + 1;

        result = result.replace(
            &format!("_CHR{}_", idx),
            &format!("\"{}\"", point.chr),
        );
        result = result.replace(&format!("_START{}_", idx), &point.start.to_string());
        result = result.replace(&format!("_END{}_", idx), &point.end.to_string());
        result = result.replace(&format!("_SIZE{}_", idx), &size.to_string());
        result = result.replace(&format!("_POSITION{}_", idx), &((point.start + point.end) / 2).to_string());
    }

    // Non-indexed versions (first point)
    if let Some(p) = link.points.first() {
        result = result.replace("_CHR_", &format!("\"{}\"", p.chr));
        result = result.replace("_START_", &p.start.to_string());
        result = result.replace("_END_", &p.end.to_string());
        let size = p.end - p.start + 1;
        result = result.replace("_SIZE_", &size.to_string());
    }

    result
}

/// Substitute _VAR_ patterns with generic variable map.
fn substitute_vars(condition: &str, vars: &HashMap<String, String>) -> String {
    let mut result = condition.to_string();
    for (key, value) in vars {
        let pattern = format!("_{}_", key.to_uppercase());
        // Quote string values, leave numbers unquoted
        let replacement = if value.parse::<f64>().is_ok() {
            value.clone()
        } else {
            format!("\"{}\"", value)
        };
        result = result.replace(&pattern, &replacement);
    }
    result
}

/// Expand unit suffixes: 65Mb -> 65000000, 10kb -> 10000, 1Gb -> 1000000000.
fn expand_units(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Look for number followed by unit suffix
        if chars[i].is_ascii_digit() || chars[i] == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let num_str: String = chars[start..i].iter().collect();

            if i + 1 < chars.len() {
                let suffix: String = chars[i..i + 2].iter().collect();
                match suffix.as_str() {
                    "Mb" => {
                        if let Ok(n) = num_str.parse::<f64>() {
                            result.push_str(&((n * 1e6) as i64).to_string());
                            i += 2;
                            continue;
                        }
                    }
                    "kb" => {
                        if let Ok(n) = num_str.parse::<f64>() {
                            result.push_str(&((n * 1e3) as i64).to_string());
                            i += 2;
                            continue;
                        }
                    }
                    "Gb" => {
                        if let Ok(n) = num_str.parse::<f64>() {
                            result.push_str(&((n * 1e9) as i64).to_string());
                            i += 2;
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            result.push_str(&num_str);
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Evaluate a simple boolean expression.
///
/// Supports: &&, ||, !, >, <, >=, <=, ==, !=, eq, ne
/// Supports: max(a,b), min(a,b), abs(a)
/// Supports: parentheses for grouping
fn evaluate_expression(expr: &str) -> bool {
    let expr = expr.trim();
    if expr.is_empty() {
        return false;
    }

    // Evaluate function calls first
    let expr = evaluate_functions(expr);

    // Parse and evaluate the boolean expression
    let tokens = tokenize(&expr);
    eval_tokens(&tokens, 0).0
}

/// Evaluate function calls: max(a,b), min(a,b), abs(a).
fn evaluate_functions(expr: &str) -> String {
    let mut result = expr.to_string();

    // Process max(a,b)
    while let Some(start) = result.find("max(") {
        if let Some(end) = find_matching_paren(&result, start + 3) {
            let inner = &result[start + 4..end];
            if let Some((a, b)) = inner.split_once(',') {
                let va = parse_numeric(a.trim());
                let vb = parse_numeric(b.trim());
                let max_val = va.max(vb);
                result = format!("{}{}{}", &result[..start], max_val, &result[end + 1..]);
                continue;
            }
        }
        break;
    }

    // Process min(a,b)
    while let Some(start) = result.find("min(") {
        if let Some(end) = find_matching_paren(&result, start + 3) {
            let inner = &result[start + 4..end];
            if let Some((a, b)) = inner.split_once(',') {
                let va = parse_numeric(a.trim());
                let vb = parse_numeric(b.trim());
                let min_val = va.min(vb);
                result = format!("{}{}{}", &result[..start], min_val, &result[end + 1..]);
                continue;
            }
        }
        break;
    }

    // Process abs(a)
    while let Some(start) = result.find("abs(") {
        if let Some(end) = find_matching_paren(&result, start + 3) {
            let inner = &result[start + 4..end];
            let v = parse_numeric(inner.trim());
            result = format!("{}{}{}", &result[..start], v.abs(), &result[end + 1..]);
            continue;
        }
        break;
    }

    result
}

fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(open_pos) != Some(&b'(') {
        return None;
    }
    let mut depth = 1;
    for i in (open_pos + 1)..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_numeric(s: &str) -> f64 {
    s.trim().trim_matches('"').parse().unwrap_or(0.0)
}

#[derive(Debug, Clone)]
enum Token {
    Num(f64),
    Str(String),
    Op(String),
    LParen,
    RParen,
    #[allow(dead_code)]
    Bool(bool),
}

fn tokenize(expr: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' => i += 1,
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '&' if i + 1 < chars.len() && chars[i + 1] == '&' => {
                tokens.push(Token::Op("&&".to_string()));
                i += 2;
            }
            '|' if i + 1 < chars.len() && chars[i + 1] == '|' => {
                tokens.push(Token::Op("||".to_string()));
                i += 2;
            }
            '!' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op("!=".to_string()));
                i += 2;
            }
            '!' => {
                tokens.push(Token::Op("!".to_string()));
                i += 1;
            }
            '>' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op(">=".to_string()));
                i += 2;
            }
            '<' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op("<=".to_string()));
                i += 2;
            }
            '=' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op("==".to_string()));
                i += 2;
            }
            '>' => {
                tokens.push(Token::Op(">".to_string()));
                i += 1;
            }
            '<' => {
                tokens.push(Token::Op("<".to_string()));
                i += 1;
            }
            '"' => {
                // Quoted string
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Token::Str(s));
                if i < chars.len() {
                    i += 1; // skip closing quote
                }
            }
            c if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' => {
                let start = i;
                if c == '-' || c == '+' {
                    i += 1;
                }
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                if let Ok(n) = s.parse::<f64>() {
                    tokens.push(Token::Num(n));
                } else {
                    tokens.push(Token::Str(s));
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                match s.as_str() {
                    "eq" => tokens.push(Token::Op("eq".to_string())),
                    "ne" => tokens.push(Token::Op("ne".to_string())),
                    _ => {
                        if let Ok(n) = s.parse::<f64>() {
                            tokens.push(Token::Num(n));
                        } else {
                            tokens.push(Token::Str(s));
                        }
                    }
                }
            }
            _ => i += 1,
        }
    }

    tokens
}

/// Simple recursive descent evaluator for boolean expressions.
/// Returns (result, next_token_index).
fn eval_tokens(tokens: &[Token], pos: usize) -> (bool, usize) {
    eval_or(tokens, pos)
}

fn eval_or(tokens: &[Token], pos: usize) -> (bool, usize) {
    let (mut left, mut pos) = eval_and(tokens, pos);
    while pos < tokens.len() {
        if let Token::Op(ref op) = tokens[pos] {
            if op == "||" {
                let (right, next) = eval_and(tokens, pos + 1);
                left = left || right;
                pos = next;
                continue;
            }
        }
        break;
    }
    (left, pos)
}

fn eval_and(tokens: &[Token], pos: usize) -> (bool, usize) {
    let (mut left, mut pos) = eval_comparison(tokens, pos);
    while pos < tokens.len() {
        if let Token::Op(ref op) = tokens[pos] {
            if op == "&&" {
                let (right, next) = eval_comparison(tokens, pos + 1);
                left = left && right;
                pos = next;
                continue;
            }
        }
        break;
    }
    (left, pos)
}

fn eval_comparison(tokens: &[Token], pos: usize) -> (bool, usize) {
    if pos >= tokens.len() {
        return (false, pos);
    }

    // Handle NOT
    if let Token::Op(ref op) = tokens[pos] {
        if op == "!" {
            let (val, next) = eval_comparison(tokens, pos + 1);
            return (!val, next);
        }
    }

    // Handle parentheses
    if let Token::LParen = tokens[pos] {
        let (val, next) = eval_or(tokens, pos + 1);
        let next = if next < tokens.len() {
            if let Token::RParen = tokens[next] {
                next + 1
            } else {
                next
            }
        } else {
            next
        };
        return (val, next);
    }

    // Get left value
    let (left_val, pos) = get_value(tokens, pos);

    if pos >= tokens.len() {
        // Single value: truthy check
        return (is_truthy(&left_val), pos);
    }

    // Check for comparison operator
    if let Token::Op(ref op) = tokens[pos] {
        let op = op.clone();
        let (right_val, next) = get_value(tokens, pos + 1);

        let result = match op.as_str() {
            ">" => to_f64(&left_val) > to_f64(&right_val),
            "<" => to_f64(&left_val) < to_f64(&right_val),
            ">=" => to_f64(&left_val) >= to_f64(&right_val),
            "<=" => to_f64(&left_val) <= to_f64(&right_val),
            "==" => to_f64(&left_val) == to_f64(&right_val),
            "!=" => to_f64(&left_val) != to_f64(&right_val),
            "eq" => to_str(&left_val) == to_str(&right_val),
            "ne" => to_str(&left_val) != to_str(&right_val),
            _ => {
                // Not a comparison op, treat left as truthy
                return (is_truthy(&left_val), pos);
            }
        };
        return (result, next);
    }

    (is_truthy(&left_val), pos)
}

#[derive(Debug)]
enum Value {
    Num(f64),
    Str(String),
}

fn get_value(tokens: &[Token], pos: usize) -> (Value, usize) {
    if pos >= tokens.len() {
        return (Value::Num(0.0), pos);
    }
    match &tokens[pos] {
        Token::Num(n) => (Value::Num(*n), pos + 1),
        Token::Str(s) => (Value::Str(s.clone()), pos + 1),
        Token::Bool(b) => (Value::Num(if *b { 1.0 } else { 0.0 }), pos + 1),
        _ => (Value::Num(0.0), pos + 1),
    }
}

fn to_f64(v: &Value) -> f64 {
    match v {
        Value::Num(n) => *n,
        Value::Str(s) => s.parse().unwrap_or(0.0),
    }
}

fn to_str(v: &Value) -> String {
    match v {
        Value::Num(n) => n.to_string(),
        Value::Str(s) => s.clone(),
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Num(n) => *n != 0.0,
        Value::Str(s) => !s.is_empty() && s != "0",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::types::Datum;
    use crate::intspan::IntSpan;

    fn make_link(chr1: &str, s1: i64, e1: i64, chr2: &str, s2: i64, e2: i64) -> Link {
        Link {
            id: "test".to_string(),
            points: vec![
                Datum {
                    chr: chr1.to_string(),
                    start: s1,
                    end: e1,
                    set: IntSpan::from_range(s1, e1),
                    id: Some("test".to_string()),
                    value: None,
                    label: None,
                    param: HashMap::new(),
                },
                Datum {
                    chr: chr2.to_string(),
                    start: s2,
                    end: e2,
                    set: IntSpan::from_range(s2, e2),
                    id: Some("test".to_string()),
                    value: None,
                    label: None,
                    param: HashMap::new(),
                },
            ],
            param: HashMap::new(),
        }
    }

    #[test]
    fn test_interchr() {
        let link = make_link("hs1", 0, 100, "hs2", 0, 100);
        assert!(evaluate_link_condition("_INTERCHR_", &link));
        assert!(!evaluate_link_condition("_INTRACHR_", &link));

        let link = make_link("hs1", 0, 100, "hs1", 200, 300);
        assert!(!evaluate_link_condition("_INTERCHR_", &link));
        assert!(evaluate_link_condition("_INTRACHR_", &link));
    }

    #[test]
    fn test_size_comparison() {
        let link = make_link("hs1", 0, 50000, "hs2", 0, 100);
        // _SIZE1_ = 50001
        assert!(evaluate_link_condition("_SIZE1_ > 40000", &link));
        assert!(!evaluate_link_condition("_SIZE1_ > 60000", &link));
    }

    #[test]
    fn test_max_function() {
        let link = make_link("hs1", 0, 50000, "hs2", 0, 100);
        assert!(evaluate_link_condition("max(_SIZE1_,_SIZE2_) > 40000", &link));
        assert!(!evaluate_link_condition("max(_SIZE1_,_SIZE2_) > 60000", &link));
    }

    #[test]
    fn test_string_eq() {
        let link = make_link("hs2", 70000000, 71000000, "hs3", 0, 100);
        assert!(evaluate_link_condition("_CHR1_ eq \"hs2\"", &link));
        assert!(!evaluate_link_condition("_CHR1_ eq \"hs1\"", &link));
    }

    #[test]
    fn test_complex_condition() {
        let link = make_link("hs2", 70000000, 71000000, "hs3", 0, 100);
        let cond = "_INTERCHR_ && _CHR1_ eq \"hs2\" && _START1_ > 65Mb && _START1_ < 75Mb";
        assert!(evaluate_link_condition(cond, &link));
    }

    #[test]
    fn test_unit_expansion() {
        assert_eq!(expand_units("65Mb"), "65000000");
        assert_eq!(expand_units("10kb"), "10000");
        assert_eq!(expand_units("1.5Gb"), "1500000000");
    }

    #[test]
    fn test_and_or() {
        let link = make_link("hs1", 0, 100, "hs2", 0, 100);
        assert!(evaluate_link_condition("1 && 1", &link));
        assert!(!evaluate_link_condition("1 && 0", &link));
        assert!(evaluate_link_condition("0 || 1", &link));
        assert!(!evaluate_link_condition("0 || 0", &link));
    }

    #[test]
    fn test_parentheses() {
        let link = make_link("hs1", 0, 100, "hs2", 0, 100);
        assert!(evaluate_link_condition("(1 && 1) || 0", &link));
        assert!(!evaluate_link_condition("1 && (0 || 0)", &link));
    }
}
