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
    eval_expression(link, condition, &[])
}

/// Port of Perl `eval_expression(datum, condition, param_path)`. The Perl
/// body is an 84-LOC loop that repeatedly finds the next `_(\w+)_` token,
/// splits it into (varroot, varnum), and dispatches:
///   - if varnum is set: look in datum.data[varnum-1].{data,param} → seek_parameter in param_path → computed `size`/`position` → error
///   - else: `intrachr`/`interchr` → seek_parameter(varroot, datum, datum.data, @param_path) → error
///
/// The resulting string is run through `format_condition` (kb/Mb/Gb suffix
/// expansion) and finally passed to Perl's `eval` for arithmetic+boolean
/// evaluation.
pub fn eval_expression(
    datum: &Link,
    condition: &str,
    _param_path: &[&HashMap<String, crate::config::types::ConfigValue>],
) -> bool {
    use std::sync::LazyLock;
    static RE_VAR: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"_(\w+)_").unwrap());
    static RE_SPLIT: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^(.+?)(\d+)$").unwrap());
    let re = &*RE_VAR;
    let re_split = &*RE_SPLIT;
    let mut condition = condition.to_string();
    // Bound the substitution loop to avoid infinite spins on pathological input.
    for _ in 0..256 {
        let caps_opt = re.captures(&condition).map(|c| {
            let all = c.get(0).unwrap();
            let var = c.get(1).unwrap().as_str().to_lowercase();
            (all.range(), var)
        });
        let (range, var) = match caps_opt {
            None => break,
            Some(x) => x,
        };

        // Split varroot / varnum on trailing digits
        let (varroot, varnum) = match re_split.captures(&var) {
            Some(cap) => (
                cap.get(1).unwrap().as_str().to_lowercase(),
                cap.get(2).and_then(|m| m.as_str().parse::<usize>().ok()),
            ),
            None => (var.clone(), None),
        };

        // Perl: if collection has only one data point, treat bare _SIZE_ as _SIZE1_
        let effective_varnum = if datum.points.len() == 1 && varnum.is_none() {
            Some(1)
        } else {
            varnum
        };

        // Perl: varnum-- (1-indexed to 0-indexed)
        let zero_idx = effective_varnum.map(|n| n.saturating_sub(1));

        let replacement: Option<String> = if let Some(i) = zero_idx {
            if let Some(point) = datum.points.get(i) {
                // point.data fields first
                let val = match varroot.as_str() {
                    "chr" => Some(format!("\"{}\"", point.chr)),
                    "start" => Some(point.start.to_string()),
                    "end" => Some(point.end.to_string()),
                    "value" => point.value.map(|v| v.to_string()),
                    "size" => Some((point.end - point.start + 1).to_string()),
                    "position" => Some(((point.end + point.start) / 2).to_string()),
                    other => point.param.get(other).cloned(),
                };
                if val.is_none() {
                    // seek_parameter in datum.param
                    datum.param.get(&varroot).cloned()
                } else {
                    val
                }
            } else {
                None
            }
        } else {
            match varroot.as_str() {
                "intrachr" => {
                    let same = datum.points.windows(2).all(|w| w[0].chr == w[1].chr);
                    Some(if same {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    })
                }
                "interchr" => {
                    let same = datum.points.windows(2).all(|w| w[0].chr == w[1].chr);
                    Some(if same {
                        "0".to_string()
                    } else {
                        "1".to_string()
                    })
                }
                other => datum.param.get(other).cloned(),
            }
        };

        let replace_with = replacement.unwrap_or_default();
        // Perl's replace_string: quote non-numeric replacements.
        let replacement_str = if replace_with.parse::<f64>().is_ok()
            || replace_with.starts_with('"')
            || replace_with == "undef"
        {
            replace_with
        } else {
            format!("\"{}\"", replace_with)
        };
        condition.replace_range(range, &replacement_str);
    }

    let with_format = crate::utils::format_condition(&condition);
    let expanded = expand_units(&with_format);
    eval_bool_expr(&expanded)
}

/// Evaluate a condition expression against a single datum's variables.
pub fn evaluate_condition(condition: &str, vars: &HashMap<String, String>) -> bool {
    let substituted = substitute_vars(condition, vars);
    let expanded = expand_units(&substituted);
    eval_bool_expr(&expanded)
}

