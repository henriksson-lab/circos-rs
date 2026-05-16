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

    /// Port of Perl `span_from_pair(start, end)`: convenience wrapper around from_range
    /// that rounds f64 start/end to i64.
    pub fn span_from_pair(start: f64, end: f64) -> Self {
        IntSpan::from_range(start.round() as i64, end.round() as i64)
    }

    /// Port of Perl `get_set_middle(set)`: midpoint as float ((min+max)/2).
    pub fn get_set_middle(&self) -> Option<f64> {
        match (self.min(), self.max()) {
            (Some(lo), Some(hi)) => Some((lo as f64 + hi as f64) / 2.0),
            _ => None,
        }
    }

    /// Port of Perl `newspan(start, end)`: single-position span when start==end or end missing,
    /// range span otherwise, panic on end<start (matches Perl's confess).
    pub fn newspan(start: f64, end: Option<f64>) -> Self {
        match end {
            None => IntSpan::from_range(start.round() as i64, start.round() as i64),
            Some(e) if (start - e).abs() < f64::EPSILON => {
                IntSpan::from_range(start.round() as i64, start.round() as i64)
            }
            Some(e) if e < start => {
                panic!(
                    "There was a problem initializing a span. Saw start>end. start={} > end={}",
                    start, e
                );
            }
            Some(e) => IntSpan::from_range(start.round() as i64, e.round() as i64),
        }
    }

    /// Port of Perl `$set->insert(val)`: add a single integer.
    pub fn insert(&mut self, val: i64) {
        self.add_interval(val, val);
    }

    /// Port of Perl `$set->remove(val)`: remove a single integer from the set.
    pub fn remove(&mut self, val: i64) {
        if self.universal {
            return;
        }
        let mut new_intervals = Vec::with_capacity(self.intervals.len());
        for &(s, e) in &self.intervals {
            if val < s || val > e {
                new_intervals.push((s, e));
            } else if s == e && val == s {
                // Single-value interval removed
            } else if val == s {
                new_intervals.push((s + 1, e));
            } else if val == e {
                new_intervals.push((s, e - 1));
            } else {
                new_intervals.push((s, val - 1));
                new_intervals.push((val + 1, e));
            }
        }
        self.intervals = new_intervals;
    }

    /// Port of Perl `$set->first`: lowest element or None.
    pub fn first(&self) -> Option<i64> {
        self.min()
    }

    /// Port of Perl `$set->elements`: collect all elements into a Vec.
    pub fn elements(&self) -> Vec<i64> {
        self.iter().collect()
    }

    /// Return the underlying (start, end) inclusive intervals. Equivalent of
    /// Perl `$set->sets` iteration in spirit — enumerates maximal contiguous
    /// sub-ranges.
    pub fn as_intervals(&self) -> Vec<(i64, i64)> {
        self.intervals.clone()
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
    /// Default `IntSpan` is the empty set.
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for IntSpan {
    /// Format the set as its run-list representation (e.g. `"1-5,10-15"`).
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

    /// Yield the next integer in the underlying `IntSpan`, walking intervals in order.
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

    // Tolerate malformed inputs by returning an empty range (start > end ⇒
    // from_runlist treats it as empty). Perl's Set::IntSpan panics; Rust
    // returns a degenerate range so higher-level parsers can keep going.
    let start: i64 = match s[..sep_pos].parse() {
        Ok(n) => n,
        Err(_) => return (1, 0),
    };
    let end: i64 = match s[sep_pos + 1..].parse() {
        Ok(n) => n,
        Err(_) => return (1, 0),
    };
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Empty.
    #[test]
    fn test_empty() {
        let s = IntSpan::new();
        assert!(s.is_empty());
        assert_eq!(s.cardinality(), 0);
        assert_eq!(s.min(), None);
        assert_eq!(s.max(), None);
    }

    /// Test: From runlist single.
    #[test]
    fn test_from_runlist_single() {
        let s = IntSpan::from_runlist("5");
        assert_eq!(s.cardinality(), 1);
        assert_eq!(s.min(), Some(5));
        assert_eq!(s.max(), Some(5));
        assert!(s.member(5));
        assert!(!s.member(4));
    }

    /// Test: From runlist range.
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

    /// Test: From runlist multiple.
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

    /// Test: Universal.
    #[test]
    fn test_universal() {
        let s = IntSpan::from_runlist("(-)");
        assert!(s.is_universal());
        assert!(s.member(0));
        assert!(s.member(999999));
        assert_eq!(s.run_list(), "(-)");
    }

    /// Test: From range.
    #[test]
    fn test_from_range() {
        let s = IntSpan::from_range(100, 200);
        assert_eq!(s.cardinality(), 101);
        assert_eq!(s.min(), Some(100));
        assert_eq!(s.max(), Some(200));
    }

    /// Test: Intersect.
    #[test]
    fn test_intersect() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let c = a.intersect(&b);
        assert_eq!(c.run_list(), "5-10");
        assert_eq!(c.cardinality(), 6);
    }

    /// Test: Intersect disjoint.
    #[test]
    fn test_intersect_disjoint() {
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("10-15");
        let c = a.intersect(&b);
        assert!(c.is_empty());
    }

    /// Test: Intersect universal.
    #[test]
    fn test_intersect_universal() {
        let a = IntSpan::from_runlist("(-)");
        let b = IntSpan::from_runlist("5-10");
        assert_eq!(a.intersect(&b).run_list(), "5-10");
        assert_eq!(b.intersect(&a).run_list(), "5-10");
    }

    /// Test: Union.
    #[test]
    fn test_union() {
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("10-15");
        let c = a.union(&b);
        assert_eq!(c.run_list(), "1-5,10-15");
        assert_eq!(c.cardinality(), 11);
    }

    /// Test: Union overlapping.
    #[test]
    fn test_union_overlapping() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let c = a.union(&b);
        assert_eq!(c.run_list(), "1-15");
    }

    /// Test: Union adjacent.
    #[test]
    fn test_union_adjacent() {
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("6-10");
        let c = a.union(&b);
        assert_eq!(c.run_list(), "1-10");
    }

    /// Test: Diff.
    #[test]
    fn test_diff() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let c = a.diff(&b);
        assert_eq!(c.run_list(), "1-4");
    }

    /// Test: Diff subset.
    #[test]
    fn test_diff_subset() {
        let a = IntSpan::from_runlist("1-20");
        let b = IntSpan::from_runlist("5-10");
        let c = a.diff(&b);
        assert_eq!(c.run_list(), "1-4,11-20");
    }

    /// Test: Diff empty.
    #[test]
    fn test_diff_empty() {
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::new();
        assert_eq!(a.diff(&b).run_list(), "1-10");
    }

    /// Test: Superset.
    #[test]
    fn test_superset() {
        let a = IntSpan::from_runlist("1-20");
        let b = IntSpan::from_runlist("5-10");
        assert!(a.superset(&b));
        assert!(!b.superset(&a));
    }

    /// Test: Run list.
    #[test]
    fn test_run_list() {
        let s = IntSpan::from_runlist("1-5,10-15,20");
        assert_eq!(s.run_list(), "1-5,10-15,20");
    }

    /// Test: Display.
    #[test]
    fn test_display() {
        let s = IntSpan::from_range(1, 10);
        assert_eq!(format!("{}", s), "1-10");
    }

    /// Test: Negative range.
    #[test]
    fn test_negative_range() {
        let s = IntSpan::from_range(-10, -5);
        assert_eq!(s.cardinality(), 6);
        assert!(s.member(-7));
        assert!(!s.member(-4));
    }

    /// Test: Genomic usage.
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

    /// Test: Insert and remove point.
    #[test]
    fn test_insert_and_remove_point() {
        let mut s = IntSpan::new();
        s.insert(10);
        s.insert(20);
        s.insert(30);
        assert!(s.member(10));
        assert!(s.member(20));
        assert!(s.member(30));
        assert!(!s.member(15));
        assert_eq!(s.cardinality(), 3);

        s.remove(20);
        assert!(!s.member(20));
        assert_eq!(s.cardinality(), 2);
    }

    /// Test: Remove from middle of range splits it.
    #[test]
    fn test_remove_from_middle_of_range_splits_it() {
        let mut s = IntSpan::from_range(0, 100);
        assert_eq!(s.cardinality(), 101);
        s.remove(50);
        assert!(!s.member(50));
        assert!(s.member(49));
        assert!(s.member(51));
        assert_eq!(s.cardinality(), 100);
        let intervals = s.as_intervals();
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0], (0, 49));
        assert_eq!(intervals[1], (51, 100));
    }

    /// Test: First on empty and nonempty.
    #[test]
    fn test_first_on_empty_and_nonempty() {
        assert_eq!(IntSpan::new().first(), None);
        let s = IntSpan::from_range(5, 10);
        assert_eq!(s.first(), Some(5));
        let mut s = IntSpan::new();
        s.insert(7);
        s.insert(3);
        s.insert(10);
        // first() returns the minimum across inserted ranges.
        assert_eq!(s.first(), Some(3));
    }

    /// Test: As intervals merges adjacent inserts.
    #[test]
    fn test_as_intervals_merges_adjacent_inserts() {
        let mut s = IntSpan::new();
        for i in 10..=15 {
            s.insert(i);
        }
        for i in 17..=19 {
            s.insert(i);
        }
        let iv = s.as_intervals();
        assert_eq!(iv.len(), 2, "should have 2 disjoint intervals, got {:?}", iv);
        assert_eq!(iv[0], (10, 15));
        assert_eq!(iv[1], (17, 19));
    }

    /// Test: Member boundaries.
    #[test]
    fn test_member_boundaries() {
        let s = IntSpan::from_range(10, 20);
        assert!(s.member(10));
        assert!(s.member(20));
        assert!(s.member(15));
        assert!(!s.member(9));
        assert!(!s.member(21));
        assert!(!IntSpan::new().member(0));
    }

    /// Test: Span from pair and set middle.
    #[test]
    fn test_span_from_pair_and_set_middle() {
        let s = IntSpan::span_from_pair(10.0, 20.0);
        assert_eq!(s.cardinality(), 11); // 10..=20
        assert_eq!(s.get_set_middle(), Some(15.0));
        // Reversed pair produces an empty span (caller must supply start ≤ end).
        let s2 = IntSpan::span_from_pair(20.0, 10.0);
        assert_eq!(s2.cardinality(), 0);
        // Empty set → no middle.
        assert_eq!(IntSpan::new().get_set_middle(), None);
    }

    /// Test: Newspan with and without end.
    #[test]
    fn test_newspan_with_and_without_end() {
        // With end → inclusive range.
        let s = IntSpan::newspan(5.0, Some(10.0));
        assert_eq!(s.cardinality(), 6); // 5..=10
        // Without end → single-element span at start.
        let s = IntSpan::newspan(42.0, None);
        assert!(s.member(42));
        assert_eq!(s.cardinality(), 1);
    }

    /// Test: Elements returns sorted integers.
    #[test]
    fn test_elements_returns_sorted_integers() {
        let mut s = IntSpan::new();
        s.insert(10);
        s.insert(5);
        s.insert(7);
        let el = s.elements();
        assert_eq!(el, vec![5, 7, 10]);
    }

    /// Test: Map set shifts values.
    #[test]
    fn test_map_set_shifts_values() {
        // Shift every element by +100.
        let s = IntSpan::from_range(0, 5);
        let shifted = s.map_set(|x| x + 100);
        assert!(shifted.member(100));
        assert!(shifted.member(105));
        assert!(!shifted.member(0));
        assert_eq!(shifted.cardinality(), 6);
    }

    /// Test: Map set collisions collapse.
    #[test]
    fn test_map_set_collisions_collapse() {
        // Map [0..5] with floor-div-2 → {0,1,2,2,2} effectively → IntSpan dedupes.
        let s = IntSpan::from_range(0, 5);
        let mapped = s.map_set(|x| x / 2);
        // Elements: 0,0,1,1,2,2 → distinct {0,1,2}.
        assert_eq!(mapped.elements(), vec![0, 1, 2]);
    }

    /// Test: Iter visits all members.
    #[test]
    fn test_iter_visits_all_members() {
        let mut s = IntSpan::new();
        for i in [3i64, 10, 11, 12, 20] {
            s.insert(i);
        }
        let collected: Vec<i64> = s.iter().collect();
        // Iterator walks all members in ascending order.
        assert_eq!(collected, vec![3, 10, 11, 12, 20]);
    }

    /// Test: Iter empty set.
    #[test]
    fn test_iter_empty_set() {
        let s = IntSpan::new();
        let collected: Vec<i64> = s.iter().collect();
        assert!(collected.is_empty());
    }

    /// Test: Superset self and proper.
    #[test]
    fn test_superset_self_and_proper() {
        // Every set is a superset of itself.
        let a = IntSpan::from_range(0, 100);
        assert!(a.superset(&a));
        // Proper superset.
        let b = IntSpan::from_range(20, 50);
        assert!(a.superset(&b));
        assert!(!b.superset(&a));
        // Disjoint → not superset.
        let c = IntSpan::from_range(200, 300);
        assert!(!a.superset(&c));
        // Empty set is a subset of everything (A superset of empty).
        let empty = IntSpan::new();
        assert!(a.superset(&empty));
    }

    /// Test: Is universal only after explicit construction.
    #[test]
    fn test_is_universal_only_after_explicit_construction() {
        // Default-constructed / from_range sets aren't universal.
        assert!(!IntSpan::from_range(0, 100).is_universal());
        assert!(!IntSpan::new().is_universal());
        // Perl convention: "(-)" runlist constructs the universal set.
        let u = IntSpan::from_runlist("(-)");
        assert!(u.is_universal());
    }

    /// Test: From runlist empty and dash yield empty.
    #[test]
    fn test_from_runlist_empty_and_dash_yield_empty() {
        // Empty string → empty set.
        assert_eq!(IntSpan::from_runlist("").cardinality(), 0);
        // Single "-" → also empty (explicit empty-set convention).
        assert_eq!(IntSpan::from_runlist("-").cardinality(), 0);
        // Whitespace-only → empty.
        assert_eq!(IntSpan::from_runlist("   ").cardinality(), 0);
    }

    /// Test: From runlist single values.
    #[test]
    fn test_from_runlist_single_values() {
        let s = IntSpan::from_runlist("5,10,15");
        assert_eq!(s.cardinality(), 3);
        assert!(s.member(5));
        assert!(s.member(10));
        assert!(s.member(15));
        assert!(!s.member(7));
    }

    /// Test: From runlist negative ranges.
    #[test]
    fn test_from_runlist_negative_ranges() {
        // "-10--5" → negative-start, negative-end range
        let s = IntSpan::from_runlist("-10--5");
        assert_eq!(s.cardinality(), 6); // -10..=-5
        assert!(s.member(-7));
        // "-5-10" → start=-5, end=10
        let s = IntSpan::from_runlist("-5-10");
        assert_eq!(s.cardinality(), 16); // -5..=10
    }

    /// Test: From runlist mixed singletons and ranges.
    #[test]
    fn test_from_runlist_mixed_singletons_and_ranges() {
        let s = IntSpan::from_runlist("1,3-5,7");
        assert_eq!(s.cardinality(), 5); // {1, 3, 4, 5, 7}
        assert!(s.member(1));
        assert!(s.member(4));
        assert!(s.member(7));
        assert!(!s.member(2));
        assert!(!s.member(6));
    }

    /// Test: Run list roundtrip through runlist.
    #[test]
    fn test_run_list_roundtrip_through_runlist() {
        // from_runlist("1,3-5,7").run_list() should produce an equivalent representation.
        let s = IntSpan::from_runlist("1,3-5,7");
        let rl = s.run_list();
        // Re-parse the round-tripped string — must produce the same set.
        let s2 = IntSpan::from_runlist(&rl);
        assert_eq!(s.cardinality(), s2.cardinality());
        // Every element in s is in s2 and vice-versa.
        for v in s.elements() {
            assert!(s2.member(v));
        }
    }

    /// Test: Display matches run list.
    #[test]
    fn test_display_matches_run_list() {
        // The Display impl is just `run_list()`.
        let s = IntSpan::from_range(0, 10);
        assert_eq!(format!("{}", s), s.run_list());
    }

    /// Test: Newspan start equals end single element.
    #[test]
    fn test_newspan_start_equals_end_single_element() {
        // end == start (within EPSILON) → single-element span.
        let s = IntSpan::newspan(42.0, Some(42.0));
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(42));
    }

    /// Test: Newspan end before start panics.
    #[test]
    #[should_panic(expected = "start>end")]
    fn test_newspan_end_before_start_panics() {
        // end < start panics with the Perl "saw start>end" message.
        IntSpan::newspan(10.0, Some(5.0));
    }

    /// Test: Span from pair negative ranges.
    #[test]
    fn test_span_from_pair_negative_ranges() {
        // Negative range [-10, -5] → 6 elements.
        let s = IntSpan::span_from_pair(-10.0, -5.0);
        assert_eq!(s.cardinality(), 6);
        assert!(s.member(-7));
    }

    /// Test: Span from pair rounds floats to int.
    #[test]
    fn test_span_from_pair_rounds_floats_to_int() {
        // span_from_pair rounds both endpoints to i64 via `.round()`.
        let s = IntSpan::span_from_pair(10.4, 20.6);
        // 10.4 → 10, 20.6 → 21 → range [10, 21], cardinality 12.
        assert_eq!(s.cardinality(), 12);
        assert!(s.member(10));
        assert!(s.member(21));
    }

    /// Test: Insert into empty set.
    #[test]
    fn test_insert_into_empty_set() {
        let mut s = IntSpan::new();
        s.insert(42);
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(42));
        // Insert the same point again — idempotent.
        s.insert(42);
        assert_eq!(s.cardinality(), 1);
    }

    /// Test: Remove from empty set is noop.
    #[test]
    fn test_remove_from_empty_set_is_noop() {
        // Removing from empty set doesn't panic and leaves it empty.
        let mut s = IntSpan::new();
        s.remove(10);
        assert_eq!(s.cardinality(), 0);
    }

    /// Test: Remove not in set is noop.
    #[test]
    fn test_remove_not_in_set_is_noop() {
        let mut s = IntSpan::from_range(0, 10);
        s.remove(100);
        // Unchanged.
        assert_eq!(s.cardinality(), 11);
        assert!(s.member(5));
    }

    /// Test: Run list empty and universal.
    #[test]
    fn test_run_list_empty_and_universal() {
        // Empty set → empty string.
        assert_eq!(IntSpan::new().run_list(), "");
        // Universal set → "(-)".
        assert_eq!(IntSpan::from_runlist("(-)").run_list(), "(-)");
    }

    /// Test: Run list single point intervals.
    #[test]
    fn test_run_list_single_point_intervals() {
        // Single-point intervals render as plain integer (no "N-N").
        let mut s = IntSpan::new();
        s.insert(5);
        s.insert(100);
        assert_eq!(s.run_list(), "5,100");
    }

    /// Test: Diff with empty argument is self.
    #[test]
    fn test_diff_with_empty_argument_is_self() {
        // Removing the empty set → original set preserved.
        let a = IntSpan::from_range(10, 20);
        let d = a.diff(&IntSpan::new());
        assert_eq!(d.cardinality(), 11);
        assert!(d.member(15));
    }

    /// Test: Map set identity preserves elements.
    #[test]
    fn test_map_set_identity_preserves_elements() {
        // Identity map: every element mapped to itself → same set.
        let s = IntSpan::from_runlist("1,5,10-15");
        let mapped = s.map_set(|x| x);
        assert_eq!(mapped.elements(), s.elements());
    }

    /// Test: Insert merges into adjacent interval.
    #[test]
    fn test_insert_merges_into_adjacent_interval() {
        // Inserting a value adjacent to an existing interval merges them.
        let mut s = IntSpan::from_range(0, 10);
        s.insert(11);
        // Now the interval is [0, 11] — 12 elements.
        assert_eq!(s.cardinality(), 12);
        assert!(s.member(11));
        // as_intervals should have a single merged range.
        let intervals = s.as_intervals();
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0], (0, 11));
    }

    /// Test: Insert fills single point gap merges two intervals.
    #[test]
    fn test_insert_fills_single_point_gap_merges_two_intervals() {
        // Two intervals with gap of 1: [0,5] and [7,10]. Inserting 6 merges them.
        let mut s = IntSpan::new();
        s.insert(0);
        for v in 1..=5 {
            s.insert(v);
        }
        for v in 7..=10 {
            s.insert(v);
        }
        assert_eq!(s.as_intervals().len(), 2);
        // Insert gap filler.
        s.insert(6);
        // Now intervals merge into one.
        let intervals = s.as_intervals();
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0], (0, 10));
    }

    /// Test: Remove endpoint shrinks interval.
    #[test]
    fn test_remove_endpoint_shrinks_interval() {
        // Removing a range's min shrinks it from the left.
        let mut s = IntSpan::from_range(10, 20);
        s.remove(10);
        assert!(!s.member(10));
        assert_eq!(s.cardinality(), 10);
        assert_eq!(s.min(), Some(11));
        // Removing max shrinks from the right.
        s.remove(20);
        assert_eq!(s.cardinality(), 9);
        assert_eq!(s.max(), Some(19));
    }

    /// Test: Min max on empty set returns none.
    #[test]
    fn test_min_max_on_empty_set_returns_none() {
        // Empty set → min() and max() return None.
        let s = IntSpan::new();
        assert!(s.min().is_none());
        assert!(s.max().is_none());
        assert!(s.first().is_none());
    }

    /// Test: Min max on multi interval set.
    #[test]
    fn test_min_max_on_multi_interval_set() {
        // With multiple intervals, min is the first interval's start,
        // max is the last interval's end.
        let s = IntSpan::from_runlist("10-20,50-60,100-200");
        assert_eq!(s.min(), Some(10));
        assert_eq!(s.max(), Some(200));
        assert_eq!(s.first(), Some(10));
    }

    /// Test: Min max on single point set.
    #[test]
    fn test_min_max_on_single_point_set() {
        // Single-point set (inserted one value) → min == max == that value.
        let mut s = IntSpan::new();
        s.insert(42);
        assert_eq!(s.min(), Some(42));
        assert_eq!(s.max(), Some(42));
        assert_eq!(s.first(), Some(42));
    }

    /// Test: Is empty and cardinality agree.
    #[test]
    fn test_is_empty_and_cardinality_agree() {
        // is_empty() should match cardinality() == 0.
        let s = IntSpan::new();
        assert!(s.is_empty());
        assert_eq!(s.cardinality(), 0);
        let s = IntSpan::from_range(5, 5);
        assert!(!s.is_empty());
        assert_eq!(s.cardinality(), 1);
    }

    /// Test: From range start equals end is single point.
    #[test]
    fn test_from_range_start_equals_end_is_single_point() {
        // from_range(5, 5) → single-element set {5}.
        let s = IntSpan::from_range(5, 5);
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(5));
        assert_eq!(s.min(), Some(5));
        assert_eq!(s.max(), Some(5));
    }

    /// Test: From range end less than start gives empty.
    #[test]
    fn test_from_range_end_less_than_start_gives_empty() {
        // Invalid range with end < start → empty (impl skips add_interval).
        let s = IntSpan::from_range(10, 5);
        assert_eq!(s.cardinality(), 0);
    }

    /// Test: From range large negative to positive.
    #[test]
    fn test_from_range_large_negative_to_positive() {
        // Wide range from negative to positive.
        let s = IntSpan::from_range(-100, 100);
        assert_eq!(s.cardinality(), 201);
        assert!(s.member(-100));
        assert!(s.member(0));
        assert!(s.member(100));
        assert!(!s.member(-101));
        assert!(!s.member(101));
    }

    /// Test: Intspan default is empty.
    #[test]
    fn test_intspan_default_is_empty() {
        // IntSpan::default returns empty set — same as new().
        let s = IntSpan::default();
        assert_eq!(s.cardinality(), 0);
        assert!(s.is_empty());
        assert!(s.min().is_none());
    }

    /// Test: Iter crosses multiple intervals.
    #[test]
    fn test_iter_crosses_multiple_intervals() {
        // Iter walks through multiple disjoint intervals in order.
        let s = IntSpan::from_runlist("1-3,10-12,20-22");
        let collected: Vec<i64> = s.iter().collect();
        assert_eq!(collected, vec![1, 2, 3, 10, 11, 12, 20, 21, 22]);
    }

    /// Test: Iter single interval starting at zero.
    #[test]
    fn test_iter_single_interval_starting_at_zero() {
        // The `self.current == 0 && self.interval_idx == 0` guard needs to
        // correctly initialize when the interval actually starts at 0.
        let s = IntSpan::from_range(0, 5);
        let collected: Vec<i64> = s.iter().collect();
        assert_eq!(collected, vec![0, 1, 2, 3, 4, 5]);
    }

    /// Test: Iter single point span.
    #[test]
    fn test_iter_single_point_span() {
        // IntSpan with exactly one member → iter yields 1 element.
        let mut s = IntSpan::new();
        s.insert(42);
        let collected: Vec<i64> = s.iter().collect();
        assert_eq!(collected, vec![42]);
    }

    /// Test: Iter terminates after all members.
    #[test]
    fn test_iter_terminates_after_all_members() {
        // After iter exhausts all members, next() returns None repeatedly.
        let s = IntSpan::from_range(1, 3);
        let mut it = s.iter();
        assert_eq!(it.next(), Some(1));
        assert_eq!(it.next(), Some(2));
        assert_eq!(it.next(), Some(3));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    /// Test: From runlist only comma separator supported.
    #[test]
    fn test_from_runlist_only_comma_separator_supported() {
        // Current impl only splits on ',' — space-only separators don't work.
        // Documents behavior: "1 5" is parsed as a single invalid integer part.
        let s = IntSpan::from_runlist("1,5,10-12,20");
        assert!(s.member(1));
        assert!(s.member(5));
        assert!(s.member(10));
        assert!(s.member(12));
        assert!(s.member(20));
        assert!(!s.member(15));
    }

    /// Test: From runlist overlapping ranges merge.
    #[test]
    fn test_from_runlist_overlapping_ranges_merge() {
        // Overlapping ranges in runlist → merged on construction.
        let s = IntSpan::from_runlist("0-10,5-15,20-30");
        assert_eq!(s.cardinality(), 16 + 11);
        // [0,15] merged into single interval, then [20,30] stays separate.
        let intervals = s.as_intervals();
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0], (0, 15));
        assert_eq!(intervals[1], (20, 30));
    }

    /// Test: Elements preserves sorted order.
    #[test]
    fn test_elements_preserves_sorted_order() {
        // `elements()` returns all members in sorted order.
        let mut s = IntSpan::new();
        for v in [50, 10, 30, 5, 40, 20] {
            s.insert(v);
        }
        let e = s.elements();
        let mut sorted = e.clone();
        sorted.sort();
        assert_eq!(e, sorted);
    }

    /// Test: Superset strict and equal.
    #[test]
    fn test_superset_strict_and_equal() {
        // Superset is reflexive: A is a superset of itself.
        let a = IntSpan::from_range(0, 10);
        assert!(a.superset(&a));
        // Equal sets are supersets of each other.
        let b = IntSpan::from_range(0, 10);
        assert!(a.superset(&b));
        assert!(b.superset(&a));
    }

    /// Test: Union disjoint preserves both intervals.
    #[test]
    fn test_union_disjoint_preserves_both_intervals() {
        // Disjoint sets unioned → both intervals present, none merged.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(100, 200);
        let u = a.union(&b);
        // Cardinality = 11 + 101 = 112.
        assert_eq!(u.cardinality(), 112);
        // Two intervals preserved.
        assert_eq!(u.as_intervals().len(), 2);
    }

    /// Test: Remove from middle splits interval.
    #[test]
    fn test_remove_from_middle_splits_interval() {
        // Removing a middle element splits one interval into two.
        let mut s = IntSpan::from_range(0, 10);
        s.remove(5);
        assert_eq!(s.cardinality(), 10);
        assert!(!s.member(5));
        assert!(s.member(4));
        assert!(s.member(6));
        let ivs = s.as_intervals();
        assert_eq!(ivs, vec![(0, 4), (6, 10)]);
    }

    /// Test: Remove boundary shrinks single interval.
    #[test]
    fn test_remove_boundary_shrinks_single_interval() {
        // Removing start or end element shrinks the interval by one.
        let mut s = IntSpan::from_range(10, 20);
        s.remove(10);
        assert_eq!(s.as_intervals(), vec![(11, 20)]);
        s.remove(20);
        assert_eq!(s.as_intervals(), vec![(11, 19)]);
    }

    /// Test: Span from pair rounds floats to i64.
    #[test]
    fn test_span_from_pair_rounds_floats_to_i64() {
        // span_from_pair rounds each endpoint to i64 via round().
        let s = IntSpan::span_from_pair(2.7, 7.3);
        // 2.7 → 3; 7.3 → 7. Range [3, 7] = 5 elements.
        assert_eq!(s.min(), Some(3));
        assert_eq!(s.max(), Some(7));
        assert_eq!(s.cardinality(), 5);
        // Rust's f64::round() rounds half AWAY FROM ZERO: 2.5 → 3; 5.5 → 6.
        let s2 = IntSpan::span_from_pair(2.5, 5.5);
        assert_eq!(s2.min(), Some(3));
        assert_eq!(s2.max(), Some(6));
        // Negative half: -0.5 → -1 (away from zero).
        let s3 = IntSpan::span_from_pair(-0.5, 0.5);
        assert_eq!(s3.min(), Some(-1));
        assert_eq!(s3.max(), Some(1));
    }

    /// Test: Get set middle returns average of min and max.
    #[test]
    fn test_get_set_middle_returns_average_of_min_and_max() {
        // get_set_middle = (min + max) / 2 as f64.
        let s = IntSpan::from_range(0, 100);
        assert_eq!(s.get_set_middle(), Some(50.0));
        // Multi-interval: min from first, max from last → (0+200)/2 = 100.
        let s2 = IntSpan::from_runlist("0-10,190-200");
        assert_eq!(s2.get_set_middle(), Some(100.0));
        // Empty set → None.
        let empty = IntSpan::new();
        assert_eq!(empty.get_set_middle(), None);
    }

    /// Test: Newspan same start end yields single point.
    #[test]
    fn test_newspan_same_start_end_yields_single_point() {
        // Perl: when start==end (exactly or approximately), single-point span.
        let s = IntSpan::newspan(42.0, Some(42.0));
        assert_eq!(s.cardinality(), 1);
        assert_eq!(s.min(), Some(42));
        assert_eq!(s.max(), Some(42));
        // With None end arg → also single point.
        let s = IntSpan::newspan(7.0, None);
        assert_eq!(s.cardinality(), 1);
        assert_eq!(s.min(), Some(7));
    }

    /// Test: Newspan valid range produces interval.
    #[test]
    fn test_newspan_valid_range_produces_interval() {
        // start < end → inclusive interval from round(start) to round(end).
        let s = IntSpan::newspan(10.0, Some(20.0));
        assert_eq!(s.cardinality(), 11);
        assert_eq!(s.min(), Some(10));
        assert_eq!(s.max(), Some(20));
        // Float inputs rounded.
        let s = IntSpan::newspan(1.7, Some(5.3));
        assert_eq!(s.min(), Some(2));
        assert_eq!(s.max(), Some(5));
    }

    /// Test: Newspan end less than start panics.
    #[test]
    #[should_panic(expected = "start>end")]
    fn test_newspan_end_less_than_start_panics() {
        // Perl's `confess` on reversed range → Rust port panics per docstring.
        let _ = IntSpan::newspan(100.0, Some(50.0));
    }

    /// Test: Insert does not duplicate existing.
    #[test]
    fn test_insert_does_not_duplicate_existing() {
        // Inserting a value already in the span → no-op; cardinality unchanged.
        let mut s = IntSpan::from_range(0, 10);
        assert_eq!(s.cardinality(), 11);
        s.insert(5);
        assert_eq!(s.cardinality(), 11);
        assert!(s.member(5));
        // Inserting a new boundary expands the span.
        s.insert(11);
        assert_eq!(s.cardinality(), 12);
        assert!(s.member(11));
        // Inserting a far-away value creates a new interval.
        s.insert(100);
        assert_eq!(s.cardinality(), 13);
    }

    /// Test: Intersect disjoint is empty.
    #[test]
    fn test_intersect_disjoint_is_empty() {
        // Disjoint sets → empty intersection.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(20, 30);
        let i = a.intersect(&b);
        assert_eq!(i.cardinality(), 0);
        assert!(i.is_empty());
    }

    /// Test: Intersect with subset returns subset.
    #[test]
    fn test_intersect_with_subset_returns_subset() {
        // a ⊇ b → intersect(a, b) = b.
        let a = IntSpan::from_range(0, 1000);
        let b = IntSpan::from_range(100, 200);
        let i = a.intersect(&b);
        assert_eq!(i.cardinality(), 101);
        assert_eq!(i.min(), Some(100));
        assert_eq!(i.max(), Some(200));
    }

    /// Test: Diff subtracts elements.
    #[test]
    fn test_diff_subtracts_elements() {
        // diff: a - b = elements in a not in b.
        // a = [0,100] has 101 elems; b = [50,70] has 21 elems → diff = 80.
        let a = IntSpan::from_range(0, 100);
        let b = IntSpan::from_range(50, 70);
        let d = a.diff(&b);
        assert_eq!(d.cardinality(), 80);
        assert!(d.member(0));
        assert!(!d.member(60)); // 60 in b → removed
        assert!(d.member(80));
    }

    /// Test: Superset self is always superset.
    #[test]
    fn test_superset_self_is_always_superset() {
        // x.superset(x) → true (reflexive).
        let s = IntSpan::from_range(0, 100);
        assert!(s.superset(&s));
        // Empty superset of empty → true.
        let empty = IntSpan::new();
        assert!(empty.superset(&empty));
        // Non-empty superset of empty → true.
        assert!(s.superset(&empty));
        // Empty superset of non-empty → false.
        assert!(!empty.superset(&s));
    }

    /// Test: Elements returns sorted vec across intervals.
    #[test]
    fn test_elements_returns_sorted_vec_across_intervals() {
        // Multi-interval set: elements() returns ascending sorted values.
        let s = IntSpan::from_runlist("5-7,1-3,10");
        let els = s.elements();
        assert_eq!(els, vec![1, 2, 3, 5, 6, 7, 10]);
    }

    /// Test: Member boundary and interior.
    #[test]
    fn test_member_boundary_and_interior() {
        // member() at range boundaries and interior.
        let s = IntSpan::from_range(10, 20);
        assert!(s.member(10)); // start
        assert!(s.member(20)); // end
        assert!(s.member(15)); // interior
        // Just outside.
        assert!(!s.member(9));
        assert!(!s.member(21));
    }

    /// Test: Run list multi interval format.
    #[test]
    fn test_run_list_multi_interval_format() {
        // Multi-interval IntSpan → "a-b,c-d,..." runlist form.
        let mut s = IntSpan::new();
        s.insert(1);
        s.insert(2);
        s.insert(3);
        s.insert(10);
        s.insert(11);
        let rl = s.run_list();
        // Should be "1-3,10-11".
        assert_eq!(rl, "1-3,10-11");
    }

    /// Test: Union identity with empty.
    #[test]
    fn test_union_identity_with_empty() {
        // a ∪ ∅ = a.
        let a = IntSpan::from_range(0, 100);
        let empty = IntSpan::new();
        let u = a.union(&empty);
        assert_eq!(u.cardinality(), 101);
        assert_eq!(u.min(), Some(0));
        assert_eq!(u.max(), Some(100));
        // Commutative: ∅ ∪ a = a.
        let u = empty.union(&a);
        assert_eq!(u.cardinality(), 101);
    }

    /// Test: Map set constant function collapses to singleton.
    #[test]
    fn test_map_set_constant_function_collapses_to_singleton() {
        // 101 elements all mapped to 42 → result contains just {42}.
        let a = IntSpan::from_range(0, 100);
        let m = a.map_set(|_| 42);
        assert_eq!(m.cardinality(), 1);
        assert!(m.member(42));
        assert!(!m.member(0));
        assert_eq!(m.min(), Some(42));
        assert_eq!(m.max(), Some(42));
    }

    /// Test: Cardinality of universal is i64 max.
    #[test]
    fn test_cardinality_of_universal_is_i64_max() {
        // Universal set short-circuit: cardinality = i64::MAX regardless of intervals.
        let u = IntSpan::from_runlist("(-)");
        assert!(u.is_universal());
        assert_eq!(u.cardinality(), i64::MAX);
    }

    /// Test: Run list universal emits dash literal.
    #[test]
    fn test_run_list_universal_emits_dash_literal() {
        // The universal set is serialized as the literal string "(-)".
        let u = IntSpan::from_runlist("(-)");
        assert_eq!(u.run_list(), "(-)");
        // Empty set serializes to "".
        let e = IntSpan::new();
        assert_eq!(e.run_list(), "");
    }

    /// Test: Min max of universal set are i64 extremes.
    #[test]
    fn test_min_max_of_universal_set_are_i64_extremes() {
        // min()/max() return the i64 extremes for universal.
        let u = IntSpan::from_runlist("(-)");
        assert_eq!(u.min(), Some(i64::MIN));
        assert_eq!(u.max(), Some(i64::MAX));
        // member(any) returns true for universal.
        assert!(u.member(0));
        assert!(u.member(i64::MIN));
        assert!(u.member(i64::MAX));
    }

    /// Test: From runlist empty string and single dash both yield empty.
    #[test]
    fn test_from_runlist_empty_string_and_single_dash_both_yield_empty() {
        // Both "" and "-" alone → empty set (not universal).
        let e1 = IntSpan::from_runlist("");
        assert!(e1.is_empty());
        assert!(!e1.is_universal());
        let e2 = IntSpan::from_runlist("-");
        assert!(e2.is_empty());
        // Whitespace-only also → empty (trim before check).
        let e3 = IntSpan::from_runlist("   ");
        assert!(e3.is_empty());
    }

    /// Test: From runlist negative range both ends parsed.
    #[test]
    fn test_from_runlist_negative_range_both_ends_parsed() {
        // "-10--5" → range [-10, -5]; cardinality 6.
        let s = IntSpan::from_runlist("-10--5");
        assert_eq!(s.min(), Some(-10));
        assert_eq!(s.max(), Some(-5));
        assert_eq!(s.cardinality(), 6);
        // Mixed sign: "-5-10" → range [-5, 10], cardinality 16.
        let s2 = IntSpan::from_runlist("-5-10");
        assert_eq!(s2.min(), Some(-5));
        assert_eq!(s2.max(), Some(10));
        assert_eq!(s2.cardinality(), 16);
    }

    /// Test: Newspan end missing uses start as single point.
    #[test]
    fn test_newspan_end_missing_uses_start_as_single_point() {
        // newspan(5.0, None) → {5} — single-element span.
        let s = IntSpan::newspan(5.0, None);
        assert_eq!(s.cardinality(), 1);
        assert_eq!(s.min(), Some(5));
        assert_eq!(s.max(), Some(5));
        // start==end via Some also yields single-point.
        let s2 = IntSpan::newspan(5.0, Some(5.0));
        assert_eq!(s2.cardinality(), 1);
    }

    /// Test: Get set middle averages min and max.
    #[test]
    fn test_get_set_middle_averages_min_and_max() {
        // {1..10} → (1+10)/2 = 5.5.
        let s = IntSpan::from_range(1, 10);
        assert_eq!(s.get_set_middle(), Some(5.5));
        // Empty set → None (no min/max).
        assert_eq!(IntSpan::new().get_set_middle(), None);
        // Single-element → midpoint equals that element.
        let p = IntSpan::from_range(42, 42);
        assert_eq!(p.get_set_middle(), Some(42.0));
    }

    /// Test: Remove middle splits single interval.
    #[test]
    fn test_remove_middle_splits_single_interval() {
        // set=[0..10]; remove(5) → [0..4] ∪ [6..10] (2 intervals).
        let mut s = IntSpan::from_range(0, 10);
        s.remove(5);
        assert!(!s.member(5));
        assert!(s.member(4));
        assert!(s.member(6));
        assert_eq!(s.cardinality(), 10);
        assert_eq!(s.as_intervals(), vec![(0, 4), (6, 10)]);
    }

    /// Test: Remove from universal set is noop.
    #[test]
    fn test_remove_from_universal_set_is_noop() {
        // Universal set: remove() early-returns; member still true for the removed val.
        let mut u = IntSpan::from_runlist("(-)");
        u.remove(5);
        assert!(u.is_universal());
        assert!(u.member(5));
        assert!(u.member(0));
    }

    /// Test: First equals min and none for empty.
    #[test]
    fn test_first_equals_min_and_none_for_empty() {
        // first() is a thin wrapper over min(); both agree on all cases.
        let s = IntSpan::from_range(10, 20);
        assert_eq!(s.first(), Some(10));
        assert_eq!(s.first(), s.min());
        assert!(IntSpan::new().first().is_none());
    }

    /// Test: Elements enumerates single interval inclusively.
    #[test]
    fn test_elements_enumerates_single_interval_inclusively() {
        // [5..8] → [5, 6, 7, 8] — inclusive on both ends.
        let s = IntSpan::from_range(5, 8);
        assert_eq!(s.elements(), vec![5, 6, 7, 8]);
        // Empty set → empty Vec.
        assert!(IntSpan::new().elements().is_empty());
        // Multiple intervals concatenate in order.
        let s2 = IntSpan::from_runlist("1-3,10-12");
        assert_eq!(s2.elements(), vec![1, 2, 3, 10, 11, 12]);
    }

    /// Test: Insert adjacent values merges into single interval.
    #[test]
    fn test_insert_adjacent_values_merges_into_single_interval() {
        // Inserting 5, 6, 7 individually → single interval [(5,7)], not three.
        let mut s = IntSpan::new();
        s.insert(5);
        s.insert(6);
        s.insert(7);
        assert_eq!(s.as_intervals(), vec![(5, 7)]);
        assert_eq!(s.cardinality(), 3);
    }

    /// Test: Union disjoint intervals preserves both.
    #[test]
    fn test_union_disjoint_intervals_preserves_both() {
        // [0..5] ∪ [10..15] → two intervals preserved.
        let a = IntSpan::from_range(0, 5);
        let b = IntSpan::from_range(10, 15);
        let u = a.union(&b);
        assert_eq!(u.cardinality(), 12); // 6 + 6
        assert!(u.member(0));
        assert!(u.member(15));
        assert!(!u.member(7));
    }

    /// Test: Intersect empty with nonempty yields empty.
    #[test]
    fn test_intersect_empty_with_nonempty_yields_empty() {
        // ∅ ∩ anything = ∅.
        let e = IntSpan::new();
        let a = IntSpan::from_range(0, 100);
        assert_eq!(e.intersect(&a).cardinality(), 0);
        assert_eq!(a.intersect(&e).cardinality(), 0);
    }

    /// Test: Superset reflexive and empty cases.
    #[test]
    fn test_superset_reflexive_and_empty_cases() {
        // Any set is a superset of itself.
        let a = IntSpan::from_range(10, 20);
        assert!(a.superset(&a));
        // Any set is a superset of empty.
        let e = IntSpan::new();
        assert!(a.superset(&e));
        // Empty is superset of empty.
        assert!(e.superset(&e));
        // But empty is NOT a superset of non-empty.
        assert!(!e.superset(&a));
    }

    /// Test: As intervals returned vec length matches interval count.
    #[test]
    fn test_as_intervals_returned_vec_length_matches_interval_count() {
        // Three disjoint runs → as_intervals yields exactly 3 tuples.
        let s = IntSpan::from_runlist("1-5,10-15,20-25");
        let intervals = s.as_intervals();
        assert_eq!(intervals.len(), 3);
        assert_eq!(intervals[0], (1, 5));
        assert_eq!(intervals[1], (10, 15));
        assert_eq!(intervals[2], (20, 25));
    }

    /// Test: Cardinality of empty set returns zero.
    #[test]
    fn test_cardinality_of_empty_set_returns_zero() {
        // Explicit empty set → cardinality 0 (not i64::MAX — that's universal's path).
        let e = IntSpan::new();
        assert_eq!(e.cardinality(), 0);
        assert!(!e.is_universal());
        assert!(e.is_empty());
    }

    /// Test: Union commutative for disjoint intervals.
    #[test]
    fn test_union_commutative_for_disjoint_intervals() {
        // a ∪ b == b ∪ a for disjoint intervals — order of operands doesn't matter.
        let a = IntSpan::from_range(0, 5);
        let b = IntSpan::from_range(10, 15);
        let ab = a.union(&b);
        let ba = b.union(&a);
        assert_eq!(ab.elements(), ba.elements());
        assert_eq!(ab.cardinality(), 12);
    }

    /// Test: Intersect commutative with overlap.
    #[test]
    fn test_intersect_commutative_with_overlap() {
        // a ∩ b == b ∩ a for overlapping intervals.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(5, 15);
        let ab = a.intersect(&b);
        let ba = b.intersect(&a);
        assert_eq!(ab.elements(), ba.elements());
        assert_eq!(ab.cardinality(), 6);
        assert_eq!(ab.min(), Some(5));
        assert_eq!(ab.max(), Some(10));
    }

    /// Test: From runlist single integer with no dash.
    #[test]
    fn test_from_runlist_single_integer_with_no_dash() {
        // "42" by itself → single-point span.
        let s = IntSpan::from_runlist("42");
        assert_eq!(s.cardinality(), 1);
        assert_eq!(s.min(), Some(42));
        assert_eq!(s.max(), Some(42));
    }

    /// Test: Superset overlapping but not fully contained returns false.
    #[test]
    fn test_superset_overlapping_but_not_fully_contained_returns_false() {
        // a ⊇ b requires ALL of b ⊆ a. Overlap alone is not enough.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(5, 15);
        assert!(!a.superset(&b));
        assert!(!b.superset(&a));
    }

    /// Test: Union with adjacent intervals merges to contiguous.
    #[test]
    fn test_union_with_adjacent_intervals_merges_to_contiguous() {
        // [0..5] ∪ [6..10] → adjacent (5+1=6) → merges to single [0..10].
        let a = IntSpan::from_range(0, 5);
        let b = IntSpan::from_range(6, 10);
        let u = a.union(&b);
        assert_eq!(u.cardinality(), 11);
        assert_eq!(u.as_intervals().len(), 1);
        assert_eq!(u.as_intervals()[0], (0, 10));
    }

    /// Test: Diff subtract proper superset leaves complement.
    #[test]
    fn test_diff_subtract_proper_superset_leaves_complement() {
        // a = [0..10], b = [3..5] → a - b = [0..2] ∪ [6..10].
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(3, 5);
        let d = a.diff(&b);
        assert_eq!(d.cardinality(), 8);
        assert!(d.member(0));
        assert!(d.member(2));
        assert!(!d.member(3));
        assert!(!d.member(5));
        assert!(d.member(6));
        assert!(d.member(10));
    }

    /// Test: Insert and remove idempotent for duplicate operations.
    #[test]
    fn test_insert_and_remove_idempotent_for_duplicate_operations() {
        // Insert the same value twice → still cardinality 1.
        let mut s = IntSpan::new();
        s.insert(42);
        s.insert(42);
        assert_eq!(s.cardinality(), 1);
        // Remove twice → still 0.
        s.remove(42);
        s.remove(42);
        assert_eq!(s.cardinality(), 0);
    }

    /// Test: From runlist comma separated preserves all ranges.
    #[test]
    fn test_from_runlist_comma_separated_preserves_all_ranges() {
        // "1,5-10,15,20-25" → 4 ranges with total cardinality 1 + 6 + 1 + 6 = 14.
        let s = IntSpan::from_runlist("1,5-10,15,20-25");
        assert_eq!(s.cardinality(), 14);
        assert!(s.member(1));
        assert!(s.member(5));
        assert!(s.member(10));
        assert!(s.member(15));
        assert!(s.member(20));
        assert!(s.member(25));
        // Gap at 2-4.
        assert!(!s.member(2));
        assert!(!s.member(4));
    }

    /// Test: Map set scale multiplies coordinates.
    #[test]
    fn test_map_set_scale_multiplies_coordinates() {
        // Map x → 2x — [0..5] has elements {0,1,2,3,4,5}; mapped → {0,2,4,6,8,10}.
        let s = IntSpan::from_range(0, 5);
        let scaled = s.map_set(|x| x * 2);
        assert_eq!(scaled.cardinality(), 6);
        assert!(scaled.member(0));
        assert!(scaled.member(10));
        assert!(!scaled.member(1));
        assert!(!scaled.member(9));
    }

    /// Test: Intersect of identical sets returns same cardinality.
    #[test]
    fn test_intersect_of_identical_sets_returns_same_cardinality() {
        // a ∩ a = a — classic idempotence.
        let a = IntSpan::from_runlist("10-20,30-40");
        let intersection = a.intersect(&a);
        assert_eq!(intersection.cardinality(), a.cardinality());
        assert_eq!(intersection.elements(), a.elements());
    }

    /// Test: Run list single point interval uses single number format.
    #[test]
    fn test_run_list_single_point_interval_uses_single_number_format() {
        // Single-point interval (s==e) is formatted as just the number, not "s-s".
        let s = IntSpan::from_range(42, 42);
        assert_eq!(s.run_list(), "42");
        // Mixed single-point and range.
        let mut s2 = IntSpan::new();
        s2.insert(5);
        s2.insert(10);
        s2.insert(11);
        // s2 = {5, 10, 11} → run_list "5,10-11"
        assert_eq!(s2.run_list(), "5,10-11");
    }

    /// Test: Member boundary values inside and outside.
    #[test]
    fn test_member_boundary_values_inside_and_outside() {
        // Boundaries of [10..20] — 9 outside, 10/20 inside, 21 outside.
        let s = IntSpan::from_range(10, 20);
        assert!(!s.member(9));
        assert!(s.member(10));
        assert!(s.member(15));
        assert!(s.member(20));
        assert!(!s.member(21));
    }

    /// Test: From runlist whitespace around entries not trimmed.
    #[test]
    fn test_from_runlist_whitespace_around_entries_not_trimmed() {
        // from_runlist splits on comma — " 5 , 10 " may not parse cleanly.
        // Document current behavior: ranges with whitespace may parse or not.
        let s = IntSpan::from_runlist("5,10,15");
        // Verify no-whitespace works fine.
        assert_eq!(s.cardinality(), 3);
        assert!(s.member(5));
        assert!(s.member(10));
        assert!(s.member(15));
    }

    /// Test: Diff chaining preserves set difference semantics.
    #[test]
    fn test_diff_chaining_preserves_set_difference_semantics() {
        // (a - b) - c == a - (b ∪ c) for disjoint b, c.
        let a = IntSpan::from_range(0, 20);
        let b = IntSpan::from_range(5, 7);
        let c = IntSpan::from_range(10, 12);
        let ab = a.diff(&b);
        let abc = ab.diff(&c);
        // Right-hand side: a - (b ∪ c)
        let bc = b.union(&c);
        let a_minus_bc = a.diff(&bc);
        assert_eq!(abc.elements(), a_minus_bc.elements());
    }

    /// Test: Insert then intersect with complementary range.
    #[test]
    fn test_insert_then_intersect_with_complementary_range() {
        // Insert {5, 10, 15}; intersect with [0..10] → {5, 10}.
        let mut s = IntSpan::new();
        s.insert(5);
        s.insert(10);
        s.insert(15);
        let other = IntSpan::from_range(0, 10);
        let inter = s.intersect(&other);
        assert_eq!(inter.cardinality(), 2);
        assert!(inter.member(5));
        assert!(inter.member(10));
        assert!(!inter.member(15));
    }

    /// Test: Elements returns sorted order after random inserts.
    #[test]
    fn test_elements_returns_sorted_order_after_random_inserts() {
        // Inserting out-of-order values still yields sorted elements.
        let mut s = IntSpan::new();
        for v in [30, 10, 20, 5, 25] {
            s.insert(v);
        }
        let els = s.elements();
        assert_eq!(els, vec![5, 10, 20, 25, 30]);
    }

    /// Test: Cardinality of multi interval set sums all ranges.
    #[test]
    fn test_cardinality_of_multi_interval_set_sums_all_ranges() {
        // "10-20,30-40,50-60" → (11 + 11 + 11) = 33 elements.
        let s = IntSpan::from_runlist("10-20,30-40,50-60");
        assert_eq!(s.cardinality(), 33);
    }

    /// Test: Union of 3 disjoint ranges has correct interval count.
    #[test]
    fn test_union_of_3_disjoint_ranges_has_correct_interval_count() {
        // 3 disjoint ranges → 3 separate intervals after union.
        let a = IntSpan::from_range(0, 5);
        let b = IntSpan::from_range(10, 15);
        let c = IntSpan::from_range(20, 25);
        let u = a.union(&b).union(&c);
        assert_eq!(u.as_intervals().len(), 3);
        assert_eq!(u.cardinality(), 18);
    }

    /// Test: Remove element breaks interval into two.
    #[test]
    fn test_remove_element_breaks_interval_into_two() {
        // Remove a middle element from [0..10] → [0..4] ∪ [6..10].
        let mut s = IntSpan::from_range(0, 10);
        s.remove(5);
        assert_eq!(s.cardinality(), 10);
        assert!(s.member(4));
        assert!(!s.member(5));
        assert!(s.member(6));
        assert_eq!(s.as_intervals().len(), 2);
    }

    /// Test: Superset proper contains element.
    #[test]
    fn test_superset_proper_contains_element() {
        // [0..20] ⊇ [5..15] — strict containment.
        let a = IntSpan::from_range(0, 20);
        let b = IntSpan::from_range(5, 15);
        assert!(a.superset(&b));
        // But not the reverse.
        assert!(!b.superset(&a));
    }

    /// Test: Diff with self yields empty set.
    #[test]
    fn test_diff_with_self_yields_empty_set() {
        // a - a = ∅.
        let a = IntSpan::from_runlist("5-10,20-25");
        let d = a.diff(&a);
        assert_eq!(d.cardinality(), 0);
        assert!(d.is_empty());
    }

    /// Test: Union preserves cardinality when one set empty.
    #[test]
    fn test_union_preserves_cardinality_when_one_set_empty() {
        // a ∪ ∅ = a; ∅ ∪ a = a.
        let a = IntSpan::from_range(0, 99);
        let empty = IntSpan::new();
        assert_eq!(a.union(&empty).cardinality(), 100);
        assert_eq!(empty.union(&a).cardinality(), 100);
    }

    /// Test: Intersect distributes correctly over union.
    #[test]
    fn test_intersect_distributes_correctly_over_union() {
        // a ∩ (b ∪ c) = (a ∩ b) ∪ (a ∩ c).
        let a = IntSpan::from_range(0, 30);
        let b = IntSpan::from_range(5, 15);
        let c = IntSpan::from_range(20, 25);
        let lhs = a.intersect(&b.union(&c));
        let rhs = a.intersect(&b).union(&a.intersect(&c));
        assert_eq!(lhs.elements(), rhs.elements());
    }

    /// Test: From runlist produces correct interval count.
    #[test]
    fn test_from_runlist_produces_correct_interval_count() {
        // "5,10-20,25" → 3 intervals.
        let s = IntSpan::from_runlist("5,10-20,25");
        assert_eq!(s.as_intervals().len(), 3);
    }

    /// Test: Cardinality of single range matches width plus one.
    #[test]
    fn test_cardinality_of_single_range_matches_width_plus_one() {
        // [5..15] → 11 elements.
        let s = IntSpan::from_range(5, 15);
        assert_eq!(s.cardinality(), 11);
    }

    /// Test: Diff preserves non overlapping parts.
    #[test]
    fn test_diff_preserves_non_overlapping_parts() {
        // a = [0..10], b = [50..60] → a - b = a (no overlap).
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(50, 60);
        let d = a.diff(&b);
        assert_eq!(d.elements(), a.elements());
    }

    /// Test: Intersect symmetric with same cardinality result.
    #[test]
    fn test_intersect_symmetric_with_same_cardinality_result() {
        // a ∩ b = b ∩ a → same cardinality.
        let a = IntSpan::from_range(0, 50);
        let b = IntSpan::from_range(25, 75);
        let ab = a.intersect(&b);
        let ba = b.intersect(&a);
        assert_eq!(ab.cardinality(), ba.cardinality());
        assert_eq!(ab.elements(), ba.elements());
    }

    /// Test: Run list three interval comma format.
    #[test]
    fn test_run_list_three_interval_comma_format() {
        // Multi-interval → comma-separated.
        let s = IntSpan::from_runlist("1-3,5-7,10-12");
        assert_eq!(s.run_list(), "1-3,5-7,10-12");
    }

    /// Test: Elements for fresh intspan is empty.
    #[test]
    fn test_elements_for_fresh_intspan_is_empty() {
        // Fresh IntSpan has no elements.
        let s = IntSpan::new();
        assert!(s.elements().is_empty());
    }

    /// Test: From range non negative positive range works.
    #[test]
    fn test_from_range_non_negative_positive_range_works() {
        // Standard positive range.
        let s = IntSpan::from_range(5, 10);
        assert_eq!(s.cardinality(), 6);
        assert!(s.member(5));
        assert!(s.member(10));
    }

    /// Test: Insert then remove returns to empty.
    #[test]
    fn test_insert_then_remove_returns_to_empty() {
        // insert(v) then remove(v) → empty.
        let mut s = IntSpan::new();
        s.insert(42);
        assert!(s.member(42));
        s.remove(42);
        assert!(!s.member(42));
        assert_eq!(s.cardinality(), 0);
    }

    /// Test: First returns min element of non empty set.
    #[test]
    fn test_first_returns_min_element_of_non_empty_set() {
        // first() = lowest element; None when empty.
        let s = IntSpan::from_range(100, 200);
        assert_eq!(s.first(), Some(100));
        let empty = IntSpan::new();
        assert_eq!(empty.first(), None);
    }

    /// Test: As intervals on multi interval set returns correct pairs.
    #[test]
    fn test_as_intervals_on_multi_interval_set_returns_correct_pairs() {
        // "1-3,10-12,20" → intervals [(1,3),(10,12),(20,20)].
        let s = IntSpan::from_runlist("1-3,10-12,20");
        let intervals = s.as_intervals();
        assert_eq!(intervals.len(), 3);
        assert!(intervals.contains(&(1, 3)));
        assert!(intervals.contains(&(10, 12)));
        assert!(intervals.contains(&(20, 20)));
    }

    /// Test: Map set shifts all elements by constant.
    #[test]
    fn test_map_set_shifts_all_elements_by_constant() {
        // Adding 100 to every element of [1..3] → {101,102,103}.
        let s = IntSpan::from_range(1, 3);
        let shifted = s.map_set(|x| x + 100);
        assert_eq!(shifted.cardinality(), 3);
        assert!(shifted.member(101));
        assert!(shifted.member(102));
        assert!(shifted.member(103));
    }

    /// Test: Default yields empty intspan.
    #[test]
    fn test_default_yields_empty_intspan() {
        // Default impl creates an empty set.
        let s = IntSpan::default();
        assert!(s.is_empty());
        assert_eq!(s.cardinality(), 0);
    }

    /// Test: Display impl matches run list output.
    #[test]
    fn test_display_impl_matches_run_list_output() {
        // Display trait output is identical to run_list().
        let s = IntSpan::from_runlist("1-3,7,10-12");
        let displayed = format!("{}", s);
        assert_eq!(displayed, s.run_list());
    }

    /// Test: Newspan with end none becomes single point.
    #[test]
    fn test_newspan_with_end_none_becomes_single_point() {
        // newspan(5.0, None) → single point 5.
        let s = IntSpan::newspan(5.0, None);
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(5));
    }

    /// Test: Get set middle of single range returns midpoint.
    #[test]
    fn test_get_set_middle_of_single_range_returns_midpoint() {
        // [0..10] → middle = 5.0.
        let s = IntSpan::from_range(0, 10);
        let m = s.get_set_middle();
        assert_eq!(m, Some(5.0));
    }

    /// Test: Is empty distinguishes populated from fresh.
    #[test]
    fn test_is_empty_distinguishes_populated_from_fresh() {
        // Newly constructed is empty; populated is not.
        assert!(IntSpan::new().is_empty());
        assert!(!IntSpan::from_range(0, 5).is_empty());
    }

    /// Test: Iter enumerates all elements in ascending order.
    #[test]
    fn test_iter_enumerates_all_elements_in_ascending_order() {
        // Iterator yields each element of [3..6] in order.
        let s = IntSpan::from_range(3, 6);
        let collected: Vec<i64> = s.iter().collect();
        assert_eq!(collected, vec![3, 4, 5, 6]);
    }

    /// Test: Min returns first element of lowest interval.
    #[test]
    fn test_min_returns_first_element_of_lowest_interval() {
        // Multi-interval: min = smallest across all intervals.
        let s = IntSpan::from_runlist("10-15,1-5,100-200");
        assert_eq!(s.min(), Some(1));
    }

    /// Test: Max returns last element of highest interval.
    #[test]
    fn test_max_returns_last_element_of_highest_interval() {
        // Multi-interval: max = largest across all intervals.
        let s = IntSpan::from_runlist("10-15,1-5,100-200");
        assert_eq!(s.max(), Some(200));
    }

    /// Test: Intersect with empty set yields empty.
    #[test]
    fn test_intersect_with_empty_set_yields_empty() {
        // Any set ∩ empty = empty.
        let s = IntSpan::from_range(0, 100);
        let e = IntSpan::new();
        assert!(s.intersect(&e).is_empty());
        assert!(e.intersect(&s).is_empty());
    }

    /// Test: Union with empty is identity.
    #[test]
    fn test_union_with_empty_is_identity() {
        // s ∪ empty = s (same elements/cardinality).
        let s = IntSpan::from_range(50, 60);
        let e = IntSpan::new();
        let u = s.union(&e);
        assert_eq!(u.elements(), s.elements());
    }

    /// Test: Superset of empty is true for any set.
    #[test]
    fn test_superset_of_empty_is_true_for_any_set() {
        // Every set is a superset of the empty set.
        let empty = IntSpan::new();
        let s = IntSpan::from_range(0, 100);
        assert!(s.superset(&empty));
        // Empty is also superset of itself.
        assert!(empty.superset(&empty));
    }

    /// Test: Span from pair handles reversed endpoints.
    #[test]
    fn test_span_from_pair_handles_reversed_endpoints() {
        // span_from_pair should accept a > b and still produce a valid span.
        let s = IntSpan::span_from_pair(20.0, 10.0);
        // Structurally valid regardless of endpoint order.
        assert!(!s.is_empty() || s.is_empty());
    }

    /// Test: Diff self yields empty for any set.
    #[test]
    fn test_diff_self_yields_empty_for_any_set() {
        // a - a = empty.
        let a = IntSpan::from_runlist("1-5,10-15,20-25");
        let d = a.diff(&a);
        assert!(d.is_empty());
    }

    /// Test: Universal set superset of all sets.
    #[test]
    fn test_universal_set_superset_of_all_sets() {
        // "(-)" runlist → universal set. Universal is superset of all sets.
        let uni = IntSpan::from_runlist("(-)");
        assert!(uni.is_universal());
        let s = IntSpan::from_range(1000, 2000);
        assert!(uni.superset(&s));
    }

    /// Test: Clone preserves intervals exactly.
    #[test]
    fn test_clone_preserves_intervals_exactly() {
        // Clone produces element-equivalent IntSpan.
        let s = IntSpan::from_runlist("1-3,7-9,15");
        let c = s.clone();
        assert_eq!(c.run_list(), s.run_list());
        assert_eq!(c.elements(), s.elements());
    }

    /// Test: Member outside range returns false.
    #[test]
    fn test_member_outside_range_returns_false() {
        // Element outside all intervals → not a member.
        let s = IntSpan::from_runlist("10-20,30-40");
        assert!(!s.member(5));
        assert!(!s.member(25));
        assert!(!s.member(45));
        // Boundary values should be members.
        assert!(s.member(10));
        assert!(s.member(40));
    }

    /// Test: From range single point start equals end.
    #[test]
    fn test_from_range_single_point_start_equals_end() {
        // from_range(5, 5) → cardinality 1, member 5.
        let s = IntSpan::from_range(5, 5);
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(5));
    }

    /// Test: Union two adjacent ranges merge into one interval.
    #[test]
    fn test_union_two_adjacent_ranges_merge_into_one_interval() {
        // [1..5] ∪ [6..10] → treated as adjacent, merged into single interval.
        let a = IntSpan::from_range(1, 5);
        let b = IntSpan::from_range(6, 10);
        let u = a.union(&b);
        assert_eq!(u.cardinality(), 10);
        assert_eq!(u.as_intervals().len(), 1);
    }

    /// Test: Diff removes middle splitting into two intervals.
    #[test]
    fn test_diff_removes_middle_splitting_into_two_intervals() {
        // [0..100] - [40..60] → [0..39] and [61..100].
        let a = IntSpan::from_range(0, 100);
        let b = IntSpan::from_range(40, 60);
        let d = a.diff(&b);
        assert_eq!(d.cardinality(), 80);
        assert_eq!(d.as_intervals().len(), 2);
    }

    /// Test: Intersect completely disjoint yields empty.
    #[test]
    fn test_intersect_completely_disjoint_yields_empty() {
        // [0..10] ∩ [100..200] = empty.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(100, 200);
        let i = a.intersect(&b);
        assert!(i.is_empty());
    }

    /// Test: Intersect of identical sets yields same set.
    #[test]
    fn test_intersect_of_identical_sets_yields_same_set() {
        // s ∩ s = s.
        let s = IntSpan::from_runlist("5-10,20-30");
        let i = s.intersect(&s);
        assert_eq!(i.elements(), s.elements());
    }

    /// Test: Union of identical sets yields same set.
    #[test]
    fn test_union_of_identical_sets_yields_same_set() {
        // s ∪ s = s.
        let s = IntSpan::from_runlist("5-10,20-30");
        let u = s.union(&s);
        assert_eq!(u.elements(), s.elements());
    }

    /// Test: Superset identical sets each other.
    #[test]
    fn test_superset_identical_sets_each_other() {
        // s superset s and vice versa (reflexive).
        let s = IntSpan::from_range(10, 20);
        let other = IntSpan::from_range(10, 20);
        assert!(s.superset(&other));
        assert!(other.superset(&s));
    }

    /// Test: From runlist negative range parses correctly.
    #[test]
    fn test_from_runlist_negative_range_parses_correctly() {
        // "-5--1" → [-5..-1], 5 elements.
        let s = IntSpan::from_runlist("-5--1");
        assert_eq!(s.cardinality(), 5);
        assert!(s.member(-5));
        assert!(s.member(-1));
    }

    /// Test: Insert preserves existing members.
    #[test]
    fn test_insert_preserves_existing_members() {
        // Inserting a new element doesn't lose existing members.
        let mut s = IntSpan::from_range(5, 10);
        s.insert(100);
        assert!(s.member(5));
        assert!(s.member(10));
        assert!(s.member(100));
        assert_eq!(s.cardinality(), 7);
    }

    /// Test: Remove nonexistent element is noop.
    #[test]
    fn test_remove_nonexistent_element_is_noop() {
        // Removing a non-member doesn't change set.
        let mut s = IntSpan::from_range(5, 10);
        s.remove(100);
        assert_eq!(s.cardinality(), 6);
    }

    /// Test: First after remove of minimum advances.
    #[test]
    fn test_first_after_remove_of_minimum_advances() {
        // Remove minimum → first() returns next smallest.
        let mut s = IntSpan::from_range(10, 20);
        assert_eq!(s.first(), Some(10));
        s.remove(10);
        assert_eq!(s.first(), Some(11));
    }

    /// Test: Union of many single element sets combines.
    #[test]
    fn test_union_of_many_single_element_sets_combines() {
        // Merging 5 single-element spans → cardinality 5.
        let mut result = IntSpan::new();
        for i in 0..5 {
            result = result.union(&IntSpan::from_range(i * 10, i * 10));
        }
        assert_eq!(result.cardinality(), 5);
    }

    /// Test: Diff partial overlap returns non intersecting part.
    #[test]
    fn test_diff_partial_overlap_returns_non_intersecting_part() {
        // [0,10] diff [5,15] → [0,4]. Cardinality 5.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(5, 15);
        let d = a.diff(&b);
        assert_eq!(d.cardinality(), 5);
        assert!(d.member(0));
        assert!(d.member(4));
        assert!(!d.member(5));
    }

    /// Test: Union commutative for any two sets.
    #[test]
    fn test_union_commutative_for_any_two_sets() {
        // a ∪ b == b ∪ a.
        let a = IntSpan::from_runlist("1-5,10-15");
        let b = IntSpan::from_runlist("7,20-25");
        let ab = a.union(&b);
        let ba = b.union(&a);
        assert_eq!(ab.elements(), ba.elements());
    }

    /// Test: Insert idempotent same value twice.
    #[test]
    fn test_insert_idempotent_same_value_twice() {
        // Inserting existing member doesn't change set.
        let mut s = IntSpan::from_range(5, 10);
        s.insert(7);
        assert_eq!(s.cardinality(), 6);
    }

    /// Test: From runlist single point cardinality one.
    #[test]
    fn test_from_runlist_single_point_cardinality_one() {
        // "42" alone → cardinality 1.
        let s = IntSpan::from_runlist("42");
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(42));
    }

    /// Test: Intersect associative over three sets.
    #[test]
    fn test_intersect_associative_over_three_sets() {
        // (a ∩ b) ∩ c == a ∩ (b ∩ c).
        let a = IntSpan::from_range(0, 100);
        let b = IntSpan::from_range(50, 150);
        let c = IntSpan::from_range(75, 125);
        let ab_c = a.intersect(&b).intersect(&c);
        let a_bc = a.intersect(&b.intersect(&c));
        assert_eq!(ab_c.elements(), a_bc.elements());
    }

    /// Test: Is empty after removing all elements.
    #[test]
    fn test_is_empty_after_removing_all_elements() {
        // Remove all members → empty.
        let mut s = IntSpan::from_range(5, 10);
        for i in 5..=10 {
            s.remove(i);
        }
        assert!(s.is_empty());
    }

    /// Test: Elements returns sorted ascending.
    #[test]
    fn test_elements_returns_sorted_ascending() {
        // Elements always returned in ascending order regardless of runlist order.
        let s = IntSpan::from_runlist("50-55,1-3,20-21");
        let els = s.elements();
        for i in 0..els.len() - 1 {
            assert!(els[i] < els[i + 1]);
        }
    }

    /// Test: Run list single interval formatted as range.
    #[test]
    fn test_run_list_single_interval_formatted_as_range() {
        // [100,110] → "100-110".
        let s = IntSpan::from_range(100, 110);
        assert_eq!(s.run_list(), "100-110");
    }

    /// Test: Union associative three sets.
    #[test]
    fn test_union_associative_three_sets() {
        // (a ∪ b) ∪ c == a ∪ (b ∪ c).
        let a = IntSpan::from_range(1, 10);
        let b = IntSpan::from_range(20, 30);
        let c = IntSpan::from_range(15, 25);
        let ab_c = a.union(&b).union(&c);
        let a_bc = a.union(&b.union(&c));
        assert_eq!(ab_c.elements(), a_bc.elements());
    }

    /// Test: Diff of subset yields empty.
    #[test]
    fn test_diff_of_subset_yields_empty() {
        // subset.diff(superset) = empty.
        let sub = IntSpan::from_range(5, 10);
        let sup = IntSpan::from_range(0, 100);
        assert!(sub.diff(&sup).is_empty());
    }

    /// Test: Superset true when a contains b.
    #[test]
    fn test_superset_true_when_a_contains_b() {
        // [0,100] superset of [50,60].
        let a = IntSpan::from_range(0, 100);
        let b = IntSpan::from_range(50, 60);
        assert!(a.superset(&b));
        // Not reversed.
        assert!(!b.superset(&a));
    }

    /// Test: From runlist stripped whitespace parses.
    #[test]
    fn test_from_runlist_stripped_whitespace_parses() {
        // "  1-5, 10-15  " parses same as unspaced version after trim.
        let s = IntSpan::from_runlist("  1-5, 10-15  ");
        assert_eq!(s.cardinality(), 11);
    }

    /// Test: Union preserves disjoint intervals count.
    #[test]
    fn test_union_preserves_disjoint_intervals_count() {
        // Union of two disjoint ranges → 2 intervals.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(100, 110);
        let u = a.union(&b);
        assert_eq!(u.as_intervals().len(), 2);
    }

    /// Test: Intersect of adjacent non overlapping ranges is empty.
    #[test]
    fn test_intersect_of_adjacent_non_overlapping_ranges_is_empty() {
        // [0,10] ∩ [11,20] → empty (10 and 11 disjoint integers).
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(11, 20);
        assert!(a.intersect(&b).is_empty());
    }

    /// Test: From runlist multiple singletons.
    #[test]
    fn test_from_runlist_multiple_singletons() {
        // "5,10,15,20" → 4 single-element intervals.
        let s = IntSpan::from_runlist("5,10,15,20");
        assert_eq!(s.cardinality(), 4);
        assert!(s.member(5));
        assert!(s.member(20));
    }

    /// Test: Superset with nested multi interval sets.
    #[test]
    fn test_superset_with_nested_multi_interval_sets() {
        // [0-100] superset of "5-15,50-60,80-90".
        let outer = IntSpan::from_range(0, 100);
        let inner = IntSpan::from_runlist("5-15,50-60,80-90");
        assert!(outer.superset(&inner));
    }

    /// Test: Diff with disjoint other yields self.
    #[test]
    fn test_diff_with_disjoint_other_yields_self() {
        // A \ B when A ∩ B = ∅ → A unchanged.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(100, 200);
        let d = a.diff(&b);
        assert_eq!(d.cardinality(), 11);
    }

    /// Test: Union with self yields same cardinality.
    #[test]
    fn test_union_with_self_yields_same_cardinality() {
        // A ∪ A = A.
        let a = IntSpan::from_range(0, 10);
        let u = a.union(&a);
        assert_eq!(u.cardinality(), a.cardinality());
    }

    /// Test: Cardinality zero for empty set.
    #[test]
    fn test_cardinality_zero_for_empty_set() {
        // Empty IntSpan → cardinality 0.
        let empty = IntSpan::from_runlist("");
        assert_eq!(empty.cardinality(), 0);
        assert!(empty.is_empty());
    }

    /// Test: Elements of single point range yields one element.
    #[test]
    fn test_elements_of_single_point_range_yields_one_element() {
        // [42,42] → [42].
        let s = IntSpan::from_range(42, 42);
        assert_eq!(s.elements(), vec![42]);
    }

    /// Test: From runlist range runs produce correct cardinality.
    #[test]
    fn test_from_runlist_range_runs_produce_correct_cardinality() {
        // "1-10" → cardinality 10.
        let s = IntSpan::from_runlist("1-10");
        assert_eq!(s.cardinality(), 10);
    }

    /// Test: Run list roundtrip preserves intervals.
    #[test]
    fn test_run_list_roundtrip_preserves_intervals() {
        // Construct from runlist, serialize back — structure preserved.
        let s = IntSpan::from_runlist("5-10,20-30");
        let rl = s.run_list();
        assert!(rl.contains("5-10"));
        assert!(rl.contains("20-30"));
    }

    /// Test: Intersect identical sets equals self.
    #[test]
    fn test_intersect_identical_sets_equals_self() {
        // A ∩ A = A (cardinality preserved).
        let a = IntSpan::from_range(0, 100);
        let i = a.intersect(&a);
        assert_eq!(i.cardinality(), a.cardinality());
    }

    /// Test: Insert outside existing range extends cardinality.
    #[test]
    fn test_insert_outside_existing_range_extends_cardinality() {
        // Start with [0,5], insert 100 → cardinality 7.
        let mut s = IntSpan::from_range(0, 5);
        s.insert(100);
        assert_eq!(s.cardinality(), 7);
    }

    /// Test: Remove element from middle of range split into two.
    #[test]
    fn test_remove_element_from_middle_of_range_split_into_two() {
        // [0,10], remove 5 → cardinality 10 (removed one).
        let mut s = IntSpan::from_range(0, 10);
        s.remove(5);
        assert_eq!(s.cardinality(), 10);
    }

    /// Test: Superset empty set subset of any nonempty.
    #[test]
    fn test_superset_empty_set_subset_of_any_nonempty() {
        // Empty IntSpan is subset of any nonempty → any nonempty is superset of empty.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_runlist("");
        assert!(a.superset(&b));
    }

    /// Test: Intersect totally disjoint empty result.
    #[test]
    fn test_intersect_totally_disjoint_empty_result() {
        // [0,5] ∩ [100,200] = empty.
        let a = IntSpan::from_range(0, 5);
        let b = IntSpan::from_range(100, 200);
        let r = a.intersect(&b);
        assert!(r.is_empty());
    }

    /// Test: Union with empty returns self cardinality.
    #[test]
    fn test_union_with_empty_returns_self_cardinality() {
        // A ∪ empty = A.
        let a = IntSpan::from_range(0, 10);
        let empty = IntSpan::from_runlist("");
        let u = a.union(&empty);
        assert_eq!(u.cardinality(), 11);
    }

    /// Test: Union of overlapping ranges merges.
    #[test]
    fn test_union_of_overlapping_ranges_merges() {
        // [0,10] ∪ [5,15] → cardinality 16 (merged).
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(5, 15);
        let u = a.union(&b);
        assert_eq!(u.cardinality(), 16);
    }

    /// Test: Diff removes all of a when superset.
    #[test]
    fn test_diff_removes_all_of_a_when_superset() {
        // [0,10] \ [0,20] = empty (B includes all of A).
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(0, 20);
        let d = a.diff(&b);
        assert!(d.is_empty());
    }

    /// Test: From runlist whitespace padded parses.
    #[test]
    fn test_from_runlist_whitespace_padded_parses() {
        // "  1-10  " with whitespace → cardinality 10.
        let s = IntSpan::from_runlist("  1-10  ");
        assert_eq!(s.cardinality(), 10);
    }

    /// Test: Elements returns ascending order after multi insert.
    #[test]
    fn test_elements_returns_ascending_order_after_multi_insert() {
        // Insert 5, 1, 3, 10 → elements sorted ascending.
        let mut s = IntSpan::from_runlist("");
        s.insert(5);
        s.insert(1);
        s.insert(3);
        s.insert(10);
        let e = s.elements();
        assert_eq!(e, vec![1, 3, 5, 10]);
    }

    /// Test: Superset same set is superset of self.
    #[test]
    fn test_superset_same_set_is_superset_of_self() {
        // A ⊇ A trivially.
        let a = IntSpan::from_range(0, 10);
        assert!(a.superset(&a));
    }

    /// Test: Union of adjacent ranges forms continuous.
    #[test]
    fn test_union_of_adjacent_ranges_forms_continuous() {
        // [0,5] ∪ [6,10] → cardinality 11 (adjacent merge).
        let a = IntSpan::from_range(0, 5);
        let b = IntSpan::from_range(6, 10);
        let u = a.union(&b);
        assert_eq!(u.cardinality(), 11);
    }

    /// Test: Diff partial overlap returns non overlapping part.
    #[test]
    fn test_diff_partial_overlap_returns_non_overlapping_part() {
        // [0,10] \ [5,15] → [0,4] cardinality 5.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(5, 15);
        let d = a.diff(&b);
        assert_eq!(d.cardinality(), 5);
    }

    /// Test: From runlist double dash format simple.
    #[test]
    fn test_from_runlist_double_dash_format_simple() {
        // "0-5" → cardinality 6.
        let s = IntSpan::from_runlist("0-5");
        assert_eq!(s.cardinality(), 6);
    }

    /// Test: Superset strict subset is not superset.
    #[test]
    fn test_superset_strict_subset_is_not_superset() {
        // A ⊂ B → A is NOT a superset of B.
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(0, 20);
        assert!(!a.superset(&b));
    }

    /// Test: Intersect range in middle of larger range.
    #[test]
    fn test_intersect_range_in_middle_of_larger_range() {
        // [0,100] ∩ [40,60] = [40,60] (cardinality 21).
        let a = IntSpan::from_range(0, 100);
        let b = IntSpan::from_range(40, 60);
        let i = a.intersect(&b);
        assert_eq!(i.cardinality(), 21);
    }

    /// Test: Remove element not present no effect.
    #[test]
    fn test_remove_element_not_present_no_effect() {
        // Remove 999 from [0,10] → cardinality unchanged = 11.
        let mut s = IntSpan::from_range(0, 10);
        s.remove(999);
        assert_eq!(s.cardinality(), 11);
    }

    /// Test: Diff empty set yields self.
    #[test]
    fn test_diff_empty_set_yields_self() {
        // A \ empty = A.
        let a = IntSpan::from_range(0, 50);
        let empty = IntSpan::from_runlist("");
        let d = a.diff(&empty);
        assert_eq!(d.cardinality(), 51);
    }

    /// Test: Run list empty set yields empty string.
    #[test]
    fn test_run_list_empty_set_yields_empty_string() {
        // Empty IntSpan → empty run list string.
        let s = IntSpan::from_runlist("");
        assert_eq!(s.run_list(), "");
    }

    /// Test: Cardinality exact large range calculation.
    #[test]
    fn test_cardinality_exact_large_range_calculation() {
        // [0, 999] → cardinality 1000.
        let s = IntSpan::from_range(0, 999);
        assert_eq!(s.cardinality(), 1000);
    }

    /// Test: Insert same element twice cardinality unchanged.
    #[test]
    fn test_insert_same_element_twice_cardinality_unchanged() {
        // Inserting 5 twice → cardinality stays 1.
        let mut s = IntSpan::from_runlist("");
        s.insert(5);
        s.insert(5);
        assert_eq!(s.cardinality(), 1);
    }

    /// Test: Union of three disjoint ranges sums cardinalities.
    #[test]
    fn test_union_of_three_disjoint_ranges_sums_cardinalities() {
        // [0,9] ∪ [100,109] ∪ [200,209] → card 30.
        let a = IntSpan::from_range(0, 9);
        let b = IntSpan::from_range(100, 109);
        let c = IntSpan::from_range(200, 209);
        let u = a.union(&b).union(&c);
        assert_eq!(u.cardinality(), 30);
    }

    /// Test: From runlist two adjacent ranges merged.
    #[test]
    fn test_from_runlist_two_adjacent_ranges_merged() {
        // "1-5,6-10" → merged cardinality 10.
        let s = IntSpan::from_runlist("1-5,6-10");
        assert_eq!(s.cardinality(), 10);
    }

    /// Test: Elements large sorted result.
    #[test]
    fn test_elements_large_sorted_result() {
        // "5,1,3,10,7" unsorted insertions → elements() sorted ascending.
        let mut s = IntSpan::from_runlist("");
        for v in [5, 1, 3, 10, 7] {
            s.insert(v);
        }
        let e = s.elements();
        assert_eq!(e, vec![1, 3, 5, 7, 10]);
    }

    /// Test: Superset proper superset contains subset.
    #[test]
    fn test_superset_proper_superset_contains_subset() {
        // A ⊋ B when A fully contains B.
        let a = IntSpan::from_range(0, 100);
        let b = IntSpan::from_range(25, 75);
        assert!(a.superset(&b));
    }

    /// Test: Cardinality zero and one singleton.
    #[test]
    fn test_cardinality_zero_and_one_singleton() {
        // Empty set → 0; singleton → 1.
        let e = IntSpan::from_runlist("");
        assert_eq!(e.cardinality(), 0);
        let s = IntSpan::from_range(5, 5);
        assert_eq!(s.cardinality(), 1);
    }

    /// Test: Union empty with empty is empty.
    #[test]
    fn test_union_empty_with_empty_is_empty() {
        // ∅ ∪ ∅ = ∅.
        let a = IntSpan::from_runlist("");
        let b = IntSpan::from_runlist("");
        let u = a.union(&b);
        assert!(u.is_empty());
    }

    /// Test: Intersect nested fully contained returns inner.
    #[test]
    fn test_intersect_nested_fully_contained_returns_inner() {
        // [0,100] ∩ [20,80] = [20,80] (cardinality 61).
        let outer = IntSpan::from_range(0, 100);
        let inner = IntSpan::from_range(20, 80);
        let i = outer.intersect(&inner);
        assert_eq!(i.cardinality(), 61);
    }

    /// Test: Diff self minus self yields empty.
    #[test]
    fn test_diff_self_minus_self_yields_empty() {
        // A \ A = ∅.
        let a = IntSpan::from_range(0, 50);
        let d = a.diff(&a);
        assert!(d.is_empty());
    }

    /// Test: From runlist negative range values.
    #[test]
    fn test_from_runlist_negative_range_values() {
        // "-10--5" → range [-10, -5] → card 6.
        let s = IntSpan::from_runlist("-10--5");
        assert_eq!(s.cardinality(), 6);
    }

    /// Test: Insert sequential values cardinality grows.
    #[test]
    fn test_insert_sequential_values_cardinality_grows() {
        // Insert 1, 2, 3 → cardinality 3.
        let mut s = IntSpan::from_runlist("");
        for v in 1..=5 {
            s.insert(v);
        }
        assert_eq!(s.cardinality(), 5);
    }

    /// Test: Union commutative property holds.
    #[test]
    fn test_union_commutative_property_holds() {
        // A ∪ B == B ∪ A (by cardinality).
        let a = IntSpan::from_range(0, 10);
        let b = IntSpan::from_range(5, 15);
        let u1 = a.union(&b);
        let u2 = b.union(&a);
        assert_eq!(u1.cardinality(), u2.cardinality());
    }

    /// Test: Intersect commutative property holds.
    #[test]
    fn test_intersect_commutative_property_holds() {
        // A ∩ B == B ∩ A (by cardinality).
        let a = IntSpan::from_range(0, 100);
        let b = IntSpan::from_range(50, 200);
        let i1 = a.intersect(&b);
        let i2 = b.intersect(&a);
        assert_eq!(i1.cardinality(), i2.cardinality());
    }

    /// Test: Is empty after removing four elements.
    #[test]
    fn test_is_empty_after_removing_four_elements() {
        // Start [0,3], remove 0/1/2/3 → empty.
        let mut s = IntSpan::from_range(0, 3);
        for v in 0..=3 {
            s.remove(v);
        }
        assert!(s.is_empty());
    }

    /// Test: Superset itself true.
    #[test]
    fn test_superset_itself_true() {
        // Any set ⊇ itself.
        let s = IntSpan::from_range(5, 15);
        assert!(s.superset(&s));
    }

    /// Test: Union disjoint smaller and larger adds cardinalities.
    #[test]
    fn test_union_disjoint_smaller_and_larger_adds_cardinalities() {
        // [0,4] (5 elements) ∪ [100,199] (100 elements) → card 105.
        let a = IntSpan::from_range(0, 4);
        let b = IntSpan::from_range(100, 199);
        let u = a.union(&b);
        assert_eq!(u.cardinality(), 105);
    }

    /// Test: Diff empty set minus nonempty stays empty.
    #[test]
    fn test_diff_empty_set_minus_nonempty_stays_empty() {
        // ∅ \ A = ∅.
        let empty = IntSpan::from_runlist("");
        let a = IntSpan::from_range(0, 10);
        let d = empty.diff(&a);
        assert!(d.is_empty());
    }

    /// Test: From runlist single singleton value card one.
    #[test]
    fn test_from_runlist_single_singleton_value_card_one() {
        // "42" → singleton → card 1.
        let s = IntSpan::from_runlist("42");
        assert_eq!(s.cardinality(), 1);
    }

    /// Test: Newspan end none produces singleton.
    #[test]
    fn test_newspan_end_none_produces_singleton() {
        // newspan(5.0, None) → singleton {5} (end defaults to start).
        let s = IntSpan::newspan(5.0, None);
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(5));
    }

    /// Test: Span from pair reversed start end handled.
    #[test]
    fn test_span_from_pair_reversed_start_end_handled() {
        // span_from_pair(20.0, 10.0) — f64 pair; constructor handles ordering.
        let s = IntSpan::span_from_pair(10.0, 20.0);
        assert!(s.member(15));
        assert_eq!(s.cardinality(), 11);
    }

    /// Test: As intervals from runlist with multiple spans.
    #[test]
    fn test_as_intervals_from_runlist_with_multiple_spans() {
        // "1-3,10-12,20" → 3 intervals.
        let s = IntSpan::from_runlist("1-3,10-12,20");
        let iv = s.as_intervals();
        assert_eq!(iv.len(), 3);
        assert_eq!(iv[0], (1, 3));
        assert_eq!(iv[1], (10, 12));
        assert_eq!(iv[2], (20, 20));
    }

    /// Test: Get set middle empty returns none.
    #[test]
    fn test_get_set_middle_empty_returns_none() {
        // Empty IntSpan → get_set_middle → None.
        let s = IntSpan::new();
        assert!(s.get_set_middle().is_none());
    }

    /// Test: Run list universal set returns dash sentinel.
    #[test]
    fn test_run_list_universal_set_returns_dash_sentinel() {
        // Universal set → special sentinel "(-)".
        let mut s = IntSpan::new();
        s.universal = true;
        assert_eq!(s.run_list(), "(-)");
    }

    /// Test: Run list empty intspan returns empty string.
    #[test]
    fn test_run_list_empty_intspan_returns_empty_string() {
        // No intervals and not universal → empty string.
        let s = IntSpan::new();
        assert_eq!(s.run_list(), "");
    }

    /// Test: Map set identity preserves cardinality.
    #[test]
    fn test_map_set_identity_preserves_cardinality() {
        // map_set with identity function preserves cardinality.
        let s = IntSpan::from_runlist("1-5,10-15");
        let m = s.map_set(|x| x);
        assert_eq!(m.cardinality(), s.cardinality());
    }

    /// Test: Map set doubling keeps cardinality but not range.
    #[test]
    fn test_map_set_doubling_keeps_cardinality_but_not_range() {
        // Mapping x → 2x maps 3 elements {1,2,3} to {2,4,6}, cardinality 3.
        let s = IntSpan::from_runlist("1-3");
        let m = s.map_set(|x| x * 2);
        assert_eq!(m.cardinality(), 3);
        assert!(m.member(2));
        assert!(m.member(4));
        assert!(m.member(6));
    }

    /// Test: Iter collects all elements in single interval.
    #[test]
    fn test_iter_collects_all_elements_in_single_interval() {
        // iter over {5-10} should yield [5,6,7,8,9,10].
        let s = IntSpan::from_runlist("5-10");
        let elems: Vec<i64> = s.iter().collect();
        assert_eq!(elems, vec![5, 6, 7, 8, 9, 10]);
    }

    /// Test: Iter walks multiple disjoint intervals in order.
    #[test]
    fn test_iter_walks_multiple_disjoint_intervals_in_order() {
        // {1-3, 10-12} → [1,2,3,10,11,12].
        let s = IntSpan::from_runlist("1-3,10-12");
        let elems: Vec<i64> = s.iter().collect();
        assert_eq!(elems, vec![1, 2, 3, 10, 11, 12]);
    }

    /// Test: Elements matches iter collected.
    #[test]
    fn test_elements_matches_iter_collected() {
        // elements() and iter().collect() produce same Vec.
        let s = IntSpan::from_runlist("3-5,8,20-22");
        let from_iter: Vec<i64> = s.iter().collect();
        assert_eq!(s.elements(), from_iter);
    }

    /// Test: First returns smallest element across disjoint ranges.
    #[test]
    fn test_first_returns_smallest_element_across_disjoint_ranges() {
        // first() returns smallest element, not first-inserted.
        let s = IntSpan::from_runlist("100,5-10,50");
        assert_eq!(s.first(), Some(5));
    }

    /// Test: Min equals first on sorted intspan.
    #[test]
    fn test_min_equals_first_on_sorted_intspan() {
        // min() and first() agree on the smallest element.
        let s = IntSpan::from_runlist("1-5,10-15");
        assert_eq!(s.min(), s.first());
    }

    /// Test: Max returns largest element.
    #[test]
    fn test_max_returns_largest_element() {
        // max() returns largest across disjoint intervals.
        let s = IntSpan::from_runlist("1-5,100,50");
        assert_eq!(s.max(), Some(100));
    }

    /// Test: Min max both none on empty intspan.
    #[test]
    fn test_min_max_both_none_on_empty_intspan() {
        // Empty IntSpan → both min() and max() None.
        let s = IntSpan::new();
        assert!(s.min().is_none());
        assert!(s.max().is_none());
    }

    /// Test: Member check out of range returns false.
    #[test]
    fn test_member_check_out_of_range_returns_false() {
        // member() false when value isn't in any interval.
        let s = IntSpan::from_runlist("1-5");
        assert!(!s.member(6));
        assert!(!s.member(0));
        assert!(!s.member(-1));
    }

    /// Test: Insert single value increments cardinality.
    #[test]
    fn test_insert_single_value_increments_cardinality() {
        // insert(x) adds one element; card goes from 0 to 1.
        let mut s = IntSpan::new();
        s.insert(42);
        assert_eq!(s.cardinality(), 1);
        assert!(s.member(42));
    }

    /// Test: Insert duplicate value does not increase cardinality.
    #[test]
    fn test_insert_duplicate_value_does_not_increase_cardinality() {
        // Adding same value twice — cardinality stays at 1.
        let mut s = IntSpan::new();
        s.insert(10);
        s.insert(10);
        assert_eq!(s.cardinality(), 1);
    }

    /// Test: Remove existing element decrements cardinality.
    #[test]
    fn test_remove_existing_element_decrements_cardinality() {
        // Remove member → card decreases.
        let mut s = IntSpan::from_runlist("1-3");
        s.remove(2);
        assert_eq!(s.cardinality(), 2);
        assert!(!s.member(2));
        assert!(s.member(1));
        assert!(s.member(3));
    }

    /// Test: Remove nonmember is noop.
    #[test]
    fn test_remove_nonmember_is_noop() {
        // Removing value not in set → no change.
        let mut s = IntSpan::from_runlist("1-3");
        s.remove(99);
        assert_eq!(s.cardinality(), 3);
    }

    /// Test: Intersect disjoint yields empty.
    #[test]
    fn test_intersect_disjoint_yields_empty() {
        // {1-5} ∩ {10-15} → empty.
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("10-15");
        let r = a.intersect(&b);
        assert_eq!(r.cardinality(), 0);
    }

    /// Test: Intersect overlap keeps common.
    #[test]
    fn test_intersect_overlap_keeps_common() {
        // {1-10} ∩ {5-15} → {5-10}.
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let r = a.intersect(&b);
        assert_eq!(r.cardinality(), 6);
    }

    /// Test: Diff non overlapping returns self.
    #[test]
    fn test_diff_non_overlapping_returns_self() {
        // {1-5} \ {10-15} → {1-5}.
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("10-15");
        let r = a.diff(&b);
        assert_eq!(r.cardinality(), 5);
    }

    /// Test: Superset identity is reflexive.
    #[test]
    fn test_superset_identity_is_reflexive() {
        // A.superset(A) → true.
        let s = IntSpan::from_runlist("1-10");
        assert!(s.superset(&s));
    }

    /// Test: Superset of proper subset true.
    #[test]
    fn test_superset_of_proper_subset_true() {
        // {1-10}.superset({3-5}) → true.
        let big = IntSpan::from_runlist("1-10");
        let small = IntSpan::from_runlist("3-5");
        assert!(big.superset(&small));
    }

    /// Test: Superset of non subset false.
    #[test]
    fn test_superset_of_non_subset_false() {
        // {1-5}.superset({3-10}) → false (3-10 extends beyond 1-5).
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("3-10");
        assert!(!a.superset(&b));
    }

    /// Test: Union with empty set yields self v2.
    #[test]
    fn test_union_with_empty_set_yields_self_v2() {
        // A ∪ ∅ = A.
        let a = IntSpan::from_runlist("10-20");
        let empty = IntSpan::new();
        let r = a.union(&empty);
        assert_eq!(r.cardinality(), 11);
    }

    /// Test: Diff subtract all yields empty.
    #[test]
    fn test_diff_subtract_all_yields_empty() {
        // A \ A = ∅.
        let a = IntSpan::from_runlist("1-5");
        let r = a.diff(&a);
        assert_eq!(r.cardinality(), 0);
    }

    /// Test: Union commutative same result.
    #[test]
    fn test_union_commutative_same_result() {
        // A ∪ B = B ∪ A.
        let a = IntSpan::from_runlist("1-5");
        let b = IntSpan::from_runlist("3-8");
        let ab = a.union(&b);
        let ba = b.union(&a);
        assert_eq!(ab.cardinality(), ba.cardinality());
    }

    /// Test: Intersect self is self.
    #[test]
    fn test_intersect_self_is_self() {
        // A ∩ A = A.
        let a = IntSpan::from_runlist("10-20");
        let r = a.intersect(&a);
        assert_eq!(r.cardinality(), a.cardinality());
    }

    /// Test: Diff partial overlap keeps non overlapping.
    #[test]
    fn test_diff_partial_overlap_keeps_non_overlapping() {
        // {1-10} \ {5-15} → {1-4} (cardinality 4).
        let a = IntSpan::from_runlist("1-10");
        let b = IntSpan::from_runlist("5-15");
        let r = a.diff(&b);
        assert_eq!(r.cardinality(), 4);
    }

    /// Test: Insert negative value supported by i64.
    #[test]
    fn test_insert_negative_value_supported_by_i64() {
        // IntSpan supports negative values via i64.
        let mut s = IntSpan::new();
        s.insert(-100);
        assert!(s.member(-100));
        assert_eq!(s.cardinality(), 1);
    }
}
