/// A Rust implementation of Set::IntSpan - efficient integer interval sets.
///
/// Stores sorted, non-overlapping `(start, end)` intervals (inclusive on both ends).
/// Supports the special "(-)" notation meaning the universal set (all integers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntSpan {
    /// Sorted, non-overlapping intervals (inclusive on both ends)
    intervals: Vec<(i64, i64)>,
    /// Whether this represents the universal set "(-)".
    universal: bool,
}

impl IntSpan {
    /// Create an empty set.
    pub fn new() -> Self {
        IntSpan {
            intervals: Vec::new(),
            universal: false,
        }
    }

    /// Parse from a run-list string.
    ///
    /// Formats:
    /// - `""` or `"-"` -> empty set
    /// - `"(-)"` -> universal set
    /// - `"5"` -> single element {5}
    /// - `"1-10"` -> range [1,10]
    /// - `"1-10,20-30"` -> union of ranges
    pub fn from_runlist(s: &str) -> Self {
        let s = s.trim();
        if s.is_empty() || s == "-" {
            return IntSpan::new();
        }
        if s == "(-)" {
            return IntSpan {
                intervals: Vec::new(),
                universal: true,
            };
        }

        let mut span = IntSpan::new();
        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if part.contains('-') {
                // Handle negative numbers: if first char is '-', it's a negative start
                // "1-10" => (1, 10), "-5-10" => (-5, 10), "-10--5" => (-10, -5)
                let (start, end) = parse_range(part);
                if start <= end {
                    span.add_interval(start, end);
                }
            } else {
                let val: i64 = part.parse().expect("invalid integer in runlist");
                span.add_interval(val, val);
            }
        }
        span
    }

    /// Create a span from a single range.
    pub fn from_range(start: i64, end: i64) -> Self {
        let mut span = IntSpan::new();
        if start <= end {
            span.intervals.push((start, end));
        }
        span
    }

    /// Add an interval, merging with existing overlapping/adjacent intervals.
    fn add_interval(&mut self, start: i64, end: i64) {
        if self.universal {
            return;
        }
        let mut new_start = start;
        let mut new_end = end;
        let mut merged = Vec::new();
        let mut inserted = false;

        for &(s, e) in &self.intervals {
            if e < new_start - 1 {
                // Current interval entirely before new one
                merged.push((s, e));
            } else if s > new_end + 1 {
                // Current interval entirely after new one
                if !inserted {
                    merged.push((new_start, new_end));
                    inserted = true;
                }
                merged.push((s, e));
            } else {
                // Overlapping or adjacent - merge
                new_start = new_start.min(s);
                new_end = new_end.max(e);
            }
        }
        if !inserted {
            merged.push((new_start, new_end));
        }
        self.intervals = merged;
    }

    /// Returns true if set is empty.
    pub fn is_empty(&self) -> bool {
        !self.universal && self.intervals.is_empty()
    }

    /// Returns true if set is the universal set.
    pub fn is_universal(&self) -> bool {
        self.universal
    }

    /// Returns the cardinality (number of elements).
    /// Returns i64::MAX for universal set.
    pub fn cardinality(&self) -> i64 {
        if self.universal {
            return i64::MAX;
        }
        self.intervals.iter().map(|&(s, e)| e - s + 1).sum()
    }

    /// Returns the minimum element, or None if empty.
    pub fn min(&self) -> Option<i64> {
        if self.universal {
            return Some(i64::MIN);
        }
        self.intervals.first().map(|&(s, _)| s)
    }

    /// Returns the maximum element, or None if empty.
    pub fn max(&self) -> Option<i64> {
        if self.universal {
            return Some(i64::MAX);
        }
        self.intervals.last().map(|&(_, e)| e)
    }

    /// Returns true if the set contains the given value.
    pub fn member(&self, val: i64) -> bool {
        if self.universal {
            return true;
        }
        self.intervals
            .binary_search_by(|&(s, e)| {
                if val < s {
                    std::cmp::Ordering::Greater
                } else if val > e {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }

    /// Compute the intersection of two sets.
    pub fn intersect(&self, other: &IntSpan) -> IntSpan {
        if self.universal {
            return other.clone();
        }
        if other.universal {
            return self.clone();
        }

        let mut result = IntSpan::new();
        let (mut i, mut j) = (0, 0);
        while i < self.intervals.len() && j < other.intervals.len() {
            let (s1, e1) = self.intervals[i];
            let (s2, e2) = other.intervals[j];
            let start = s1.max(s2);
            let end = e1.min(e2);
            if start <= end {
                result.intervals.push((start, end));
            }
            if e1 < e2 {
                i += 1;
            } else {
                j += 1;
            }
        }
        result
    }

    /// Compute the union of two sets.
    pub fn union(&self, other: &IntSpan) -> IntSpan {
        if self.universal || other.universal {
            return IntSpan {
                intervals: Vec::new(),
                universal: true,
            };
        }

        let mut result = self.clone();
        for &(s, e) in &other.intervals {
            result.add_interval(s, e);
        }
        result
    }

    /// Compute self - other (set difference).
    pub fn diff(&self, other: &IntSpan) -> IntSpan {
        if self.is_empty() || other.is_empty() {
            return self.clone();
        }
        if other.universal {
            return IntSpan::new();
        }
        if self.universal {
            // Universal minus finite set: not easily representable,
            // but in Circos usage "(-)" diff is always with a finite set
            // and the result is used with intersect afterward.
            // For now, panic; we'll handle this if needed.
            panic!("diff from universal set not supported for finite results");
        }

        let mut result = IntSpan::new();
        let mut j = 0;

        for &(s, e) in &self.intervals {
            while j < other.intervals.len() && other.intervals[j].1 < s {
                j += 1;
            }
            let mut k = j;
            let mut current_start = s;
            while k < other.intervals.len() && other.intervals[k].0 <= e {
                let (os, oe) = other.intervals[k];
                if current_start < os {
                    result.intervals.push((current_start, (os - 1).min(e)));
                }
                current_start = oe + 1;
                k += 1;
            }
            if current_start <= e {
                result.intervals.push((current_start, e));
            }
        }
        result
    }

    /// Returns the run-list string representation.
    pub fn run_list(&self) -> String {
        if self.universal {
            return "(-)".to_string();
        }
        if self.intervals.is_empty() {
            return String::new();
        }
        self.intervals
            .iter()
            .map(|&(s, e)| {
                if s == e {
                    format!("{}", s)
                } else {
                    format!("{}-{}", s, e)
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Returns true if self is a superset of other.
    pub fn superset(&self, other: &IntSpan) -> bool {
        other.diff(self).is_empty()
    }

    /// Apply a function to each element and collect into a new IntSpan.
    /// This is the map_set functionality from Set::IntSpan.
    pub fn map_set<F>(&self, f: F) -> IntSpan
    where
        F: Fn(i64) -> i64,
    {
        let mut result = IntSpan::new();
        for &(s, e) in &self.intervals {
            for val in s..=e {
                let mapped = f(val);
                result.add_interval(mapped, mapped);
            }
        }
        result
    }

    /// Iterate over all elements in the set.
    pub fn iter(&self) -> IntSpanIter<'_> {
        IntSpanIter {
            span: self,
            interval_idx: 0,
            current: 0,
        }
    }
}

impl Default for IntSpan {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for IntSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.run_list())
    }
}

pub struct IntSpanIter<'a> {
    span: &'a IntSpan,
    interval_idx: usize,
    current: i64,
}

impl Iterator for IntSpanIter<'_> {
    type Item = i64;

    fn next(&mut self) -> Option<i64> {
        if self.interval_idx >= self.span.intervals.len() {
            return None;
        }
        let (s, e) = self.span.intervals[self.interval_idx];
        if self.current == 0 && self.interval_idx == 0 {
            self.current = s;
        }
        if self.current > e {
            self.interval_idx += 1;
            if self.interval_idx >= self.span.intervals.len() {
                return None;
            }
            self.current = self.span.intervals[self.interval_idx].0;
        }
        let val = self.current;
        self.current += 1;
        Some(val)
    }
}

/// Parse a range string like "1-10", "-5-10", "-10--5"
fn parse_range(s: &str) -> (i64, i64) {
    let bytes = s.as_bytes();
    // Find the dash that separates start from end.
    // If string starts with '-', the first dash is part of the number.
    let sep_pos = if bytes[0] == b'-' {
        // Start is negative, find next dash after the first digit sequence
        let mut i = 1;
        while i < bytes.len() && (bytes[i].is_ascii_digit()) {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'-' {
            i
        } else {
            // Single negative number
            return (s.parse::<i64>().unwrap(), s.parse::<i64>().unwrap());
        }
    } else {
        s.find('-').unwrap()
    };

    let start: i64 = s[..sep_pos].parse().expect("invalid range start");
    let end: i64 = s[sep_pos + 1..].parse().expect("invalid range end");
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let s = IntSpan::new();
        assert!(s.is_empty());
        assert_eq!(s.cardinality(), 0);
        assert_eq!(s.min(), None);
        assert_eq!(s.max(), None);
    }

    #[test]
    fn test_from_runlist_single() {
        let s = IntSpan::from_runlist("5");
        assert_eq!(s.cardinality(), 1);
        assert_eq!(s.min(), Some(5));
        assert_eq!(s.max(), Some(5));
        assert!(s.member(5));
        assert!(!s.member(4));
    }

    #[test]
    fn test_from_runlist_range() {
        let s = IntSpan::from_runlist("1-10");
        assert_eq!(s.cardinality(), 10);
        assert_eq!(s.min(), Some(1));
        assert_eq!(s.max(), Some(10));
        assert!(s.member(1));
        assert!(s.member(5));
        assert!(s.member(10));
        assert!(!s.member(0));
        assert!(!s.member(11));
    }

    #[test]
    fn test_from_runlist_multiple() {
        let s = IntSpan::from_runlist("1-5,10-15,20");
        assert_eq!(s.cardinality(), 12);
        assert!(s.member(3));
        assert!(!s.member(7));
        assert!(s.member(12));
        assert!(s.member(20));
        assert!(!s.member(21));
    }

    #[test]
    fn test_universal() {
        let s = IntSpan::from_runlist("(-)");
        assert!(s.is_universal());
        assert!(s.member(0));
        assert!(s.member(999999));
        assert_eq!(s.run_list(), "(-)");
    }

    #[test]
    fn test_from_range() {
        let s = IntSpan::from_range(100, 200);
        assert_eq!(s.cardinality(), 101);
        assert_eq!(s.min(), Some(100));
        assert_eq!(s.max(), Some(200));
    }

    #[test]
    fn test_intersect() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let c = a.intersect(&b);
        assert_eq!(c.run_list(), "5-10");
        assert_eq!(c.cardinality(), 6);
    }

    #[test]
    fn test_intersect_disjoint() {
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("10-15");
        let c = a.intersect(&b);
        assert!(c.is_empty());
    }

    #[test]
    fn test_intersect_universal() {
        let a = IntSpan::from_runlist("(-)");
        let b = IntSpan::from_runlist("5-10");
        assert_eq!(a.intersect(&b).run_list(), "5-10");
        assert_eq!(b.intersect(&a).run_list(), "5-10");
    }

    #[test]
    fn test_union() {
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("10-15");
        let c = a.union(&b);
        assert_eq!(c.run_list(), "1-5,10-15");
        assert_eq!(c.cardinality(), 11);
    }

    #[test]
    fn test_union_overlapping() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let c = a.union(&b);
        assert_eq!(c.run_list(), "1-15");
    }

    #[test]
    fn test_union_adjacent() {
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("6-10");
        let c = a.union(&b);
        assert_eq!(c.run_list(), "1-10");
    }

    #[test]
    fn test_diff() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let c = a.diff(&b);
        assert_eq!(c.run_list(), "1-4");
    }

    #[test]
    fn test_diff_subset() {
        let a = IntSpan::from_runlist("1-20");
        let b = IntSpan::from_runlist("5-10");
        let c = a.diff(&b);
        assert_eq!(c.run_list(), "1-4,11-20");
    }

    #[test]
    fn test_diff_empty() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::new();
        assert_eq!(a.diff(&b).run_list(), "1-10");
    }

    #[test]
    fn test_superset() {
        let a = IntSpan::from_runlist("1-20");
        let b = IntSpan::from_runlist("5-10");
        assert!(a.superset(&b));
        assert!(!b.superset(&a));
    }

    #[test]
    fn test_run_list() {
        let s = IntSpan::from_runlist("1-5,10-15,20");
        assert_eq!(s.run_list(), "1-5,10-15,20");
    }

    #[test]
    fn test_display() {
        let s = IntSpan::from_range(1, 10);
        assert_eq!(format!("{}", s), "1-10");
    }

    #[test]
    fn test_negative_range() {
        let s = IntSpan::from_range(-10, -5);
        assert_eq!(s.cardinality(), 6);
        assert!(s.member(-7));
        assert!(!s.member(-4));
    }

    #[test]
    fn test_genomic_usage() {
        // Simulate typical Circos usage: chromosome region
        let chr_set = IntSpan::from_range(0, 247249719);
        let band_set = IntSpan::from_range(0, 2300000);
        let inter = chr_set.intersect(&band_set);
        assert_eq!(inter.cardinality(), 2300001);
        assert_eq!(inter.min(), Some(0));
        assert_eq!(inter.max(), Some(2300000));
    }
}