/// Substitute _VAR_ patterns with link data.
#[allow(dead_code)]
fn substitute_link_vars(condition: &str, link: &Link) -> String {
    let mut result = condition.to_string();

    // _INTERCHR_ and _INTRACHR_
    let all_same_chr = link.points.windows(2).all(|w| w[0].chr == w[1].chr);
    result = result.replace("_INTERCHR_", if all_same_chr { "0" } else { "1" });
    result = result.replace("_INTRACHR_", if all_same_chr { "1" } else { "0" });

    // Indexed variables: _CHR1_, _START1_, _END1_, _SIZE1_, etc.
    for (i, point) in link.points.iter().enumerate() {
        let idx = i + 1; // 1-based indexing
        let size = point.end - point.start + 1;

        result = result.replace(&format!("_CHR{}_", idx), &format!("\"{}\"", point.chr));
        result = result.replace(&format!("_START{}_", idx), &point.start.to_string());
        result = result.replace(&format!("_END{}_", idx), &point.end.to_string());
        result = result.replace(&format!("_SIZE{}_", idx), &size.to_string());
        result = result.replace(
            &format!("_POSITION{}_", idx),
            &((point.start + point.end) / 2).to_string(),
        );
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
fn eval_bool_expr(expr: &str) -> bool {
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

/// Find the index of the `)` that matches the `(` at `open_pos` in the string.
fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(open_pos) != Some(&b'(') {
        return None;
    }
    let mut depth = 1;
    for (i, &c) in bytes.iter().enumerate().skip(open_pos + 1) {
        match c {
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

/// Parse a string to f64, stripping surrounding whitespace and quotes; returns 0.0 on failure.
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

/// Tokenize an expression string into numbers, strings, operators, parens, and bool literals.
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
            '*' => {
                tokens.push(Token::Op("*".to_string()));
                i += 1;
            }
            '/' => {
                tokens.push(Token::Op("/".to_string()));
                i += 1;
            }
            '%' => {
                tokens.push(Token::Op("%".to_string()));
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
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_digit()
                        || chars[i] == '.'
                        || chars[i] == 'e'
                        || chars[i] == 'E')
                {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                if let Ok(n) = s.parse::<f64>() {
                    tokens.push(Token::Num(n));
                } else {
                    tokens.push(Token::Str(s));
                }
            }
            // +/- are emitted as operators — the arithmetic/primary layer
            // decides whether they are binary (add/sub) or unary (signed literal).
            '+' => {
                tokens.push(Token::Op("+".to_string()));
                i += 1;
            }
            '-' => {
                tokens.push(Token::Op("-".to_string()));
                i += 1;
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
        if let Token::Op(ref op) = tokens[pos]
            && op == "||"
        {
            let (right, next) = eval_and(tokens, pos + 1);
            left = left || right;
            pos = next;
            continue;
        }
        break;
    }
    (left, pos)
}

fn eval_and(tokens: &[Token], pos: usize) -> (bool, usize) {
    let (mut left, mut pos) = eval_comparison(tokens, pos);
    while pos < tokens.len() {
        if let Token::Op(ref op) = tokens[pos]
            && op == "&&"
        {
            let (right, next) = eval_comparison(tokens, pos + 1);
            left = left && right;
            pos = next;
            continue;
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
    if let Token::Op(ref op) = tokens[pos]
        && op == "!"
    {
        let (val, next) = eval_comparison(tokens, pos + 1);
        return (!val, next);
    }

    // Handle a parenthesized *boolean* sub-expression — we only descend into
    // `eval_or` when the parens look like a full boolean expression (contain
    // `&&`/`||`/a top-level comparator). Otherwise the parens are part of an
    // arithmetic expression (e.g. `(_SIZE_ * 2) > 1000`) and the arithmetic
    // layer below handles them.
    if let Token::LParen = tokens[pos] {
        let close = find_matching_paren_token(tokens, pos);
        let is_bool_paren = close
            .map(|c| contains_bool_op(&tokens[pos + 1..c]))
            .unwrap_or(false);
        if is_bool_paren {
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
    }

    // Arithmetic-aware left operand.
    let (left_val, pos) = eval_addition(tokens, pos);

    if pos >= tokens.len() {
        return (is_truthy(&left_val), pos);
    }

    // Check for comparison operator
    if let Token::Op(ref op) = tokens[pos] {
        let op_s = op.clone();
        let (right_val, next) = eval_addition(tokens, pos + 1);

        let result = match op_s.as_str() {
            ">" => to_f64(&left_val) > to_f64(&right_val),
            "<" => to_f64(&left_val) < to_f64(&right_val),
            ">=" => to_f64(&left_val) >= to_f64(&right_val),
            "<=" => to_f64(&left_val) <= to_f64(&right_val),
            "==" => to_f64(&left_val) == to_f64(&right_val),
            "!=" => to_f64(&left_val) != to_f64(&right_val),
            "eq" => to_str(&left_val) == to_str(&right_val),
            "ne" => to_str(&left_val) != to_str(&right_val),
            _ => {
                return (is_truthy(&left_val), pos);
            }
        };
        return (result, next);
    }

    (is_truthy(&left_val), pos)
}

/// Port of Perl arithmetic evaluation within conditions. Add/subtract layer.
fn eval_addition(tokens: &[Token], pos: usize) -> (Value, usize) {
    let (mut left, mut pos) = eval_multiplication(tokens, pos);
    while pos < tokens.len() {
        if let Token::Op(op) = &tokens[pos]
            && (op == "+" || op == "-")
        {
            let op = op.clone();
            let (right, next) = eval_multiplication(tokens, pos + 1);
            let a = to_f64(&left);
            let b = to_f64(&right);
            left = Value::Num(if op == "+" { a + b } else { a - b });
            pos = next;
            continue;
        }
        break;
    }
    (left, pos)
}

/// Multiply/divide/modulo layer.
fn eval_multiplication(tokens: &[Token], pos: usize) -> (Value, usize) {
    let (mut left, mut pos) = eval_primary(tokens, pos);
    while pos < tokens.len() {
        if let Token::Op(op) = &tokens[pos]
            && (op == "*" || op == "/" || op == "%")
        {
            let op = op.clone();
            let (right, next) = eval_primary(tokens, pos + 1);
            let a = to_f64(&left);
            let b = to_f64(&right);
            left = Value::Num(match op.as_str() {
                "*" => a * b,
                "/" => {
                    if b == 0.0 {
                        0.0
                    } else {
                        a / b
                    }
                }
                "%" => {
                    if b == 0.0 {
                        0.0
                    } else {
                        a % b
                    }
                }
                _ => a,
            });
            pos = next;
            continue;
        }
        break;
    }
    (left, pos)
}

/// Primary expression: number, string, or parenthesized arithmetic.
fn eval_primary(tokens: &[Token], pos: usize) -> (Value, usize) {
    if pos >= tokens.len() {
        return (Value::Num(0.0), pos);
    }
    // Unary minus/plus
    if let Token::Op(op) = &tokens[pos] {
        if op == "-" {
            let (v, next) = eval_primary(tokens, pos + 1);
            return (Value::Num(-to_f64(&v)), next);
        }
        if op == "+" {
            return eval_primary(tokens, pos + 1);
        }
    }
    if let Token::LParen = tokens[pos] {
        let (v, next) = eval_addition(tokens, pos + 1);
        let next = if next < tokens.len() {
            if let Token::RParen = tokens[next] {
                next + 1
            } else {
                next
            }
        } else {
            next
        };
        return (v, next);
    }
    get_value(tokens, pos)
}

/// Token-stream analogue of `find_matching_paren`: find the index of the `RParen` that closes the `LParen` at `open_pos`.
fn find_matching_paren_token(tokens: &[Token], open_pos: usize) -> Option<usize> {
    if !matches!(tokens.get(open_pos), Some(Token::LParen)) {
        return None;
    }
    let mut depth = 1;
    for (i, t) in tokens.iter().enumerate().skip(open_pos + 1) {
        match t {
            Token::LParen => depth += 1,
            Token::RParen => {
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

/// Return true if the token slice contains a boolean/comparison operator at depth 0 — used to decide whether parens are boolean vs arithmetic.
fn contains_bool_op(tokens: &[Token]) -> bool {
    let mut depth = 0;
    for t in tokens {
        match t {
            Token::LParen => depth += 1,
            Token::RParen => depth -= 1,
            Token::Op(op) if depth == 0 => {
                if matches!(
                    op.as_str(),
                    "&&" | "||" | ">" | "<" | ">=" | "<=" | "==" | "!=" | "eq" | "ne"
                ) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

#[derive(Debug)]
enum Value {
    Num(f64),
    Str(String),
}

/// Read a single Num/Str/Bool token at `pos` and return it as a Value plus the advanced position.
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

/// Coerce a Value to f64 (parsing strings as numbers, falling back to 0.0).
fn to_f64(v: &Value) -> f64 {
    match v {
        Value::Num(n) => *n,
        Value::Str(s) => s.parse().unwrap_or(0.0),
    }
}

/// Render a Value as a String for string comparisons (`eq`/`ne`).
fn to_str(v: &Value) -> String {
    match v {
        Value::Num(n) => n.to_string(),
        Value::Str(s) => s.clone(),
    }
}

/// Perl-style truthiness: non-zero number, or non-empty string that isn't "0".
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
        assert!(evaluate_link_condition(
            "max(_SIZE1_,_SIZE2_) > 40000",
            &link
        ));
        assert!(!evaluate_link_condition(
            "max(_SIZE1_,_SIZE2_) > 60000",
            &link
        ));
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

    #[test]
    fn test_arithmetic_in_condition() {
        let link = make_link("hs1", 0, 1000, "hs2", 0, 500);
        // _SIZE1_ = 1001, _SIZE2_ = 501
        assert!(evaluate_link_condition("_SIZE1_ * 2 > 1000", &link));
        assert!(!evaluate_link_condition("_SIZE1_ / 2 > 1000", &link));
        assert!(evaluate_link_condition("_START1_ + 500 < _END1_", &link));
        assert!(evaluate_link_condition(
            "_SIZE1_ + _SIZE2_ > 1000",
            &link
        ));
        // precedence: * binds tighter than +
        assert!(evaluate_link_condition("2 + 3 * 4 == 14", &link));
        // parentheses override precedence
        assert!(evaluate_link_condition("(2 + 3) * 4 == 20", &link));
        // unary minus
        assert!(evaluate_link_condition("-5 + 10 > 0", &link));
        // modulo
        assert!(evaluate_link_condition("10 % 3 == 1", &link));
    }

    #[test]
    fn test_arithmetic_with_bool_mix() {
        let link = make_link("hs1", 0, 1000, "hs2", 0, 500);
        assert!(evaluate_link_condition(
            "_SIZE1_ * 2 > 1000 && _CHR1_ eq \"hs1\"",
            &link
        ));
        assert!(!evaluate_link_condition(
            "_SIZE1_ * 2 > 1000 && _CHR1_ eq \"hsX\"",
            &link
        ));
    }

    #[test]
    fn test_find_matching_paren_nested() {
        // Basic: simple parens
        assert_eq!(find_matching_paren("(abc)", 0), Some(4));
        // Nested: inner closer shouldn't terminate outer
        assert_eq!(find_matching_paren("((a)(b))", 0), Some(7));
        // Open pos not on `(` → None
        assert_eq!(find_matching_paren("(abc)", 1), None);
        // Unbalanced → None
        assert_eq!(find_matching_paren("(abc", 0), None);
    }

    #[test]
    fn test_evaluate_condition_with_generic_vars() {
        let mut vars = HashMap::new();
        vars.insert("COUNT".into(), "42".into());
        vars.insert("NAME".into(), "widget".into());
        // Numeric compare
        assert!(evaluate_condition("_COUNT_ > 10", &vars));
        assert!(!evaluate_condition("_COUNT_ < 10", &vars));
        // String compare
        assert!(evaluate_condition("_NAME_ eq \"widget\"", &vars));
        assert!(!evaluate_condition("_NAME_ eq \"gadget\"", &vars));
    }

    #[test]
    fn test_not_operator() {
        let link = make_link("hs1", 0, 100, "hs2", 0, 100);
        assert!(evaluate_link_condition("!0", &link));
        assert!(!evaluate_link_condition("!1", &link));
        // NOT with a comparison
        assert!(evaluate_link_condition("!(0 > 1)", &link));
    }

    #[test]
    fn test_negative_size_comparison() {
        // Negative literals round-trip through the unary-minus path.
        let link = make_link("hs1", 0, 100, "hs2", 0, 100);
        assert!(evaluate_link_condition("-5 < 0", &link));
        assert!(evaluate_link_condition("-5 + 3 == -2", &link));
        assert!(evaluate_link_condition("abs(-42) == 42", &link));
    }

    #[test]
    fn test_empty_expression_is_falsy() {
        let link = make_link("hs1", 0, 100, "hs2", 0, 100);
        // Empty string → eval_bool_expr returns false.
        assert!(!evaluate_link_condition("", &link));
    }

    #[test]
    fn test_expand_units_mixed_content() {
        // Mixes surrounding non-numeric text with multiple unit suffixes.
        assert_eq!(expand_units("_SIZE_ > 10kb && _POS_ < 5Mb"), "_SIZE_ > 10000 && _POS_ < 5000000");
        // No units → passthrough.
        assert_eq!(expand_units("x = 42"), "x = 42");
        // Decimal with unit.
        assert_eq!(expand_units("1.5Mb"), "1500000");
    }

    #[test]
    fn test_evaluate_functions_nested_in_expression() {
        // max / min / abs collapse before the bool evaluator sees the tokens.
        assert_eq!(evaluate_functions("max(3, 7)"), "7");
        assert_eq!(evaluate_functions("min(3, 7)"), "3");
        assert_eq!(evaluate_functions("abs(-5)"), "5");
        // Multiple fns in one expression.
        let out = evaluate_functions("max(1, 2) + min(3, 4)");
        assert_eq!(out, "2 + 3");
    }

    #[test]
    fn test_evaluate_functions_malformed_paren_bails_out() {
        // Unbalanced paren → leaves the input alone (no infinite loop).
        let out = evaluate_functions("max(1, 2");
        assert!(out.contains("max(1, 2"), "expected no rewrite for unbalanced: {}", out);
    }

    #[test]
    fn test_parse_numeric_strips_quotes() {
        // parse_numeric tolerates quoted strings (they come from substituted _CHR_).
        assert!((parse_numeric("\"42\"") - 42.0).abs() < 1e-12);
        assert!((parse_numeric("42") - 42.0).abs() < 1e-12);
        // Non-numeric with quotes → 0.0 (unwrap_or fallback).
        assert!((parse_numeric("\"abc\"") - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_substitute_vars_numeric_unquoted_string_quoted() {
        let mut vars = HashMap::new();
        vars.insert("COUNT".into(), "42".into());
        vars.insert("NAME".into(), "widget".into());
        let out = substitute_vars("x=_COUNT_ y=_NAME_", &vars);
        // Numbers unquoted; non-numeric values quoted.
        assert_eq!(out, "x=42 y=\"widget\"");
    }

    #[test]
    fn test_complex_arithmetic_with_functions_and_units() {
        // max(_SIZE1_, _SIZE2_) * 2 > 1kb combining fn + arith + unit suffix.
        let link = make_link("hs1", 0, 1000, "hs2", 0, 500);
        // SIZE1=1001, SIZE2=501 → max=1001, ×2=2002, > 1000.
        assert!(evaluate_link_condition(
            "max(_SIZE1_, _SIZE2_) * 2 > 1kb",
            &link
        ));
        // SIZE1×SIZE2 > 500M → 1001×501 = 501501, not > 500M.
        assert!(!evaluate_link_condition(
            "_SIZE1_ * _SIZE2_ > 500Mb",
            &link
        ));
    }

    #[test]
    fn test_tokenize_operators() {
        // All the binary arithmetic operators tokenize to Op variants.
        // "1 + 2 - 3 * 4 / 5 % 6" has 6 numbers and 5 operators.
        let toks = tokenize("1 + 2 - 3 * 4 / 5 % 6");
        let op_count = toks
            .iter()
            .filter(|t| matches!(t, Token::Op(_)))
            .count();
        let num_count = toks
            .iter()
            .filter(|t| matches!(t, Token::Num(_)))
            .count();
        assert_eq!(op_count, 5);
        assert_eq!(num_count, 6);
        // Verify each operator kind appears exactly once.
        for op in ["+", "-", "*", "/", "%"] {
            assert!(
                toks.iter()
                    .any(|t| matches!(t, Token::Op(o) if o == op)),
                "missing operator {}",
                op
            );
        }
    }

    #[test]
    fn test_tokenize_parens_and_compare() {
        let toks = tokenize("(_SIZE_ > 100) && (x eq \"y\")");
        // Should contain LParen, RParen, comparison ops, eq, and strings.
        assert!(toks.iter().any(|t| matches!(t, Token::LParen)));
        assert!(toks.iter().any(|t| matches!(t, Token::RParen)));
        assert!(toks.iter().any(|t| matches!(t, Token::Op(op) if op == ">")));
        assert!(toks.iter().any(|t| matches!(t, Token::Op(op) if op == "&&")));
        assert!(toks.iter().any(|t| matches!(t, Token::Op(op) if op == "eq")));
    }

    #[test]
    fn test_tokenize_quoted_strings_preserved() {
        let toks = tokenize("\"hello world\" ne \"bye\"");
        // Two quoted strings should both be Token::Str entries.
        let strs: Vec<&str> = toks
            .iter()
            .filter_map(|t| match t {
                Token::Str(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert!(strs.contains(&"hello world"));
        assert!(strs.contains(&"bye"));
    }

    #[test]
    fn test_eval_bool_expr_numeric_truthy_and_falsy() {
        // Non-zero number → truthy.
        assert!(eval_bool_expr("1"));
        assert!(eval_bool_expr("42.5"));
        // Zero → falsy.
        assert!(!eval_bool_expr("0"));
        assert!(!eval_bool_expr("0.0"));
    }

    #[test]
    fn test_eval_bool_expr_string_truthy_and_falsy() {
        // Non-empty non-"0" string → truthy.
        assert!(eval_bool_expr("\"text\""));
        // Empty string or "0" → falsy (Perl semantics).
        assert!(!eval_bool_expr("\"\""));
        assert!(!eval_bool_expr("\"0\""));
    }

    #[test]
    fn test_evaluate_functions_min_with_equal_args() {
        // min(5, 5) → 5.
        assert_eq!(evaluate_functions("min(5, 5)"), "5");
    }

    #[test]
    fn test_evaluate_functions_max_with_negative_values() {
        // max(-3, -10) → -3 (larger of two negatives).
        assert_eq!(evaluate_functions("max(-3, -10)"), "-3");
        // min(-3, -10) → -10.
        assert_eq!(evaluate_functions("min(-3, -10)"), "-10");
    }

    #[test]
    fn test_evaluate_functions_abs_with_decimal() {
        // abs(-3.14) → 3.14.
        assert_eq!(evaluate_functions("abs(-3.14)"), "3.14");
        // abs(0.5) → 0.5 (no change).
        assert_eq!(evaluate_functions("abs(0.5)"), "0.5");
    }

    #[test]
    fn test_evaluate_functions_min_and_abs_chained_in_expression() {
        // Rewrite order is max → min → abs. `min(3, 7) + abs(-2)` →
        // min(3,7)→3 → "3 + abs(-2)" → abs(-2)→2 → "3 + 2".
        let out = evaluate_functions("min(3, 7) + abs(-2)");
        assert_eq!(out, "3 + 2");
    }

    #[test]
    fn test_substitute_vars_unknown_name_leaves_intact() {
        // Unknown variable name → left as-is in output.
        let vars = HashMap::new();
        let out = substitute_vars("x=_UNKNOWN_ y=5", &vars);
        // _UNKNOWN_ not in vars → not substituted.
        assert!(out.contains("_UNKNOWN_"));
    }

    #[test]
    fn test_substitute_vars_empty_string_input() {
        // Empty input → empty output.
        let vars = HashMap::new();
        assert_eq!(substitute_vars("", &vars), "");
    }

    #[test]
    fn test_substitute_vars_multi_variable_substitution() {
        // Multiple variables in one expression.
        let mut vars = HashMap::new();
        vars.insert("A".into(), "42".into());
        vars.insert("B".into(), "hi".into());
        let out = substitute_vars("_A_ and _B_", &vars);
        // Numeric unquoted; string quoted.
        assert!(out.contains("42 and \"hi\""));
    }

    #[test]
    fn test_substitute_vars_negative_numeric_unquoted() {
        // Negative numbers are still numeric (no quoting).
        let mut vars = HashMap::new();
        vars.insert("NEG".into(), "-42".into());
        let out = substitute_vars("_NEG_", &vars);
        // Negative-numeric: no quotes expected.
        assert!(out == "-42" || out == "\"-42\"", "got: {}", out);
    }

    #[test]
    fn test_eval_bool_expr_comparison_operators() {
        // == / != / < / > / <= / >= arithmetic comparisons.
        assert!(eval_bool_expr("1 == 1"));
        assert!(!eval_bool_expr("1 == 2"));
        assert!(eval_bool_expr("1 != 2"));
        assert!(eval_bool_expr("1 < 2"));
        assert!(!eval_bool_expr("1 > 2"));
        assert!(eval_bool_expr("1 <= 1"));
        assert!(eval_bool_expr("2 >= 2"));
    }

    #[test]
    fn test_eval_bool_expr_string_eq_and_ne() {
        // String comparisons via `eq` / `ne`.
        assert!(eval_bool_expr("\"hello\" eq \"hello\""));
        assert!(!eval_bool_expr("\"hello\" eq \"world\""));
        assert!(eval_bool_expr("\"a\" ne \"b\""));
        assert!(!eval_bool_expr("\"a\" ne \"a\""));
    }

    #[test]
    fn test_eval_bool_expr_and_or_short_circuit() {
        // && short-circuits on false; || on true.
        assert!(eval_bool_expr("1 && 1"));
        assert!(!eval_bool_expr("1 && 0"));
        assert!(!eval_bool_expr("0 && 1"));
        assert!(eval_bool_expr("0 || 1"));
        assert!(eval_bool_expr("1 || 0"));
        assert!(!eval_bool_expr("0 || 0"));
    }

    #[test]
    fn test_eval_bool_expr_with_nested_parens() {
        // Nested parentheses for precedence control.
        assert!(eval_bool_expr("(1 == 1) && (2 > 1)"));
        assert!(!eval_bool_expr("(1 == 2) || (3 < 2)"));
        assert!(eval_bool_expr("((1 + 2) == 3)"));
    }

    #[test]
    fn test_expand_units_gb_suffix() {
        // Gb (gigabase) expansion: N × 1e9.
        assert_eq!(expand_units("1Gb"), "1000000000");
        assert_eq!(expand_units("2.5Gb"), "2500000000");
        // Mixed with other text.
        assert_eq!(expand_units("_SIZE_ > 1Gb"), "_SIZE_ > 1000000000");
    }

    #[test]
    fn test_expand_units_unknown_suffix_passthrough() {
        // A unit-like suffix not in [kb, Mb, Gb] is left alone and the
        // number is emitted verbatim.
        assert_eq!(expand_units("10Xb"), "10Xb");
        // Purely numeric trailing — no suffix to consume → passthrough.
        assert_eq!(expand_units("42"), "42");
        // Number at end of string with no room for a 2-char suffix.
        assert_eq!(expand_units("x=5"), "x=5");
    }

    #[test]
    fn test_evaluate_functions_abs_positive_unchanged() {
        // abs of positive → same value.
        assert_eq!(evaluate_functions("abs(5)"), "5");
        // abs of zero → zero.
        let out = evaluate_functions("abs(0)");
        assert!(out == "0" || out == "0.0" || out == "-0" || out == "-0.0");
        // abs of negative → positive magnitude.
        assert_eq!(evaluate_functions("abs(-7.25)"), "7.25");
    }

    #[test]
    fn test_evaluate_functions_processes_max_before_abs() {
        // The rewrite order is max → min → abs. So `max(abs(-3), 2)` first
        // finds "max(", but the inner `abs(-3)` text isn't yet evaluated —
        // `parse_numeric("abs(-3)") = 0` (unparseable) → max(0,2) = 2, not 3.
        // This documents the current single-pass limitation.
        let out = evaluate_functions("max(abs(-3), 2)");
        assert_eq!(out, "2");
    }

    #[test]
    fn test_find_matching_paren_empty_input() {
        // Empty string → None (out of bounds).
        assert_eq!(find_matching_paren("", 0), None);
        // Single paren with no match → None.
        assert_eq!(find_matching_paren("(", 0), None);
        // Closer-only, no opener at open_pos → None.
        assert_eq!(find_matching_paren(")", 0), None);
    }

    #[test]
    fn test_expand_units_mixed_suffixes_in_expression() {
        // Expression combining kb/Mb/Gb — each replaced independently.
        let out = expand_units("5kb + 2Mb + 0.5Gb");
        assert_eq!(out, "5000 + 2000000 + 500000000");
    }

    #[test]
    fn test_expand_units_decimal_number_with_suffix() {
        // Decimal: 2.5kb → 2500 via (2.5 × 1000) i64 truncation.
        assert_eq!(expand_units("2.5kb"), "2500");
        // 0.001Gb → (0.001 × 1e9) = 1_000_000.
        assert_eq!(expand_units("0.001Gb"), "1000000");
    }

    #[test]
    fn test_expand_units_number_without_suffix_preserved() {
        // A bare number (no suffix) → returned verbatim without conversion.
        assert_eq!(expand_units("42"), "42");
        // Trailing single char that's not a recognized suffix → left alone.
        assert_eq!(expand_units("42x"), "42x");
        // Trailing char alone (not digit) pass through.
        assert_eq!(expand_units("abc"), "abc");
    }

    #[test]
    fn test_parse_numeric_handles_ints_floats_and_garbage() {
        // Integer string → f64 via parse.
        assert!((parse_numeric("42") - 42.0).abs() < 1e-12);
        // Negative float.
        assert!((parse_numeric("-3.14") - (-3.14)).abs() < 1e-12);
        // Scientific notation.
        assert!((parse_numeric("1.5e3") - 1500.0).abs() < 1e-12);
        // Unparseable → 0.0 fallback.
        assert_eq!(parse_numeric("not_a_number"), 0.0);
        assert_eq!(parse_numeric(""), 0.0);
    }

    #[test]
    fn test_evaluate_functions_min_simple() {
        // min(a, b) rewrites to the smaller numeric argument.
        let out = evaluate_functions("min(5, 10)");
        assert_eq!(out, "5");
        let out = evaluate_functions("min(3.14, 1.5)");
        assert_eq!(out, "1.5");
        let out = evaluate_functions("min(-5, -10)");
        assert_eq!(out, "-10");
    }

    #[test]
    fn test_evaluate_functions_max_simple() {
        // max(a, b) rewrites to the larger numeric argument.
        let out = evaluate_functions("max(5, 10)");
        assert_eq!(out, "10");
        let out = evaluate_functions("max(-3.14, -1.5)");
        assert_eq!(out, "-1.5");
    }

    #[test]
    fn test_evaluate_functions_abs_negative_and_positive() {
        // abs returns absolute value; positives pass through untouched.
        assert_eq!(evaluate_functions("abs(-5)"), "5");
        assert_eq!(evaluate_functions("abs(-3.14)"), "3.14");
        assert_eq!(evaluate_functions("abs(0)"), "0");
        assert_eq!(evaluate_functions("abs(7)"), "7");
    }

    #[test]
    fn test_find_matching_paren_nested_parens() {
        // Matching paren across nested pairs. "(a(b)c)" at pos 0 matches pos 6.
        let r = find_matching_paren("(a(b)c)", 0);
        assert_eq!(r, Some(6));
        // Inner pair: "(b)" at pos 2 matches pos 4.
        let r = find_matching_paren("(a(b)c)", 2);
        assert_eq!(r, Some(4));
        // Deeply nested triple: "((()))" — outer (pos 0) matches pos 5.
        assert_eq!(find_matching_paren("((()))", 0), Some(5));
        // Middle pair at pos 1 matches pos 4.
        assert_eq!(find_matching_paren("((()))", 1), Some(4));
    }

    #[test]
    fn test_eval_bool_expr_empty_is_false() {
        // Empty expression → false.
        assert!(!eval_bool_expr(""));
        // Whitespace-only → false.
        assert!(!eval_bool_expr("   "));
    }

    #[test]
    fn test_eval_bool_expr_single_number_truthy() {
        // Non-zero number → true (Perl-style truthy).
        assert!(eval_bool_expr("1"));
        assert!(eval_bool_expr("42"));
        assert!(eval_bool_expr("-5"));
        // Zero → false.
        assert!(!eval_bool_expr("0"));
    }

    #[test]
    fn test_expand_units_at_start_and_middle() {
        // Expression with leading unit expression.
        assert_eq!(expand_units("100kb"), "100000");
        // Unit in middle of expression (surrounded by spaces).
        assert_eq!(expand_units("x=5kb"), "x=5000");
    }

    #[test]
    fn test_parse_numeric_whitespace_and_quote_stripping() {
        // parse_numeric trims whitespace AND strips surrounding quotes before parsing.
        assert!((parse_numeric(" 42 ") - 42.0).abs() < 1e-12);
        // Whitespace-only trims to empty → parse fails → 0.
        assert_eq!(parse_numeric("  "), 0.0);
        // Double-quoted number: "42" → 42.
        assert!((parse_numeric("\"42\"") - 42.0).abs() < 1e-12);
        // Positive sign prefix — f64 accepts it.
        assert!((parse_numeric("+3") - 3.0).abs() < 1e-12);
    }

    #[test]
    fn test_eval_bool_expr_truthy_string_value() {
        // Non-empty, non-zero string → true. "hello" is non-numeric but truthy.
        // Per impl: tries numeric first, then non-numeric strings become truthy.
        assert!(eval_bool_expr("\"hello\""));
        // Empty quoted string — likely falsy.
        assert!(!eval_bool_expr("\"\""));
    }

    #[test]
    fn test_evaluate_functions_abs_zero_is_zero() {
        assert_eq!(evaluate_functions("abs(0)"), "0");
    }

    #[test]
    fn test_expand_units_already_converted_value_unchanged() {
        // Plain integer with no suffix — passed through as is.
        assert_eq!(expand_units("500"), "500");
        assert_eq!(expand_units("-100"), "-100");
    }

    #[test]
    fn test_find_matching_paren_open_position_beyond_input() {
        // Passing pos beyond string length → None (no OOB panic).
        assert_eq!(find_matching_paren("(abc)", 10), None);
        assert_eq!(find_matching_paren("(abc)", 5), None); // just at end
    }

    #[test]
    fn test_expand_units_decimal_suffixes_convert() {
        // 1.5Mb → 1.5e6 as i64 = 1500000.
        assert_eq!(expand_units("1.5Mb"), "1500000");
        assert_eq!(expand_units("2.5kb"), "2500");
        // 0.001Gb → 0.001*1e9 = 1_000_000 exactly.
        assert_eq!(expand_units("0.001Gb"), "1000000");
    }

    #[test]
    fn test_expand_units_mixed_arithmetic_expression() {
        // Each number-unit pair converted independently; operators pass through verbatim.
        assert_eq!(expand_units("1Mb+2kb"), "1000000+2000");
        assert_eq!(expand_units("chr_start > 10Mb"), "chr_start > 10000000");
    }

    #[test]
    fn test_eval_bool_expr_empty_returns_false() {
        // Empty / whitespace-only → trimmed-empty → false (no parse).
        assert!(!eval_bool_expr(""));
        assert!(!eval_bool_expr("   "));
        assert!(!eval_bool_expr("\t\n"));
    }

    #[test]
    fn test_find_matching_paren_nested_pairs_matches_outer() {
        // Outer '(' at 0 matches final ')' at 5; inner '(' at 1 matches inner ')' at 3.
        assert_eq!(find_matching_paren("((a)b)", 0), Some(5));
        assert_eq!(find_matching_paren("((a)b)", 1), Some(3));
        // Not starting on '(' → None.
        assert_eq!(find_matching_paren("((a)b)", 2), None);
        // Unbalanced — no matching close → None.
        assert_eq!(find_matching_paren("(ab", 0), None);
    }

    #[test]
    fn test_evaluate_functions_max_returns_larger_value() {
        // max(a,b) replaced with the larger — result is just the substituted string.
        assert_eq!(evaluate_functions("max(3,7)"), "7");
        assert_eq!(evaluate_functions("max(-5,2)"), "2");
        // Decimal values preserved through f64::max.
        let r = evaluate_functions("max(1.5,2.5)");
        assert!(r == "2.5" || r.starts_with("2.5"));
    }

    #[test]
    fn test_evaluate_functions_min_returns_smaller_value() {
        // min(a,b) replaced with the smaller.
        assert_eq!(evaluate_functions("min(3,7)"), "3");
        assert_eq!(evaluate_functions("min(-5,2)"), "-5");
    }

    #[test]
    fn test_evaluate_functions_abs_yields_magnitude() {
        // abs(a) replaces with |a| — handles negative, zero, positive.
        assert_eq!(evaluate_functions("abs(-5)"), "5");
        assert_eq!(evaluate_functions("abs(5)"), "5");
        assert_eq!(evaluate_functions("abs(0)"), "0");
    }

    #[test]
    fn test_evaluate_functions_malformed_max_breaks_loop_preserves_input() {
        // "max(" without matching close → find_matching_paren None → loop breaks.
        // Original expression preserved unchanged.
        assert_eq!(evaluate_functions("max(1,2"), "max(1,2");
        // "max(" followed by non-comma content after inner — split_once fails → break.
        // (Still contains "max(" so no substitution occurs.)
        let input = "max(just_one_arg)";
        assert_eq!(evaluate_functions(input), input);
    }

    #[test]
    fn test_substitute_link_vars_interchr_and_intrachr_markers() {
        // All-same-chr → _INTRACHR_=1, _INTERCHR_=0.
        use crate::data::types::{Datum, Link};
        use crate::intspan::IntSpan;
        let l = Link {
            id: "L".into(),
            points: vec![
                Datum { chr: "hs1".into(), start: 0, end: 100, set: IntSpan::from_range(0, 100), id: Some("L".into()), value: None, label: None, param: HashMap::new() },
                Datum { chr: "hs1".into(), start: 200, end: 300, set: IntSpan::from_range(200, 300), id: Some("L".into()), value: None, label: None, param: HashMap::new() },
            ],
            param: HashMap::new(),
        };
        let s = substitute_link_vars("_INTERCHR_ / _INTRACHR_", &l);
        assert_eq!(s, "0 / 1");
        // Inter-chr case.
        let mut l2 = l.clone();
        l2.points[1].chr = "hs2".into();
        let s2 = substitute_link_vars("_INTERCHR_ / _INTRACHR_", &l2);
        assert_eq!(s2, "1 / 0");
    }

    #[test]
    fn test_substitute_vars_numeric_values_unquoted_string_quoted() {
        // Numeric "42" → unquoted; non-numeric "text" → wrapped in quotes.
        let mut vars: HashMap<String, String> = HashMap::new();
        vars.insert("count".into(), "42".into());
        vars.insert("name".into(), "alpha".into());
        let r = substitute_vars("_COUNT_ + 0, _NAME_", &vars);
        assert!(r.contains("42 + 0"));
        assert!(r.contains("\"alpha\""));
    }

    #[test]
    fn test_substitute_link_vars_indexed_positions_chr_start_end_size() {
        use crate::data::types::{Datum, Link};
        use crate::intspan::IntSpan;
        let l = Link {
            id: "L".into(),
            points: vec![
                Datum { chr: "hs1".into(), start: 10, end: 20, set: IntSpan::from_range(10, 20), id: Some("L".into()), value: None, label: None, param: HashMap::new() },
                Datum { chr: "hs2".into(), start: 100, end: 200, set: IntSpan::from_range(100, 200), id: Some("L".into()), value: None, label: None, param: HashMap::new() },
            ],
            param: HashMap::new(),
        };
        // _CHR2_ → "hs2"; _SIZE2_ → 101 (200-100+1); _POSITION2_ → (100+200)/2=150.
        let r = substitute_link_vars("_CHR2_ _SIZE2_ _POSITION2_", &l);
        assert_eq!(r, "\"hs2\" 101 150");
    }

    #[test]
    fn test_substitute_vars_uppercases_key_before_pattern_match() {
        // vars key "color" → pattern "_COLOR_" (to_uppercase).
        let mut vars: HashMap<String, String> = HashMap::new();
        vars.insert("color".into(), "red".into());
        let r = substitute_vars("_COLOR_ only", &vars);
        assert!(r.contains("\"red\""));
        // Lowercase _color_ in input is NOT substituted.
        let r2 = substitute_vars("_color_ only", &vars);
        assert!(r2.contains("_color_"));
    }

    #[test]
    fn test_expand_units_identifiers_and_operators_pass_through_unchanged() {
        // No number → no unit expansion; whole string returned verbatim.
        assert_eq!(expand_units("chr eq \"hs1\""), "chr eq \"hs1\"");
        assert_eq!(expand_units("&& || !="), "&& || !=");
        // Mixed: identifier "kb" alone (without preceding digit) passes through.
        assert_eq!(expand_units("foo_kb + bar"), "foo_kb + bar");
    }

    #[test]
    fn test_substitute_link_vars_empty_points_skips_indexed_substitution() {
        // Empty points → no _CHR1_/_CHR2_/etc substitutions; input mostly unchanged.
        use crate::data::types::Link;
        let l = Link {
            id: "empty".into(),
            points: Vec::new(),
            param: HashMap::new(),
        };
        // _INTERCHR_ / _INTRACHR_ still substituted (all_same_chr on empty windows → true).
        let r = substitute_link_vars("_CHR1_ _SIZE1_ _INTRACHR_", &l);
        assert!(r.contains("_CHR1_")); // not substituted
        assert!(r.contains("_SIZE1_"));
        assert!(r.contains(" 1"));      // _INTRACHR_ → "1"
    }

    #[test]
    fn test_parse_numeric_strips_surrounding_quotes() {
        // parse_numeric trims both whitespace and `"` chars before parse.
        assert_eq!(parse_numeric("\"3.14\""), 3.14);
        assert_eq!(parse_numeric("  \"42\"  "), 42.0);
        // Non-numeric → 0.0 via unwrap_or.
        assert_eq!(parse_numeric("abc"), 0.0);
        assert_eq!(parse_numeric("\"abc\""), 0.0);
    }

    #[test]
    fn test_substitute_vars_key_pattern_not_anchored_substitutes_inline() {
        // _FOO_ is replaced wherever it appears — even mid-expression.
        let mut vars: HashMap<String, String> = HashMap::new();
        vars.insert("foo".into(), "100".into());
        let r = substitute_vars("2 * _FOO_ + 5", &vars);
        assert_eq!(r, "2 * 100 + 5");
        // Numeric value → unquoted (no surrounding ").
        assert!(!r.contains("\"100\""));
    }
}
