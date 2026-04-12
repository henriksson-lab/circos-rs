use std::path::{Path, PathBuf};

/// Check if a string represents a number (integer or float, optional sign and exponent).
pub fn is_number(s: &str) -> bool {
    // Matches Perl: /^[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?$/
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    s.parse::<f64>().is_ok()
}

/// Check if a string is blank (empty or whitespace only).
pub fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// Check if a string is a comment line (starts with optional whitespace then #).
pub fn is_comment(s: &str) -> bool {
    s.trim_start().starts_with('#')
}

/// Check if a value is an integer.
pub fn is_integer(v: f64) -> bool {
    v == v.floor() && v.is_finite()
}

/// Add thousands separators to a number string.
pub fn add_thousands_separator(s: &str, sep: char) -> String {
    if let Some(dot_pos) = s.find('.') {
        let integer_part = &s[..dot_pos];
        let decimal_part = &s[dot_pos..];
        format!("{}{}", insert_separators(integer_part, sep), decimal_part)
    } else {
        insert_separators(s, sep)
    }
}

fn insert_separators(s: &str, sep: char) -> String {
    let (sign, digits) = if s.starts_with('-') || s.starts_with('+') {
        (&s[..1], &s[1..])
    } else {
        ("", s)
    };

    let mut result = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            result.push(sep);
        }
        result.push(c);
    }
    format!("{}{}", sign, result)
}

/// Locate a file by searching in standard directories relative to a base path.
pub fn locate_file(file: &str, search_paths: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(file);
    if path.exists() {
        return Some(path.to_path_buf());
    }

    for dir in search_paths {
        let candidate = dir.join(file);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Build the default search paths relative to a base directory.
pub fn default_search_paths(base_dir: &Path) -> Vec<PathBuf> {
    vec![
        base_dir.to_path_buf(),
        base_dir.join("etc"),
        base_dir.parent().map(|p| p.join("etc")).unwrap_or_default(),
        base_dir.parent().unwrap_or(base_dir).to_path_buf(),
        base_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("etc"))
            .unwrap_or_default(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_number() {
        assert!(is_number("42"));
        assert!(is_number("-3.14"));
        assert!(is_number("+1.5e10"));
        assert!(is_number("0.001"));
        assert!(is_number("1E-5"));
        assert!(!is_number(""));
        assert!(!is_number("abc"));
        assert!(!is_number("12abc"));
    }

    #[test]
    fn test_is_blank() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("\t\n"));
        assert!(!is_blank("a"));
        assert!(!is_blank(" a "));
    }

    #[test]
    fn test_is_comment() {
        assert!(is_comment("# this is a comment"));
        assert!(is_comment("  # indented comment"));
        assert!(!is_comment("not a comment"));
        assert!(!is_comment(""));
    }

    #[test]
    fn test_is_integer() {
        assert!(is_integer(5.0));
        assert!(is_integer(-3.0));
        assert!(is_integer(0.0));
        assert!(!is_integer(3.5));
        assert!(!is_integer(f64::NAN));
    }

    #[test]
    fn test_add_thousands_separator() {
        assert_eq!(add_thousands_separator("1000", ','), "1,000");
        assert_eq!(add_thousands_separator("1000000", ','), "1,000,000");
        assert_eq!(add_thousands_separator("999", ','), "999");
        assert_eq!(add_thousands_separator("1234.567", ','), "1,234.567");
        assert_eq!(add_thousands_separator("-1000", ','), "-1,000");
    }
}
