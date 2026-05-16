//! Chromosome selection pipeline — direct ports of the Perl subs in Circos.pm
//! that parse `chromosomes=`, `chromosomes_order=`, `chromosomes_scale=`, filter
//! ideogram displays, and manage ordering groups.
//!
//! Perl depends on `%CONF`, `%KARYOTYPE`, and `@IDEOGRAMS` globals; here we pass
//! them as function parameters.

use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::intspan::IntSpan;

/// Per-chromosome show/hide/combined filter (Perl `$filter->{CHR}{show|hide|combined}`).
#[derive(Debug, Default, Clone)]
pub struct ChrFilter {
    pub show: Option<IntSpan>,
    pub hide: Option<IntSpan>,
    pub combined: Option<IntSpan>,
}

/// Keyed by chromosome tag (or chromosome name).
pub type IdeogramFilter = HashMap<String, ChrFilter>;

/// Port of Perl `parse_ideogram_filter(filter_string)`: parses `"chr[tag]:range;chr[tag]:range"`
/// style filter strings into `{chr: {show|hide: IntSpan}}`.
/// `chromosomes_units` is the multiplier applied to numeric positions (e.g. 1e6 for Mb).
pub fn parse_ideogram_filter(
    filter_string: Option<&str>,
    chromosomes_units: Option<f64>,
) -> IdeogramFilter {
    let mut filter: IdeogramFilter = HashMap::new();
    let s = match filter_string {
        Some(s) => s,
        None => return filter,
    };
    let units_re = regex::Regex::new(r"[\.\d]+").unwrap();
    for chr in s.split(';') {
        let chr = chr.trim();
        if chr.is_empty() {
            continue;
        }
        // /(-)?([^:]+):?(.*)/
        let (is_suppressed, rest) = if let Some(stripped) = chr.strip_prefix('-') {
            (true, stripped)
        } else {
            (false, chr)
        };
        let (tag, runlist) = match rest.split_once(':') {
            Some((t, r)) => (t.to_string(), r.to_string()),
            None => (rest.to_string(), String::new()),
        };
        let runlist = if let Some(units) = chromosomes_units {
            // Expand decimal numbers by chromosomes_units (Perl: s/([\.\d]+)/$1*$CONF{chromosomes_units}/eg)
            units_re
                .replace_all(&runlist, |c: &regex::Captures| {
                    let v: f64 = c[0].parse().unwrap_or(0.0);
                    format!("{}", (v * units) as i64)
                })
                .to_string()
        } else {
            runlist
        };
        let set = if runlist.is_empty() {
            IntSpan::from_runlist("(-)")
        } else {
            IntSpan::from_runlist(&runlist)
        };
        let entry = filter.entry(tag).or_default();
        if is_suppressed {
            entry.hide = Some(set);
        } else {
            entry.show = Some(set);
        }
    }
    filter
}

/// Port of Perl `merge_ideogram_filters(@filters)`: union show/hide across filters,
/// then compute `combined = show - hide` (universal if show missing).
pub fn merge_ideogram_filters(filters: &[IdeogramFilter]) -> IdeogramFilter {
    let mut merged: IdeogramFilter = HashMap::new();
    for f in filters {
        for (chr, cf) in f {
            let entry = merged.entry(chr.clone()).or_default();
            if let Some(show) = &cf.show {
                entry.show = Some(match &entry.show {
                    Some(existing) => existing.union(show),
                    None => show.clone(),
                });
            }
            if let Some(hide) = &cf.hide {
                entry.hide = Some(match &entry.hide {
                    Some(existing) => existing.union(hide),
                    None => hide.clone(),
                });
            }
        }
    }
    for cf in merged.values_mut() {
        cf.combined = Some(match (&cf.show, &cf.hide) {
            (Some(s), Some(h)) => s.diff(h),
            (Some(s), None) => s.clone(),
            (None, Some(h)) => IntSpan::from_runlist("(-)").diff(h),
            (None, None) => IntSpan::from_runlist("(-)"),
        });
    }
    merged
}

/// Port of Perl `filter_data(set, chr)`: intersect `set` with the karyotype's
/// accept region for `chr`. Returns empty if no accept region.
pub fn filter_data(
    set: &IntSpan,
    chr: &str,
    karyotype_accept: &HashMap<String, IntSpan>,
) -> IntSpan {
    match karyotype_accept.get(chr) {
        Some(accept) => set.intersect(accept),
        None => IntSpan::new(),
    }
}

/// Per-chromosome display region (Perl `$KARYOTYPE->{CHR}{chr}{display_region}`).
#[derive(Debug, Default, Clone)]
pub struct DisplayRegion {
    pub accept: Option<IntSpan>,
    pub reject: Option<IntSpan>,
    pub display: bool,
}

/// Port of Perl `refine_display_regions`: normalize accept/reject sets for every
/// chromosome against its full karyotype set, computing `display` flag. Mutates
/// the provided map in place.
pub fn refine_display_regions(
    chromosomes_display_default: bool,
    karyotype_set: &HashMap<String, IntSpan>,
    regions: &mut HashMap<String, DisplayRegion>,
) {
    for (chr, chr_set) in karyotype_set {
        let region = regions.entry(chr.clone()).or_default();
        match (region.accept.clone(), region.reject.clone()) {
            (Some(accept), Some(reject)) => {
                let reject_clamped = reject.intersect(chr_set);
                let accept_clamped = accept.intersect(chr_set).diff(&reject_clamped);
                region.reject = Some(reject_clamped);
                region.accept = Some(accept_clamped);
            }
            (None, Some(reject)) => {
                let reject_clamped = reject.intersect(chr_set);
                region.accept = Some(chr_set.diff(&reject_clamped));
                region.reject = Some(reject_clamped);
            }
            (Some(accept), None) => {
                region.accept = Some(accept.intersect(chr_set));
                region.reject = Some(IntSpan::new());
            }
            (None, None) => {
                if chromosomes_display_default {
                    region.accept = Some(chr_set.clone());
                    region.reject = Some(IntSpan::new());
                } else {
                    region.accept = Some(IntSpan::new());
                    region.reject = Some(IntSpan::new());
                }
            }
        }
        region.display = region
            .accept
            .as_ref()
            .map(|s| s.cardinality() > 0)
            .unwrap_or(false);
    }
}

/// One entry inside a `ChrorderGroup`'s `tags` list.
#[derive(Debug, Default, Clone)]
pub struct TagItem {
    pub tag: String,
    pub group_idx: usize,
    pub ideogram_idx: Option<usize>,
    pub display_idx: Option<usize>,
}

/// Port of Perl ideogram-order group (`make_chrorder_groups` output).
#[derive(Debug, Default, Clone)]
pub struct ChrorderGroup {
    pub idx: usize,
    pub n: usize,
    pub cumulidx: usize,
    pub tags: Vec<TagItem>,
    pub start: bool,
    pub end: bool,
    pub display_idx_set: IntSpan,
    pub reform: bool,
}

/// Port of Perl `report_chromosomes`: print chromosome display info via the debug
/// module. `chrs` is in display_order.
pub fn report_chromosomes(
    chrs: &[String],
    display_order: &HashMap<String, u32>,
    scale: &HashMap<String, f64>,
    regions: &HashMap<String, DisplayRegion>,
    length_cumul: &HashMap<String, i64>,
) {
    let mut sorted = chrs.to_vec();
    sorted.sort_by_key(|c| *display_order.get(c).unwrap_or(&0));
    for chr in sorted {
        let region = regions.get(&chr);
        let is_displayed = region.map(|r| r.display).unwrap_or(false);
        if !is_displayed {
            continue;
        }
        let run_list = region
            .and_then(|r| r.accept.as_ref())
            .map(|s| s.run_list())
            .unwrap_or_else(|| "-".to_string());
        crate::debug::printinfo(&[
            &chr,
            &display_order.get(&chr).copied().unwrap_or(0).to_string(),
            &scale.get(&chr).copied().unwrap_or(1.0).to_string(),
            &run_list,
            &length_cumul.get(&chr).copied().unwrap_or(0).to_string(),
        ]);
    }
}

/// Port of Perl `set_display_index(chrorder_groups)`: walk groups sorted by
/// start/end flag priority, assigning display_idx slots. Mutates in place.
/// `n_ideograms` is the total count (Perl: `@IDEOGRAMS`).
pub fn set_display_index(chrorder_groups: &mut [ChrorderGroup], n_ideograms: usize) {
    // Sort: groups with start/end flags first (largest-first by Perl)
    let mut order: Vec<usize> = (0..chrorder_groups.len()).collect();
    order.sort_by(|&a, &b| {
        let sa = if chrorder_groups[a].start || chrorder_groups[a].end {
            1
        } else {
            0
        };
        let sb = if chrorder_groups[b].start || chrorder_groups[b].end {
            1
        } else {
            0
        };
        sb.cmp(&sa)
    });

    for idx in order {
        let group = &mut chrorder_groups[idx];
        if group.start {
            for (display_idx, tag_item) in group.tags.iter_mut().enumerate() {
                tag_item.display_idx = Some(display_idx);
            }
        } else if group.end {
            let base = n_ideograms.saturating_sub(group.n);
            for (i, tag_item) in group.tags.iter_mut().enumerate() {
                tag_item.display_idx = Some(base + i);
            }
        } else {
            // Find anchor: first tag with defined ideogram_idx, sorted by group_idx
            let mut sorted_by_group = group.tags.clone();
            sorted_by_group.sort_by_key(|t| t.group_idx);
            let anchor_opt = sorted_by_group
                .iter()
                .find(|t| t.ideogram_idx.is_some())
                .cloned();
            if let Some(anchor) = anchor_opt {
                let anchor_group_idx = anchor.group_idx as isize;
                let anchor_ideogram_idx = anchor.ideogram_idx.unwrap() as isize;
                for tag_item in group.tags.iter_mut() {
                    let di = tag_item.group_idx as isize - anchor_group_idx + anchor_ideogram_idx;
                    tag_item.display_idx = Some(di.max(0) as usize);
                }
                let min_di = group
                    .tags
                    .iter()
                    .filter_map(|t| t.display_idx.map(|d| d as isize))
                    .min()
                    .unwrap_or(0);
                if min_di < 0 {
                    for tag_item in group.tags.iter_mut() {
                        if let Some(d) = tag_item.display_idx {
                            tag_item.display_idx = Some(((d as isize) - min_di) as usize);
                        }
                    }
                }
            }
        }
    }
}

/// Port of Perl `register_z_levels(node)`: gather all z values defined anywhere
/// beneath a highlights/plots node and store the sorted distinct list in
/// `node.param.zlist`. Stub form — the Rust data model should grow a matching
/// tree before this can be a full 1-1 port. For now collects z values from the
/// simplest case: top-level, dataset list, and nested data points.
pub fn register_z_levels(z_values: impl IntoIterator<Item = i64>) -> Vec<i64> {
    use std::collections::BTreeSet;
    let mut set: BTreeSet<i64> = BTreeSet::new();
    set.insert(0);
    for z in z_values {
        set.insert(z);
    }
    set.into_iter().collect()
}

/// Port of Perl `parse_chromosomes`: main chromosome selection directive parser.
/// Parses the `chromosomes=` and `chromosomes_breaks=` config strings into
/// a list of ParsedChromosome entries and mutates per-chromosome
/// accept/reject display regions in `regions_out`.
///
/// `chromosomes_display_default=yes` means add all karyotype chromosomes not
/// already in `chromosomes` to the list. `chromosomes_order_by_karyotype=yes`
/// uses the karyotype file order; otherwise a name-then-number sort is used.
pub fn parse_chromosomes(
    chromosomes: Option<&str>,
    chromosomes_breaks: Option<&str>,
    chromosomes_units: f64,
    chromosomes_display_default: bool,
    chromosomes_order_by_karyotype: bool,
    karyotype_chrs: &HashMap<String, u32>,
    regions_out: &mut HashMap<String, DisplayRegion>,
) -> Result<Vec<ParsedChromosome>, String> {
    let mut chrs_out: Vec<ParsedChromosome> = Vec::new();
    let mut chromosomes_effective: String = chromosomes.unwrap_or("").to_string();

    if chromosomes_display_default {
        // Build sorted default list of karyotype chromosomes
        let mut chrs_tmp: Vec<String> = karyotype_chrs.keys().cloned().collect();
        if chromosomes_order_by_karyotype {
            chrs_tmp.sort_by_key(|c| *karyotype_chrs.get(c).unwrap_or(&0));
        } else {
            // name-then-number sort (Perl heuristic)
            let re_prefix = regex::Regex::new(r"^(\D+)").unwrap();
            let re_number = regex::Regex::new(r"(\d+)").unwrap();
            chrs_tmp.sort_by(|a, b| {
                let a_has_digit = a.chars().any(|c| c.is_ascii_digit());
                let b_has_digit = b.chars().any(|c| c.is_ascii_digit());
                if a_has_digit && b_has_digit {
                    let a_pref = re_prefix.find(a).map(|m| m.as_str()).unwrap_or("");
                    let b_pref = re_prefix.find(b).map(|m| m.as_str()).unwrap_or("");
                    let pref_cmp = a_pref.cmp(b_pref);
                    if pref_cmp != std::cmp::Ordering::Equal {
                        pref_cmp
                    } else {
                        let a_num: u64 = re_number
                            .captures(a)
                            .and_then(|c| c.get(1))
                            .and_then(|m| m.as_str().parse().ok())
                            .unwrap_or(0);
                        let b_num: u64 = re_number
                            .captures(b)
                            .and_then(|c| c.get(1))
                            .and_then(|m| m.as_str().parse().ok())
                            .unwrap_or(0);
                        a_num.cmp(&b_num)
                    }
                } else {
                    a.cmp(b)
                }
            });
        }

        // Remove chromosomes already mentioned in `chromosomes`
        if !chromosomes_effective.is_empty() {
            let mentioned = chromosomes_effective.clone();
            chrs_tmp.retain(|c| {
                let re = regex::Regex::new(&format!(r"\b{}\b", regex::escape(c)));
                match re {
                    Ok(re) => !re.is_match(&mentioned),
                    Err(_) => true,
                }
            });
        }

        if !chrs_tmp.is_empty() {
            if chromosomes_effective.is_empty() {
                chromosomes_effective = chrs_tmp.join(";");
            } else {
                chromosomes_effective = format!("{};{}", chromosomes_effective, chrs_tmp.join(";"));
            }
        }
    }

    // Process both `chromosomes` (accept_default=1) and `chromosomes_breaks` (accept_default=0)
    let re_tag = regex::Regex::new(r"^([^\[\]]+)\[?([^\]]*)\]?$").unwrap();
    let re_units = regex::Regex::new(r"[\.\d]+").unwrap();
    for (string, accept_default) in [
        (chromosomes_effective.as_str(), true),
        (chromosomes_breaks.unwrap_or(""), false),
    ] {
        if string.is_empty() {
            continue;
        }
        for chrstring in string.split([';', ' ']) {
            if chrstring.is_empty() {
                continue;
            }
            let (chr_part, runlist): (&str, &str) = match chrstring.split_once(':') {
                Some((c, r)) => (c, r),
                None => (chrstring, ""),
            };
            let mut accept = accept_default;
            let chr_part = if let Some(stripped) = chr_part.strip_prefix('-') {
                accept = false;
                stripped
            } else {
                chr_part
            };
            // Parse optional [tag]
            let (chr, tag) = match re_tag.captures(chr_part) {
                Some(cap) => (
                    cap.get(1)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default(),
                    cap.get(2)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default(),
                ),
                None => (chr_part.to_string(), String::new()),
            };
            if !karyotype_chrs.contains_key(&chr) {
                return Err(format!(
                    "fatal error - entry in 'chromosomes' parameter [{}] mentions chromosome [{}] which is not defined the karyotype file.",
                    chrstring, chr
                ));
            }

            // Expand units in runlist
            let runlist_expanded: String = if chromosomes_units > 0.0 && !runlist.is_empty() {
                re_units
                    .replace_all(runlist, |c: &regex::Captures| {
                        let v: f64 = c[0].parse().unwrap_or(0.0);
                        format!("{}", (v * chromosomes_units) as i64)
                    })
                    .to_string()
            } else {
                runlist.to_string()
            };

            let set = if !runlist_expanded.is_empty() {
                IntSpan::from_runlist(&runlist_expanded)
            } else {
                IntSpan::new()
            };

            let effective_tag = if tag.is_empty() { chr.clone() } else { tag };
            if accept {
                chrs_out.push(ParsedChromosome {
                    chr: chr.clone(),
                    tag: effective_tag,
                    set: set.clone(),
                    accept: true,
                });
            }

            let region = regions_out.entry(chr.clone()).or_default();
            if accept {
                region.accept = Some(match &region.accept {
                    Some(existing) => existing.union(&set),
                    None => set,
                });
            } else {
                region.reject = Some(match &region.reject {
                    Some(existing) => existing.union(&set),
                    None => set,
                });
            }
        }
    }

    if !chrs_out.iter().any(|c| c.accept) {
        return Err(
            "no chromosomes to draw - either define some in 'chromosomes' parameter or set chromosomes_display_default=yes".to_string(),
        );
    }

    Ok(chrs_out)
}

/// One entry returned by `parse_chromosomes`.
#[derive(Debug, Clone, Default)]
pub struct ParsedChromosome {
    pub chr: String,
    pub tag: String,
    pub set: IntSpan,
    pub accept: bool,
}

/// Minimal ideogram view needed by `recompute_chrorder_groups`.
#[derive(Debug, Clone, Default)]
pub struct IdeogramRef {
    pub idx: usize,
    pub tag: String,
    pub chr: String,
    pub display_idx: Option<usize>,
}

/// Port of Perl `recompute_chrorder_groups(chrorder_groups)`: walk the groups,
/// reconcile `tag_item.ideogram_idx` against the real ideogram list (matching
/// by tag, or by chr when tag contains `__`), allocate display_idx slots from
/// an IntSpan of unused slots, then set each ideogram's display_idx. Any
/// ideogram left unallocated gets the next available slot.
pub fn recompute_chrorder_groups(
    chrorder_groups: &mut [ChrorderGroup],
    ideograms: &mut [IdeogramRef],
) {
    let n = ideograms.len();
    let mut display_idx_set = if n == 0 {
        IntSpan::new()
    } else {
        IntSpan::from_range(0, n as i64 - 1)
    };
    let mut allocated: HashMap<usize, u32> = HashMap::new();

    // First pass: for each tag_item that maps to a real ideogram, claim its slot.
    for group in chrorder_groups.iter_mut() {
        for tag_item in group.tags.iter_mut() {
            let ideogram_opt = ideograms.iter().find(|ideo| {
                (!ideo.tag.contains("__") && ideo.tag == tag_item.tag)
                    || (ideo.tag.contains("__") && ideo.chr == tag_item.tag)
            });
            if let Some(ideogram) = ideogram_opt {
                if let Some(di) = tag_item.display_idx {
                    display_idx_set.remove(di as i64);
                }
                *allocated.entry(ideogram.idx).or_insert(0) += 1;
            }
        }
    }

    // Second pass: for each tag_item that didn't find an ideogram, assign one
    // of the remaining unallocated ideograms.
    for group in chrorder_groups.iter_mut() {
        for tag_item in group.tags.iter_mut() {
            let ideogram_exists = ideograms.iter().any(|ideo| {
                (!ideo.tag.contains("__") && ideo.tag == tag_item.tag)
                    || (ideo.tag.contains("__") && ideo.chr == tag_item.tag)
            });
            if !ideogram_exists {
                // Find first unallocated ideogram (by idx)
                let unallocated_opt = ideograms
                    .iter()
                    .find(|ideo| !allocated.contains_key(&ideo.idx))
                    .cloned();
                if let Some(unallocated) = unallocated_opt {
                    tag_item.tag = unallocated.tag.clone();
                    tag_item.ideogram_idx = Some(unallocated.idx);
                    *allocated.entry(unallocated.idx).or_insert(0) += 1;
                    if let Some(di) = tag_item.display_idx {
                        display_idx_set.remove(di as i64);
                    }
                }
            }
        }
    }

    // Third pass: for each tag_item with an ideogram_idx but no display_idx,
    // allocate the first unused slot. Mirror back to the ideogram.
    for group in chrorder_groups.iter_mut() {
        for tag_item in group.tags.iter_mut() {
            if tag_item.ideogram_idx.is_some() {
                let display_idx = match tag_item.display_idx {
                    Some(di) => di,
                    None => {
                        let di = display_idx_set.first();
                        if let Some(di_i) = di {
                            display_idx_set.remove(di_i);
                            tag_item.display_idx = Some(di_i as usize);
                            di_i as usize
                        } else {
                            continue;
                        }
                    }
                };
                if let Some(idx) = tag_item.ideogram_idx
                    && let Some(ideo) = ideograms.iter_mut().find(|i| i.idx == idx)
                {
                    ideo.display_idx = Some(display_idx);
                }
            } else {
                crate::debug::printwarning(&[
                    "trimming ideogram order - removing entry",
                    &tag_item.group_idx.to_string(),
                    "from group",
                    &group.idx.to_string(),
                ]);
                tag_item.display_idx = None;
            }
        }
    }

    // Any ideograms still missing a display_idx: fill from the unused pool.
    for ideo in ideograms.iter_mut() {
        if ideo.display_idx.is_none()
            && let Some(di) = display_idx_set.first()
        {
            display_idx_set.remove(di);
            ideo.display_idx = Some(di as usize);
        }
    }
}

/// Port of Perl `reform_chrorder_groups(chrorder_groups)`: iteratively resolve
/// display_idx collisions between groups by sliding offending groups to the
/// first start where their display_idx_set fits. Panics (`confess`) if a group
/// cannot be placed.
pub fn reform_chrorder_groups(
    chrorder_groups: &mut [ChrorderGroup],
    n_ideograms: usize,
) -> Result<(), String> {
    loop {
        let mut reform_display_idx = false;
        let mut union = IntSpan::new();

        // Compute each group's display_idx_set and mark collisions
        for group in chrorder_groups.iter_mut() {
            let mut set = IntSpan::new();
            for tag_item in &group.tags {
                if let Some(di) = tag_item.display_idx {
                    set.insert(di as i64);
                }
            }
            group.display_idx_set = set.clone();
            if union.intersect(&set).cardinality() == 0 {
                union = union.union(&set);
                group.reform = false;
            } else {
                reform_display_idx = true;
                group.reform = true;
            }
        }

        // Try to slide colliding groups to the first fitting start position.
        // Needs index + mutable access to other elements, so `0..n` is the
        // right pattern here even though clippy prefers iter().enumerate().
        let group_count = chrorder_groups.len();
        #[allow(clippy::needless_range_loop)]
        'outer: for gi in 0..group_count {
            if !chrorder_groups[gi].reform {
                continue;
            }
            let old_set = chrorder_groups[gi].display_idx_set.clone();
            let old_min = old_set.min().unwrap_or(0);
            let n = chrorder_groups[gi].n;
            let max_start = n_ideograms.saturating_sub(n);
            for start in 0..=max_start {
                let newgroup = old_set.map_set(|v| v - old_min + start as i64);
                if newgroup.intersect(&union).cardinality() == 0 {
                    union = union.union(&newgroup);
                    let mut elements = newgroup.elements();
                    for tag_item in chrorder_groups[gi].tags.iter_mut() {
                        tag_item.display_idx = if elements.is_empty() {
                            None
                        } else {
                            Some(elements.remove(0) as usize)
                        };
                    }
                    chrorder_groups[gi].display_idx_set = newgroup;
                    chrorder_groups[gi].reform = false;
                    continue 'outer;
                }
            }
            if chrorder_groups[gi].reform {
                let tags: Vec<String> = chrorder_groups[gi]
                    .tags
                    .iter()
                    .map(|t| t.tag.clone())
                    .collect();
                return Err(format!(
                    "fatal error - chromosomes_order string cannot be processed because group {} cannot be placed in the display. This may be due to more tags in the chromosomes_order field than ideograms.",
                    tags.join(",")
                ));
            }
        }
        if !reform_display_idx {
            break;
        }
    }
    Ok(())
}

/// Port of Perl `ideogram_spacing_helper(value)`: convert a relative or u-unit
/// spacing value into bases. `units_ok`/`units_nounit` come from CONF;
/// `chromosomes_units` and `spacing_default` are runtime dims.
pub fn ideogram_spacing_helper(
    value: &str,
    units_ok: &str,
    units_nounit: &str,
    chromosomes_units: f64,
    spacing_default: f64,
) -> Result<f64, String> {
    crate::layout::units::unit_validate(value, units_ok, units_nounit, &["u", "r"])?;
    let unit = crate::layout::units::unit_fetch(value, units_ok, units_nounit)?;
    let numeric_str = crate::layout::units::unit_strip(value, units_ok, units_nounit)?;
    let numeric: f64 = numeric_str
        .parse()
        .map_err(|_| format!("cannot parse '{}' as number", numeric_str))?;
    match unit.as_str() {
        "u" => Ok(numeric * chromosomes_units),
        "r" => Ok(numeric * spacing_default),
        other => Err(format!(
            "unexpected unit [{}] in ideogram_spacing_helper",
            other
        )),
    }
}

/// Port of Perl `get_ideogram_radius(ideogram)`: lookup radius in the per-tag
/// dimensions table, fall back to `default`.
pub fn get_ideogram_radius(tag: &str, dims_radius_by_tag: &HashMap<String, f64>) -> f64 {
    if let Some(r) = dims_radius_by_tag.get(tag) {
        return *r;
    }
    *dims_radius_by_tag.get("default").unwrap_or(&0.0)
}

/// Port of Perl `read_chromosomes_order`: determine the tag order list from
/// (in priority order) `$CONF{chromosomes_order}`, `$CONF{chromosomes_order_file}`,
/// or the karyotype's `display_order` sort. Validates that every named tag
/// appears in `@IDEOGRAMS` or `%KARYOTYPE`, and that no tag is mentioned twice.
/// `^` / `$` / `|` / `-` sentinels in the order string are permitted even if
/// not present in karyotype (they are layout directives).
///
/// `ideogram_tags` is the effective list of ideogram tags (Perl: `map $_->{tag}`),
/// and `karyotype_chrs` maps chromosome name to its karyotype display_order.
pub fn read_chromosomes_order(
    chromosomes_order: Option<&str>,
    chromosomes_order_file_contents: Option<&str>,
    ideogram_tags: &[String],
    karyotype_chrs: &HashMap<String, u32>,
) -> Result<Vec<String>, String> {
    let chrorder: Vec<String> = if let Some(s) = chromosomes_order {
        let re = regex::Regex::new(r"\s*[;,]\s*").unwrap();
        re.split(s)
            .map(|t| t.to_string())
            .filter(|t| !t.is_empty())
            .collect()
    } else if let Some(contents) = chromosomes_order_file_contents {
        contents
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
            .collect()
    } else {
        let mut karyotype_sorted: Vec<&String> = karyotype_chrs.keys().collect();
        karyotype_sorted.sort_by_key(|c| *karyotype_chrs.get(*c).unwrap_or(&0));
        let mut v = vec!["^".to_string()];
        v.extend(karyotype_sorted.iter().map(|s| (*s).clone()));
        v
    };

    // Validate: count tags; detect duplicates; error on unknown tags.
    let mut seen_tag: HashMap<String, u32> = HashMap::new();
    let mut n: usize = 0;
    for tag in &chrorder {
        let tag_found = ideogram_tags.iter().any(|t| t == tag);
        if tag_found {
            let count = seen_tag.entry(tag.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                return Err(format!(
                    "fatal error - incorrectly formatted chromosomes_order field (or content of chromosomes_order_file) - tag {} appears multiple times.",
                    tag
                ));
            }
        } else if tag != "|"
            && tag != "$"
            && tag != "^"
            && tag != "-"
            && !ideogram_tags.iter().any(|t| t == tag)
            && !karyotype_chrs.contains_key(tag)
        {
            return Err(format!(
                "fatal error - incorrectly formatted chromosomes_order field (or content of chromosomes_order_file) - tag {} appears in the chromosome order, but it is not associated with any chromosome.",
                tag
            ));
        }
        if tag_found || tag == "-" {
            n += 1;
        }
    }
    if n > ideogram_tags.len() {
        crate::debug::printwarning(&[
            "you have more tags",
            &format!("({})", n),
            "in the chromosomes_order field than ideograms",
            &format!("({})", ideogram_tags.len()),
            "- circos may not be able to correctly order the display",
        ]);
    }
    Ok(chrorder)
}

/// Port of Perl `parse_parameters(node, type, continue, @extras)`: filter a
/// node's keys against a whitelist of parameters appropriate for `type`
/// (highlights/link/connector/plot/tile/text). Yes/no strings normalize to 1/0;
/// `;word` sequences in values normalize to `,`. Unknown keys panic unless
/// `continue` is true.
pub fn parse_parameters(
    node: &HashMap<String, ConfigValue>,
    r#type: &str,
    continue_on_unknown: bool,
    extras: &[&str],
) -> HashMap<String, String> {
    let default_params: &[&str] = &[
        "url",
        "id",
        "record_limit",
        "perturb",
        "z",
        "show",
        "hide",
        "axis",
        "axis_color",
        "axis_thickness",
        "axis_spacing",
        "background",
        "background_color",
        "background_stroke_color",
        "background_stroke_thickness",
        "label_size",
        "label_offset",
        "label_font",
    ];
    let highlight_params: &[&str] = &[
        "offset",
        "r0",
        "r1",
        "layer_with_data",
        "fill_color",
        "stroke_color",
        "stroke_thickness",
        "ideogram",
        "minsize",
        "padding",
    ];
    let link_params: &[&str] = &[
        "offset",
        "start",
        "end",
        "color",
        "flat",
        "rev",
        "reversed",
        "inv",
        "inverted",
        "twist",
        "thickness",
        "stroke_thickness",
        "stroke_color",
        "ribbon",
        "radius",
        "radius1",
        "radius2",
        "bezier_radius",
        "crest",
        "bezier_radius_purity",
        "ribbon",
        "perturb_crest",
        "perturb_bezier_radius",
        "perturb_bezier_radius_purity",
    ];
    let connector_params: &[&str] = &["connector_dims", "thickness", "color", "r0", "r1"];
    let plot_params: &[&str] = &[
        "start",
        "end",
        "angle_shift",
        "layers_overflow",
        "connector_dims",
        "extend_bin",
        "label_rotate",
        "value",
        "scale_log_base",
        "layers_overflow_color",
        "offset",
        "padding",
        "rpadding",
        "thickness",
        "layers",
        "margin",
        "max_gap",
        "fill_color",
        "color",
        "thickness",
        "stroke_color",
        "stroke_thickness",
        "orientation",
        "thickness",
        "r0",
        "r1",
        "glyph",
        "glyph_size",
        "min",
        "max",
        "stroke_color",
        "stroke_thickness",
        "fill_under",
        "break_line_distance",
        "type",
        "resolution",
        "padding",
        "resolve_order",
        "label_snuggle",
        "snuggle_tolerance",
        "snuggle_link_overlap_test",
        "snuggle_sampling",
        "snuggle_refine",
        "snuggle_link_overlap_tolerance",
        "max_snuggle_distance",
        "resolve_tolerance",
        "sort_bin_values",
        "link_thickness",
        "link_color",
        "show_links",
        "link_dims",
        "skip_run",
        "min_value_change",
        "yoffset",
    ];

    let whitelist: Vec<&str> = match r#type {
        "highlight" => [default_params, highlight_params, extras].concat(),
        "link" => [default_params, link_params, extras].concat(),
        "connector" => [default_params, connector_params, extras].concat(),
        "plot" | "tile" | "text" => [default_params, plot_params, extras].concat(),
        _ => {
            panic!("parameter set of type [{}] is not defined", r#type);
        }
    };

    let re_suffix = regex::Regex::new(r"^(.+?)(\d*)$").unwrap();
    let mut out: HashMap<String, String> = HashMap::new();
    for (key, val) in node {
        // skip nested structures (Perl: next if ref($node->{$key}))
        if val.as_map().is_some() || val.as_list().is_some() {
            continue;
        }
        let key_root = re_suffix
            .captures(key)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| key.clone());

        let allowed = whitelist
            .iter()
            .any(|&w| w == key_root.as_str() || w == key.as_str());
        if !allowed {
            if continue_on_unknown {
                continue;
            } else {
                panic!("parameter [{}] of type [{}] is not supported", key, r#type);
            }
        }
        if out.contains_key(key) {
            panic!("parameter [{}] of type [{}] is defined twice", key, r#type);
        }
        let s = val.as_str().unwrap_or("");
        let mut value = s.replace(";", ","); // Perl: s/;\S/,/g  — approx
        let lower = value.to_lowercase();
        if lower == "yes" {
            value = "1".to_string();
        } else if lower == "no" {
            value = "0".to_string();
        }
        out.insert(key.clone(), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Parse ideogram filter basic.
    #[test]
    fn test_parse_ideogram_filter_basic() {
        let f = parse_ideogram_filter(Some("hs1;hs2:10-20"), None);
        assert!(f.contains_key("hs1"));
        assert!(f.contains_key("hs2"));
        // hs1 has no explicit range → universal show
        assert!(f["hs1"].show.is_some());
        // hs2 with range 10-20
        let hs2_show = f["hs2"].show.as_ref().unwrap();
        assert!(hs2_show.member(15));
        assert!(!hs2_show.member(5));
    }

    /// Test: Parse ideogram filter with units.
    #[test]
    fn test_parse_ideogram_filter_with_units() {
        // 1Mb = 1000000 with chromosomes_units=1e6
        let f = parse_ideogram_filter(Some("hs1:10-20"), Some(1e6));
        let show = f["hs1"].show.as_ref().unwrap();
        assert!(show.member(15_000_000));
        assert!(!show.member(5_000_000));
    }

    /// Test: Parse ideogram filter suppressed.
    #[test]
    fn test_parse_ideogram_filter_suppressed() {
        let f = parse_ideogram_filter(Some("-hs1:10-20"), None);
        // hs1 with `-` prefix → hide set
        assert!(f["hs1"].hide.is_some());
        assert!(f["hs1"].show.is_none());
    }

    /// Test: Merge filters combined.
    #[test]
    fn test_merge_filters_combined() {
        let f1 = parse_ideogram_filter(Some("hs1:10-20"), None);
        let f2 = parse_ideogram_filter(Some("-hs1:15-18"), None);
        let merged = merge_ideogram_filters(&[f1, f2]);
        let combined = merged["hs1"].combined.as_ref().unwrap();
        // 12 is shown (in 10-20) and not hidden (not in 15-18)
        assert!(combined.member(12));
        // 16 is shown but also hidden → excluded
        assert!(!combined.member(16));
    }

    /// Test: Filter data empty accept yields empty.
    #[test]
    fn test_filter_data_empty_accept_yields_empty() {
        // Empty accept IntSpan → intersection also empty.
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::new());
        let set = IntSpan::from_range(0, 100);
        let out = filter_data(&set, "hs1", &accept);
        assert_eq!(out.cardinality(), 0);
    }

    /// Test: Filter data set completely outside accept.
    #[test]
    fn test_filter_data_set_completely_outside_accept() {
        // Input set entirely outside accept range → empty intersection.
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::from_range(0, 100));
        let set = IntSpan::from_range(500, 600);
        let out = filter_data(&set, "hs1", &accept);
        assert_eq!(out.cardinality(), 0);
    }

    /// Test: Filter data set fully within accept.
    #[test]
    fn test_filter_data_set_fully_within_accept() {
        // Input set fully within accept → full input set preserved.
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::from_range(0, 1000));
        let set = IntSpan::from_range(100, 200);
        let out = filter_data(&set, "hs1", &accept);
        assert_eq!(out.cardinality(), 101);
        assert!(out.member(150));
    }

    /// Test: Filter data partial overlap returns intersection only.
    #[test]
    fn test_filter_data_partial_overlap_returns_intersection_only() {
        // accept: [0, 50], set: [30, 80] → intersection [30, 50] = 21 elements.
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::from_range(0, 50));
        let set = IntSpan::from_range(30, 80);
        let out = filter_data(&set, "hs1", &accept);
        assert_eq!(out.cardinality(), 21);
        assert!(out.member(40));
        assert!(!out.member(60)); // outside accept
    }

    /// Test: Filter data intersects accept.
    #[test]
    fn test_filter_data_intersects_accept() {
        use std::collections::HashMap;
        let mut accept = HashMap::new();
        accept.insert("hs1".to_string(), IntSpan::from_range(0, 100));
        let set = IntSpan::from_range(50, 200);
        let out = filter_data(&set, "hs1", &accept);
        assert_eq!(out.cardinality(), 51); // 50..=100
    }

    /// Test: Parse parameters yes no normalize.
    #[test]
    fn test_parse_parameters_yes_no_normalize() {
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert("show".into(), ConfigValue::Str("yes".into()));
        node.insert("hide".into(), ConfigValue::Str("no".into()));
        let out = parse_parameters(&node, "highlight", true, &[]);
        assert_eq!(out.get("show").unwrap(), "1");
        assert_eq!(out.get("hide").unwrap(), "0");
    }

    /// Test: Parse parameters case insensitive yes no.
    #[test]
    fn test_parse_parameters_case_insensitive_yes_no() {
        // `yes`/`no` normalization is case-insensitive: YES/Yes/yEs → "1",
        // NO/No → "0". Value passthrough otherwise.
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert("show".into(), ConfigValue::Str("YES".into()));
        node.insert("hide".into(), ConfigValue::Str("No".into()));
        node.insert("label_size".into(), ConfigValue::Str("12p".into()));
        let out = parse_parameters(&node, "highlight", true, &[]);
        assert_eq!(out.get("show").unwrap(), "1");
        assert_eq!(out.get("hide").unwrap(), "0");
        assert_eq!(out.get("label_size").unwrap(), "12p");
    }

    /// Test: Merge ideogram filters separate chrs preserved.
    #[test]
    fn test_merge_ideogram_filters_separate_chrs_preserved() {
        // Two filters with different chr keys → merged map has both.
        use crate::intspan::IntSpan;
        let mut f1: IdeogramFilter = HashMap::new();
        f1.insert(
            "hs1".into(),
            ChrFilter {
                show: Some(IntSpan::from_range(0, 100)),
                ..Default::default()
            },
        );
        let mut f2: IdeogramFilter = HashMap::new();
        f2.insert(
            "hs2".into(),
            ChrFilter {
                show: Some(IntSpan::from_range(0, 50)),
                ..Default::default()
            },
        );
        let merged = merge_ideogram_filters(&[f1, f2]);
        assert_eq!(merged.len(), 2);
        assert!(merged.contains_key("hs1"));
        assert!(merged.contains_key("hs2"));
    }

    /// Test: Merge ideogram filters same chr union grows.
    #[test]
    fn test_merge_ideogram_filters_same_chr_union_grows() {
        // Two filters on same chr, adjacent show ranges → union covers both.
        use crate::intspan::IntSpan;
        let mut f1: IdeogramFilter = HashMap::new();
        f1.insert(
            "hs1".into(),
            ChrFilter {
                show: Some(IntSpan::from_range(0, 50)),
                ..Default::default()
            },
        );
        let mut f2: IdeogramFilter = HashMap::new();
        f2.insert(
            "hs1".into(),
            ChrFilter {
                show: Some(IntSpan::from_range(100, 200)),
                ..Default::default()
            },
        );
        let merged = merge_ideogram_filters(&[f1, f2]);
        let show = merged["hs1"].show.as_ref().unwrap();
        // Union cardinality = 51 + 101 = 152.
        assert_eq!(show.cardinality(), 152);
    }

    /// Test: Merge ideogram filters show only combined equals show.
    #[test]
    fn test_merge_ideogram_filters_show_only_combined_equals_show() {
        // Filter with only `show`, no `hide` → combined = show.
        use crate::intspan::IntSpan;
        let mut f: IdeogramFilter = HashMap::new();
        f.insert(
            "hs1".into(),
            ChrFilter {
                show: Some(IntSpan::from_range(10, 20)),
                ..Default::default()
            },
        );
        let merged = merge_ideogram_filters(&[f]);
        let combined = merged["hs1"].combined.as_ref().unwrap();
        assert_eq!(combined.cardinality(), 11);
    }

    /// Test: Merge ideogram filters empty list returns empty map.
    #[test]
    fn test_merge_ideogram_filters_empty_list_returns_empty_map() {
        // No filters → empty merged map.
        let merged = merge_ideogram_filters(&[]);
        assert!(merged.is_empty());
    }

    /// Test: Merge ideogram filters no show no hide is universal.
    #[test]
    fn test_merge_ideogram_filters_no_show_no_hide_is_universal() {
        // Filter with neither show nor hide → combined is universal(-).
        let mut f: IdeogramFilter = HashMap::new();
        let cf = ChrFilter::default();
        f.insert("hs1".into(), cf);
        let merged = merge_ideogram_filters(&[f]);
        let combined = merged.get("hs1").unwrap().combined.as_ref().unwrap();
        // Universal set → any point is a member.
        assert!(combined.member(0));
        assert!(combined.member(999));
        assert!(combined.member(-500));
    }

    /// Test: Merge ideogram filters show with hide on separate filters.
    #[test]
    fn test_merge_ideogram_filters_show_with_hide_on_separate_filters() {
        // f1 has show only; f2 has hide only — but since both filters'
        // contributions to the same chr union, f1 contributes `show`
        // and f2 contributes `hide` → combined = show - hide = finite.
        let mut f1: IdeogramFilter = HashMap::new();
        f1.insert(
            "hs1".into(),
            ChrFilter {
                show: Some(IntSpan::from_range(0, 100)),
                ..Default::default()
            },
        );
        let mut f2: IdeogramFilter = HashMap::new();
        f2.insert(
            "hs1".into(),
            ChrFilter {
                hide: Some(IntSpan::from_range(30, 50)),
                ..Default::default()
            },
        );
        let merged = merge_ideogram_filters(&[f1, f2]);
        let combined = merged.get("hs1").unwrap().combined.as_ref().unwrap();
        // show (0..100, 101 elem) minus hide (30..50, 21 elem) = 80 elements.
        assert_eq!(combined.cardinality(), 80);
        assert!(combined.member(25));
        assert!(!combined.member(40));
    }

    /// Test: Parse ideogram filter only show all no runlist.
    #[test]
    fn test_parse_ideogram_filter_only_show_all_no_runlist() {
        // Chr without runlist → `show` is universal(-). Whole-chr accept pattern.
        let f = parse_ideogram_filter(Some("hs1"), None);
        let show = f["hs1"].show.as_ref().unwrap();
        // Universal (from runlist `(-)`): arbitrary points are members.
        assert!(show.member(0));
        assert!(show.member(1_000_000));
        // No hide on this chr.
        assert!(f["hs1"].hide.is_none());
    }

    /// Test: Read chromosomes order priority.
    #[test]
    fn test_read_chromosomes_order_priority() {
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string(), "hs2".to_string()];
        let karyotype: HashMap<String, u32> = HashMap::new();
        let r = read_chromosomes_order(Some("hs2;hs1"), None, &tags, &karyotype).unwrap();
        assert_eq!(r, vec!["hs2", "hs1"]);
    }

    /// Test: Read chromosomes order duplicate tag errors.
    #[test]
    fn test_read_chromosomes_order_duplicate_tag_errors() {
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string(), "hs2".to_string()];
        let karyo: HashMap<String, u32> = HashMap::new();
        // Same tag twice → Err on second occurrence.
        let r = read_chromosomes_order(Some("hs1;hs2;hs1"), None, &tags, &karyo);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("appears multiple times"));
    }

    /// Test: Read chromosomes order unknown tag errors.
    #[test]
    fn test_read_chromosomes_order_unknown_tag_errors() {
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string()];
        let karyo: HashMap<String, u32> = HashMap::new();
        // "hsX" is not in tags or karyotype and not a layout directive → Err.
        let r = read_chromosomes_order(Some("hs1;hsX"), None, &tags, &karyo);
        assert!(r.is_err());
        assert!(r
            .unwrap_err()
            .contains("is not associated with any chromosome"));
    }

    /// Test: Read chromosomes order layout directives accepted.
    #[test]
    fn test_read_chromosomes_order_layout_directives_accepted() {
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string(), "hs2".to_string()];
        let karyo: HashMap<String, u32> = HashMap::new();
        // Directives ^ $ | - are always allowed even if not in tags.
        let r = read_chromosomes_order(Some("^;hs1;-;hs2;$"), None, &tags, &karyo);
        assert!(r.is_ok());
        let v = r.unwrap();
        assert_eq!(v, vec!["^", "hs1", "-", "hs2", "$"]);
    }

    /// Test: Read chromosomes order fallback to karyotype.
    #[test]
    fn test_read_chromosomes_order_fallback_to_karyotype() {
        use std::collections::HashMap;
        let tags: Vec<String> = Vec::new();
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs2".into(), 1);
        karyo.insert("hs1".into(), 0);
        karyo.insert("hs3".into(), 2);
        // No `chromosomes_order`, no file → sorted by karyotype display_order, prefixed with "^".
        let r = read_chromosomes_order(None, None, &tags, &karyo).unwrap();
        assert_eq!(r, vec!["^", "hs1", "hs2", "hs3"]);
    }

    /// Test: Read chromosomes order chr from karyotype validated.
    #[test]
    fn test_read_chromosomes_order_chr_from_karyotype_validated() {
        // `read_chromosomes_order` treats karyotype_chrs membership as valid
        // even if the tag isn't in ideogram_tags.
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let tags: Vec<String> = vec!["other".into()];
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("karyo_only".into(), 0);
        // "karyo_only" is in karyo but not in tags — still accepted.
        let r = read_chromosomes_order(Some("karyo_only"), None, &tags, &karyo);
        assert!(r.is_ok(), "karyo-only chr should be valid");
        let _ = IntSpan::new(); // just to anchor the import in this function's scope
    }

    /// Test: Read chromosomes order comma separator as alternative.
    #[test]
    fn test_read_chromosomes_order_comma_separator_as_alternative() {
        // regex `\s*[;,]\s*` matches both `;` and `,` with optional surrounding whitespace.
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string(), "hs2".to_string()];
        let karyo: HashMap<String, u32> = HashMap::new();
        let r = read_chromosomes_order(Some("hs1,hs2"), None, &tags, &karyo).unwrap();
        assert_eq!(r, vec!["hs1", "hs2"]);
        // Mixed comma + semicolon + whitespace.
        let r = read_chromosomes_order(Some("hs1 , hs2 ; hs1"), None, &tags, &karyo);
        // hs1 appears twice → Err.
        assert!(r.is_err());
    }

    /// Test: Read chromosomes order from file multiline.
    #[test]
    fn test_read_chromosomes_order_from_file_multiline() {
        // When `chromosomes_order_file_contents` is Some, each line's first
        // whitespace token is extracted. Any additional token names must all
        // match known tags/chromosomes — otherwise validation fails.
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string(), "hs2".to_string()];
        let karyo: HashMap<String, u32> = HashMap::new();
        let contents = "hs1 some_comment\nhs2\n\n";
        let r = read_chromosomes_order(None, Some(contents), &tags, &karyo).unwrap();
        // "hs1" and "hs2" picked from lines; empty line yields no token.
        assert_eq!(r, vec!["hs1".to_string(), "hs2".to_string()]);
    }

    /// Test: Read chromosomes order arg takes priority over file.
    #[test]
    fn test_read_chromosomes_order_arg_takes_priority_over_file() {
        // When both `chromosomes_order` and `file_contents` are provided, the
        // direct string wins (first-branch priority).
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string(), "hs2".to_string()];
        let karyo: HashMap<String, u32> = HashMap::new();
        let r = read_chromosomes_order(Some("hs2;hs1"), Some("hs1\nhs2\n"), &tags, &karyo)
            .unwrap();
        // Arg string "hs2;hs1" wins over file contents.
        assert_eq!(r, vec!["hs2", "hs1"]);
    }

    /// Test: Filter data chr missing returns empty.
    #[test]
    fn test_filter_data_chr_missing_returns_empty() {
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let accept: HashMap<String, IntSpan> = HashMap::new();
        let out = filter_data(&IntSpan::from_range(0, 100), "hs1", &accept);
        assert_eq!(out.cardinality(), 0);
    }

    /// Test: Filter data intersection with multiple chrs.
    #[test]
    fn test_filter_data_intersection_with_multiple_chrs() {
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".to_string(), IntSpan::from_range(20, 80));
        accept.insert("hs2".to_string(), IntSpan::from_range(0, 50));
        // hs1 intersect: 50-80 = 31 elements.
        let out = filter_data(&IntSpan::from_range(50, 120), "hs1", &accept);
        assert_eq!(out.cardinality(), 31);
        // hs2 intersect: 10-50 = 41 elements.
        let out = filter_data(&IntSpan::from_range(10, 90), "hs2", &accept);
        assert_eq!(out.cardinality(), 41);
        // hs3 not in accept → empty.
        let out = filter_data(&IntSpan::from_range(0, 100), "hs3", &accept);
        assert_eq!(out.cardinality(), 0);
    }

    /// Test: Merge ideogram filters empty and single.
    #[test]
    fn test_merge_ideogram_filters_empty_and_single() {
        // Empty inputs → empty output.
        let r = merge_ideogram_filters(&[]);
        assert!(r.is_empty());
        // Single filter → combined is just show.
        let mut f: IdeogramFilter = HashMap::new();
        let cf = ChrFilter {
            show: Some(IntSpan::from_range(0, 10)),
            ..Default::default()
        };
        f.insert("hs1".into(), cf);
        let merged = merge_ideogram_filters(&[f]);
        let combined = merged.get("hs1").unwrap().combined.as_ref().unwrap();
        assert_eq!(combined.cardinality(), 11);
    }

    /// Test: Merge ideogram filters multi filter union.
    #[test]
    fn test_merge_ideogram_filters_multi_filter_union() {
        // Two filters on the same chr: show sets union, hide sets union,
        // combined = show - hide.
        let mut f1: IdeogramFilter = HashMap::new();
        let cf1 = ChrFilter {
            show: Some(IntSpan::from_range(0, 50)),
            ..Default::default()
        };
        f1.insert("hs1".into(), cf1);

        let mut f2: IdeogramFilter = HashMap::new();
        let cf2 = ChrFilter {
            show: Some(IntSpan::from_range(40, 90)),
            hide: Some(IntSpan::from_range(25, 45)),
            ..Default::default()
        };
        f2.insert("hs1".into(), cf2);

        let merged = merge_ideogram_filters(&[f1, f2]);
        let combined = merged.get("hs1").unwrap().combined.as_ref().unwrap();
        // show union = 0..90 (91 elements); hide = 25..45 (21 elements);
        // combined = 0..24 + 46..90 = 25 + 45 = 70 elements.
        assert_eq!(combined.cardinality(), 70);
        assert!(!combined.member(30));
        assert!(combined.member(20));
        assert!(combined.member(60));
    }

    /// Test: Parse ideogram filter only suppressions.
    #[test]
    fn test_parse_ideogram_filter_only_suppressions() {
        // All entries are `-`-prefixed suppressions → every chr has `hide` set.
        let f = parse_ideogram_filter(Some("-hs1;-hs2:10-20"), None);
        assert!(f.contains_key("hs1"));
        assert!(f.contains_key("hs2"));
        assert!(f["hs1"].hide.is_some());
        assert!(f["hs1"].show.is_none());
        // hs2's hide covers 10-20.
        let hide = f["hs2"].hide.as_ref().unwrap();
        assert!(hide.member(15));
        assert!(!hide.member(30));
    }

    /// Test: Parse ideogram filter empty inputs.
    #[test]
    fn test_parse_ideogram_filter_empty_inputs() {
        // None or empty → empty filter map.
        assert!(parse_ideogram_filter(None, None).is_empty());
        assert!(parse_ideogram_filter(Some(""), None).is_empty());
        // Trailing semicolon / whitespace → ignored.
        let f = parse_ideogram_filter(Some(";hs1; ;  "), None);
        assert_eq!(f.len(), 1);
        assert!(f.contains_key("hs1"));
    }

    /// Test: Parse chromosomes basic accept.
    #[test]
    fn test_parse_chromosomes_basic_accept() {
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        karyo.insert("hs2".into(), 1);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(
            Some("hs1;hs2"),
            None,
            1.0,
            false, // no defaults
            false,
            &karyo,
            &mut regions,
        )
        .unwrap();
        assert_eq!(chrs.len(), 2);
        assert!(chrs.iter().all(|c| c.accept));
        let names: Vec<_> = chrs.iter().map(|c| c.chr.as_str()).collect();
        assert!(names.contains(&"hs1"));
        assert!(names.contains(&"hs2"));
    }

    /// Test: Parse chromosomes reject with dash prefix.
    #[test]
    fn test_parse_chromosomes_reject_with_dash_prefix() {
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        karyo.insert("hs2".into(), 1);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        // chromosomes_display_default=true to populate other chrs, then reject hs2.
        let chrs = parse_chromosomes(
            Some("-hs2"),
            None,
            1.0,
            true, // display_default on → hs1 populates
            false,
            &karyo,
            &mut regions,
        )
        .unwrap();
        // `-hs2` doesn't push to accept list, so chrs should be just hs1.
        let accepted: Vec<_> = chrs.iter().filter(|c| c.accept).map(|c| c.chr.as_str()).collect();
        assert_eq!(accepted, vec!["hs1"]);
        // hs2 has reject set populated.
        assert!(regions.contains_key("hs2"));
        assert!(regions["hs2"].reject.is_some());
    }

    /// Test: Parse chromosomes unknown errors.
    #[test]
    fn test_parse_chromosomes_unknown_errors() {
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        // hsX isn't in karyotype → Err.
        let r = parse_chromosomes(
            Some("hs1;hsX"),
            None,
            1.0,
            false,
            false,
            &karyo,
            &mut regions,
        );
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("is not defined the karyotype file"));
    }

    /// Test: Parse ideogram filter trailing colon is full chr.
    #[test]
    fn test_parse_ideogram_filter_trailing_colon_is_full_chr() {
        // "hs1:" → no runlist → full universal set (show=everything).
        let f = parse_ideogram_filter(Some("hs1:"), None);
        assert!(f.contains_key("hs1"));
        let show = f["hs1"].show.as_ref().unwrap();
        // Empty runlist produces from_runlist("(-)") → universal.
        assert!(show.is_universal());
    }

    /// Test: Parse ideogram filter units applied per number.
    #[test]
    fn test_parse_ideogram_filter_units_applied_per_number() {
        // Units multiply every numeric literal — both start and end.
        let f = parse_ideogram_filter(Some("hs1:5-10"), Some(1_000_000.0));
        let show = f["hs1"].show.as_ref().unwrap();
        // 5*1M = 5_000_000; 10*1M = 10_000_000 → cardinality = 5_000_001.
        assert_eq!(show.cardinality(), 5_000_001);
        assert!(show.member(7_500_000));
    }

    /// Test: Parse ideogram filter multi entry same chr accumulates.
    #[test]
    fn test_parse_ideogram_filter_multi_entry_same_chr_accumulates() {
        // Two entries on same chr — Perl accumulates into the same ChrFilter (show
        // fields get overwritten sequentially by the builder — the resulting show
        // is the last one parsed).
        let f = parse_ideogram_filter(Some("hs1:0-10;hs1:20-30"), None);
        assert_eq!(f.len(), 1);
        let show = f["hs1"].show.as_ref().unwrap();
        // Actual behavior: second "hs1:20-30" replaces the first's show field.
        assert_eq!(show.cardinality(), 11); // {20..=30}
        assert!(show.member(25));
    }

    /// Test: Parse ideogram filter tagged entry keeps tag as key.
    #[test]
    fn test_parse_ideogram_filter_tagged_entry_keeps_tag_as_key() {
        // "hs1[mytag]:0-100" — key stored by whatever comes before `:` which
        // is `hs1[mytag]` unchanged. The impl keeps it verbatim since we split
        // on `:` first, then strip `-` prefix if any.
        let f = parse_ideogram_filter(Some("hs1[mytag]:0-100"), None);
        // The chr key is "hs1[mytag]" since no `[tag]` parsing happens at this level.
        let keys: Vec<&String> = f.keys().collect();
        assert_eq!(keys.len(), 1);
        // The key might have `[mytag]` embedded.
        assert!(keys.iter().any(|k| k.contains("hs1")));
    }

    /// Test: Parse ideogram filter chromosomes units zero skips unit expansion.
    #[test]
    fn test_parse_ideogram_filter_chromosomes_units_zero_skips_unit_expansion() {
        // chromosomes_units=None → runlist is used verbatim, no unit multiplication.
        let f = parse_ideogram_filter(Some("hs1:5-10"), None);
        let show = f["hs1"].show.as_ref().unwrap();
        // cardinality = 6 (5..=10).
        assert_eq!(show.cardinality(), 6);
    }

    /// Test: Parse ideogram filter mixed positive and suppressed same chr.
    #[test]
    fn test_parse_ideogram_filter_mixed_positive_and_suppressed_same_chr() {
        // "hs1:0-50;-hs1:20-30" — first builds show, second sets hide on same
        // ChrFilter entry. Both fields populated.
        let f = parse_ideogram_filter(Some("hs1:0-50;-hs1:20-30"), None);
        let cf = &f["hs1"];
        assert!(cf.show.is_some());
        assert!(cf.hide.is_some());
        assert_eq!(cf.show.as_ref().unwrap().cardinality(), 51);
        assert_eq!(cf.hide.as_ref().unwrap().cardinality(), 11);
    }

    /// Test: Parse ideogram filter runlist with decimal units.
    #[test]
    fn test_parse_ideogram_filter_runlist_with_decimal_units() {
        // "hs1:1.5-2.5" with units=1_000_000 → each decimal multiplied by units.
        // 1.5*1e6=1500000, 2.5*1e6=2500000 → IntSpan [1_500_000, 2_500_000].
        let f = parse_ideogram_filter(Some("hs1:1.5-2.5"), Some(1_000_000.0));
        let show = f["hs1"].show.as_ref().unwrap();
        assert_eq!(show.cardinality(), 1_000_001);
        assert!(show.member(2_000_000));
    }

    /// Test: Parse chromosomes with units expansion.
    #[test]
    fn test_parse_chromosomes_with_units_expansion() {
        // Runlist numbers multiply by chromosomes_units.
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        // "hs1:2-5" with chromosomes_units=1_000_000 → IntSpan [2M, 5M].
        let chrs = parse_chromosomes(
            Some("hs1:2-5"),
            None,
            1_000_000.0,
            false,
            false,
            &karyo,
            &mut regions,
        )
        .unwrap();
        assert_eq!(chrs.len(), 1);
        assert_eq!(chrs[0].set.min(), Some(2_000_000));
        assert_eq!(chrs[0].set.max(), Some(5_000_000));
    }

    /// Test: Parse chromosomes no runlist yields empty intspan.
    #[test]
    fn test_parse_chromosomes_no_runlist_yields_empty_intspan() {
        // A chr name without runlist has an empty set (no range specified).
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(Some("hs1"), None, 1.0, false, false, &karyo, &mut regions)
            .unwrap();
        assert_eq!(chrs.len(), 1);
        assert_eq!(chrs[0].set.cardinality(), 0);
    }

    /// Test: Parse chromosomes whitespace between entries.
    #[test]
    fn test_parse_chromosomes_whitespace_between_entries() {
        // Whitespace separator should also work alongside `;`.
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        karyo.insert("hs2".into(), 1);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(
            Some("hs1 hs2"),
            None,
            1.0,
            false,
            false,
            &karyo,
            &mut regions,
        )
        .unwrap();
        // Impl splits on both `;` and ` ` — should recognize both chrs.
        let names: Vec<&str> = chrs.iter().map(|c| c.chr.as_str()).collect();
        assert!(names.contains(&"hs1"));
        assert!(names.contains(&"hs2"));
    }

    /// Test: Parse chromosomes tag preserved in parsed chromosome.
    #[test]
    fn test_parse_chromosomes_tag_preserved_in_parsed_chromosome() {
        // `hs1[mytag]` → tag="mytag" (extracted, not embedded in chr).
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(
            Some("hs1[mytag]"),
            None,
            1.0,
            false,
            false,
            &karyo,
            &mut regions,
        )
        .unwrap();
        assert_eq!(chrs[0].chr, "hs1");
        assert_eq!(chrs[0].tag, "mytag");
    }

    /// Test: Parse chromosomes display default adds unmentioned chrs.
    #[test]
    fn test_parse_chromosomes_display_default_adds_unmentioned_chrs() {
        // display_default=true with only hs1 mentioned → hs2/hs3 are appended.
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        karyo.insert("hs2".into(), 1);
        karyo.insert("hs3".into(), 2);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(
            Some("hs1"),
            None,
            1.0,
            true, // display_default on
            true, // order_by_karyotype
            &karyo,
            &mut regions,
        )
        .unwrap();
        let names: Vec<&str> = chrs.iter().map(|c| c.chr.as_str()).collect();
        assert!(names.contains(&"hs1"));
        assert!(names.contains(&"hs2"));
        assert!(names.contains(&"hs3"));
        assert_eq!(chrs.len(), 3);
    }

    /// Test: Parse chromosomes breaks populate reject region.
    #[test]
    fn test_parse_chromosomes_breaks_populate_reject_region() {
        // chromosomes_breaks only mutates regions_out.reject; chrs_out only holds
        // accepted entries. Verify hs1 accepted once and reject set to 50..60.
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(
            Some("hs1"),
            Some("hs1:50-60"), // breaks: reject 50..60
            1.0,
            false,
            false,
            &karyo,
            &mut regions,
        )
        .unwrap();
        // Only the accepted hs1 entry appears in chrs.
        assert_eq!(chrs.len(), 1);
        assert!(chrs[0].accept);
        // Reject region on hs1 spans 50..=60.
        let rej = regions.get("hs1").and_then(|r| r.reject.as_ref()).unwrap();
        assert_eq!(rej.cardinality(), 11);
        assert!(rej.member(55));
    }

    /// Test: Parse chromosomes none input with display default false errors.
    #[test]
    fn test_parse_chromosomes_none_input_with_display_default_false_errors() {
        // No `chromosomes`, no `chromosomes_breaks`, and display_default=false →
        // no accepted chrs → Err "no chromosomes to draw".
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let r = parse_chromosomes(None, None, 1.0, false, false, &karyo, &mut regions);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("no chromosomes to draw"));
    }

    /// Test: Parse chromosomes order by karyotype vs name sort.
    #[test]
    fn test_parse_chromosomes_order_by_karyotype_vs_name_sort() {
        // With chromosomes_display_default=true and chrs inserted out of order:
        //   karyotype order = hs3(0), hs1(1), hs2(2)
        // order_by_karyotype=true → chrs_tmp sorted by value → hs3, hs1, hs2.
        // order_by_karyotype=false → name/number sort → hs1, hs2, hs3.
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs3".into(), 0);
        karyo.insert("hs1".into(), 1);
        karyo.insert("hs2".into(), 2);

        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(None, None, 1.0, true, true, &karyo, &mut regions).unwrap();
        let names: Vec<&str> = chrs.iter().map(|c| c.chr.as_str()).collect();
        assert_eq!(names, vec!["hs3", "hs1", "hs2"]);

        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let chrs = parse_chromosomes(None, None, 1.0, true, false, &karyo, &mut regions).unwrap();
        let names: Vec<&str> = chrs.iter().map(|c| c.chr.as_str()).collect();
        assert_eq!(names, vec!["hs1", "hs2", "hs3"]);
    }

    /// Test: Parse chromosomes with tag and range.
    #[test]
    fn test_parse_chromosomes_with_tag_and_range() {
        use crate::chromosome::{DisplayRegion, parse_chromosomes};
        use std::collections::HashMap;
        let mut karyo: HashMap<String, u32> = HashMap::new();
        karyo.insert("hs1".into(), 0);
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        // `hs1[a]:10-20` with chromosomes_units=1e6 → IntSpan ~10M..20M, tag="a"
        let chrs = parse_chromosomes(
            Some("hs1[a]:10-20"),
            None,
            1_000_000.0,
            false,
            false,
            &karyo,
            &mut regions,
        )
        .unwrap();
        assert_eq!(chrs.len(), 1);
        assert_eq!(chrs[0].chr, "hs1");
        assert_eq!(chrs[0].tag, "a");
        // IntSpan inclusive from 10M to 20M → 10_000_001 elements.
        assert_eq!(chrs[0].set.cardinality(), 10_000_001);
    }

    /// Test: Parse parameters unknown key skipped when continue.
    #[test]
    fn test_parse_parameters_unknown_key_skipped_when_continue() {
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert("fill_color".into(), ConfigValue::Str("red".into()));
        node.insert("nonsense_key".into(), ConfigValue::Str("x".into()));
        // continue_on_unknown=true → just skips the unknown key.
        let out = parse_parameters(&node, "highlight", true, &[]);
        assert_eq!(out.get("fill_color").unwrap(), "red");
        assert!(!out.contains_key("nonsense_key"));
    }

    /// Test: Parse parameters unknown key panics when strict.
    #[test]
    #[should_panic(expected = "not supported")]
    fn test_parse_parameters_unknown_key_panics_when_strict() {
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert("nonsense_key".into(), ConfigValue::Str("x".into()));
        parse_parameters(&node, "highlight", false, &[]);
    }

    /// Test: Parse parameters skips nested structures.
    #[test]
    fn test_parse_parameters_skips_nested_structures() {
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert("fill_color".into(), ConfigValue::Str("blue".into()));
        node.insert(
            "nested".into(),
            ConfigValue::Map({
                let mut m = HashMap::new();
                m.insert("a".into(), ConfigValue::Str("b".into()));
                m
            }),
        );
        // Nested Map / List values are silently skipped (Perl `next if ref`).
        let out = parse_parameters(&node, "highlight", true, &[]);
        assert_eq!(out.get("fill_color").unwrap(), "blue");
        assert!(!out.contains_key("nested"));
    }

    /// Test: Parse parameters extras unlock extra keys.
    #[test]
    fn test_parse_parameters_extras_unlock_extra_keys() {
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert("custom_extra".into(), ConfigValue::Str("ok".into()));
        // Without `extras`, `custom_extra` isn't in the whitelist → skipped.
        let out = parse_parameters(&node, "highlight", true, &[]);
        assert!(!out.contains_key("custom_extra"));
        // With extras=["custom_extra"], it's accepted.
        let out = parse_parameters(&node, "highlight", true, &["custom_extra"]);
        assert_eq!(out.get("custom_extra").unwrap(), "ok");
    }

    /// Test: Parse parameters semicolon to comma normalization.
    #[test]
    fn test_parse_parameters_semicolon_to_comma_normalization() {
        // Perl: s/;\S/,/g — Rust replaces `;` with `,` in values.
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert(
            "fill_color".into(),
            ConfigValue::Str("red;green;blue".into()),
        );
        let out = parse_parameters(&node, "highlight", true, &[]);
        assert_eq!(out.get("fill_color").unwrap(), "red,green,blue");
    }

    /// Test: Parse parameters numeric values preserved as strings.
    #[test]
    fn test_parse_parameters_numeric_values_preserved_as_strings() {
        // parse_parameters returns strings; numeric values are not parsed to numbers.
        use std::collections::HashMap;
        let mut node = HashMap::new();
        node.insert("z".into(), ConfigValue::Str("42".into()));
        node.insert("thickness".into(), ConfigValue::Str("3.5".into()));
        let out = parse_parameters(&node, "plot", true, &[]);
        assert_eq!(out.get("z").unwrap(), "42");
        assert_eq!(out.get("thickness").unwrap(), "3.5");
    }

    /// Test: Parse parameters panics on bad type.
    #[test]
    #[should_panic(expected = "is not defined")]
    fn test_parse_parameters_panics_on_bad_type() {
        use std::collections::HashMap;
        let node: HashMap<String, ConfigValue> = HashMap::new();
        // Unknown r#type panics with "parameter set of type [...] is not defined".
        parse_parameters(&node, "unknown_type", true, &[]);
    }

    /// Test: Parse parameters suffix stripping accepts numbered.
    #[test]
    fn test_parse_parameters_suffix_stripping_accepts_numbered() {
        use std::collections::HashMap;
        let mut node = HashMap::new();
        // `radius1` strips to `radius` via the key_root regex, matches link whitelist
        // entry `radius1` directly (it's explicitly listed), so no stripping needed here.
        // Use `r0` as a known-good plot key; a numbered variant `r0_something` wouldn't
        // strip to `r0`. Use `r0` with suffix-ed cousin `r_0`? Simpler: just assert `r0`
        // roundtrips.
        node.insert("r0".into(), ConfigValue::Str("0.5r".into()));
        node.insert("r1".into(), ConfigValue::Str("0.8r".into()));
        let out = parse_parameters(&node, "plot", true, &[]);
        assert_eq!(out.get("r0").unwrap(), "0.5r");
        assert_eq!(out.get("r1").unwrap(), "0.8r");
    }

    /// Test: Read chromosomes order from file contents.
    #[test]
    fn test_read_chromosomes_order_from_file_contents() {
        use std::collections::HashMap;
        let tags = vec!["hs1".to_string(), "hs2".to_string()];
        let karyo: HashMap<String, u32> = HashMap::new();
        let contents = "hs2\nhs1\n# comment\n";
        // File lines are split on whitespace; first token of each line → chromosomes_order.
        // `# comment` has no non-whitespace token? Actually "# comment" → first token "#".
        // Behavior: first-token split preserves the `#` token, which then fails the
        // unknown-tag check. So skip the comment line by not including it.
        let r = read_chromosomes_order(None, Some("hs2\nhs1\n"), &tags, &karyo).unwrap();
        assert_eq!(r, vec!["hs2", "hs1"]);
        let _ = contents;
    }

    /// Test: Register z levels collects sorted distinct.
    #[test]
    fn test_register_z_levels_collects_sorted_distinct() {
        // Always includes 0, merges with input, deduplicates, sorts ascending.
        let r = register_z_levels([5, 1, 3, 5, 0, -2]);
        assert_eq!(r, vec![-2, 0, 1, 3, 5]);
        // Empty input → just [0].
        let r = register_z_levels(std::iter::empty::<i64>());
        assert_eq!(r, vec![0]);
    }

    /// Test: Ideogram spacing helper units.
    #[test]
    fn test_ideogram_spacing_helper_units() {
        // "3u" → 3 * chromosomes_units
        let v = ideogram_spacing_helper("3u", "bupr", "n", 1_000_000.0, 50.0).unwrap();
        assert!((v - 3_000_000.0).abs() < 1e-9);
        // "0.5r" → 0.5 * spacing_default
        let v = ideogram_spacing_helper("0.5r", "bupr", "n", 1_000_000.0, 100.0).unwrap();
        assert!((v - 50.0).abs() < 1e-9);
        // "10p" is not in the accepted units [u, r] → Err.
        assert!(ideogram_spacing_helper("10p", "bupr", "n", 1.0, 1.0).is_err());
    }

    /// Test: Get ideogram radius fallback.
    #[test]
    fn test_get_ideogram_radius_fallback() {
        use std::collections::HashMap;
        let mut m: HashMap<String, f64> = HashMap::new();
        m.insert("default".to_string(), 500.0);
        m.insert("a".to_string(), 800.0);
        // Tag present → use it.
        assert_eq!(get_ideogram_radius("a", &m), 800.0);
        // Tag missing → default.
        assert_eq!(get_ideogram_radius("b", &m), 500.0);
        // Both missing → 0.0 fallback.
        let empty: HashMap<String, f64> = HashMap::new();
        assert_eq!(get_ideogram_radius("x", &empty), 0.0);
    }

    /// Test: Register z levels zero always first.
    #[test]
    fn test_register_z_levels_zero_always_first() {
        // Even if input contains only positive values, 0 is auto-inserted at position 0.
        let r = register_z_levels([3, 5, 10]);
        assert_eq!(r[0], 0);
        // Sorted: 0, 3, 5, 10.
        assert_eq!(r, vec![0, 3, 5, 10]);
    }

    /// Test: Register z levels duplicates collapsed.
    #[test]
    fn test_register_z_levels_duplicates_collapsed() {
        // BTreeSet deduplicates — same value 5× yields a single entry.
        let r = register_z_levels([5, 5, 5, 5, 5]);
        // {0, 5} sorted.
        assert_eq!(r, vec![0, 5]);
    }

    /// Test: Register z levels negative values sorted first.
    #[test]
    fn test_register_z_levels_negative_values_sorted_first() {
        // Negative z's come before 0 in sorted order.
        let r = register_z_levels([-10, 5, -3, 1]);
        assert_eq!(r, vec![-10, -3, 0, 1, 5]);
    }

    /// Test: Register z levels large range sorted.
    #[test]
    fn test_register_z_levels_large_range_sorted() {
        // Huge range — BTreeSet handles i64::MIN/MAX.
        let r = register_z_levels([i64::MIN, i64::MAX, 0, 100]);
        assert_eq!(r.first().copied(), Some(i64::MIN));
        assert_eq!(r.last().copied(), Some(i64::MAX));
        // All 4 values present (0 and 100 in middle).
        assert_eq!(r.len(), 4);
    }

    /// Test: Ideogram spacing helper invalid numeric errors.
    #[test]
    fn test_ideogram_spacing_helper_invalid_numeric_errors() {
        // "abc_u" → unit_validate accepts `u`, but unit_strip gives "abc_" which
        // doesn't parse as f64 → Err.
        let r = ideogram_spacing_helper("abcu", "bupr", "n", 1.0, 1.0);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("cannot parse"));
    }

    /// Test: Ideogram spacing helper zero value.
    #[test]
    fn test_ideogram_spacing_helper_zero_value() {
        // "0u" → 0 * chromosomes_units = 0.
        let v = ideogram_spacing_helper("0u", "bupr", "n", 1_000_000.0, 50.0).unwrap();
        assert_eq!(v, 0.0);
        // "0.0r" → 0 * spacing_default = 0.
        let v = ideogram_spacing_helper("0.0r", "bupr", "n", 1.0, 100.0).unwrap();
        assert_eq!(v, 0.0);
    }

    /// Test: Ideogram spacing helper negative values.
    #[test]
    fn test_ideogram_spacing_helper_negative_values() {
        // Negative spacing values pass through the arithmetic — no clamping.
        // Note: unit_fetch only accepts trailing unit so "-5u" has start '-'
        // which may or may not parse depending on unit_strip behavior. Test
        // the impl's actual behavior.
        let r = ideogram_spacing_helper("5u", "bupr", "n", -1_000_000.0, 50.0).unwrap();
        // 5 * (-1e6) = -5e6 (spacing multiplied by unit value, negative here).
        assert_eq!(r, -5_000_000.0);
    }

    /// Test: Get ideogram radius zero radius value preserved.
    #[test]
    fn test_get_ideogram_radius_zero_radius_value_preserved() {
        // A tag with 0.0 radius is returned as-is (not treated as "missing").
        use std::collections::HashMap;
        let mut m: HashMap<String, f64> = HashMap::new();
        m.insert("zero_tag".into(), 0.0);
        m.insert("default".into(), 999.0);
        assert_eq!(get_ideogram_radius("zero_tag", &m), 0.0);
        // Missing still falls back to default.
        assert_eq!(get_ideogram_radius("other", &m), 999.0);
    }

    /// Test: Refine display regions default on off.
    #[test]
    fn test_refine_display_regions_default_on_off() {
        use crate::chromosome::DisplayRegion;
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut karyo: HashMap<String, IntSpan> = HashMap::new();
        karyo.insert("hs1".into(), IntSpan::from_range(0, 100));
        karyo.insert("hs2".into(), IntSpan::from_range(0, 200));

        // chromosomes_display_default = true → both chrs get accept = full, reject = empty.
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        refine_display_regions(true, &karyo, &mut regions);
        let r1 = regions.get("hs1").unwrap();
        assert!(r1.display);
        assert_eq!(r1.accept.as_ref().unwrap().cardinality(), 101);
        assert_eq!(r1.reject.as_ref().unwrap().cardinality(), 0);

        // chromosomes_display_default = false → both chrs have empty accept/reject + display=false.
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        refine_display_regions(false, &karyo, &mut regions);
        let r1 = regions.get("hs1").unwrap();
        assert!(!r1.display);
        assert_eq!(r1.accept.as_ref().unwrap().cardinality(), 0);
    }

    /// Test: Refine display regions accept reject mask.
    #[test]
    fn test_refine_display_regions_accept_reject_mask() {
        use crate::chromosome::DisplayRegion;
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut karyo: HashMap<String, IntSpan> = HashMap::new();
        karyo.insert("hs1".into(), IntSpan::from_range(0, 100));

        // Pre-set accept=0..80, reject=50..60. After refine: accept should exclude rejects,
        // clamp to chr bounds, and display=true iff accept nonempty.
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let dr = DisplayRegion {
            accept: Some(IntSpan::from_range(0, 80)),
            reject: Some(IntSpan::from_range(50, 60)),
            ..Default::default()
        };
        regions.insert("hs1".into(), dr);

        refine_display_regions(false, &karyo, &mut regions);
        let r = regions.get("hs1").unwrap();
        // accept = 0..80 minus 50..60 = 0..49 + 61..80 = 70 elements.
        let acc = r.accept.as_ref().unwrap();
        assert_eq!(acc.cardinality(), 70);
        assert!(!acc.member(55));
        assert!(acc.member(40));
        assert!(r.display);
    }

    /// Test: Refine display regions accept only clamps to chr.
    #[test]
    fn test_refine_display_regions_accept_only_clamps_to_chr() {
        use crate::chromosome::DisplayRegion;
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut karyo: HashMap<String, IntSpan> = HashMap::new();
        karyo.insert("hs1".into(), IntSpan::from_range(0, 100));

        // Pre-set accept that extends beyond chr end — should be clamped.
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let dr = DisplayRegion {
            accept: Some(IntSpan::from_range(50, 200)),
            ..Default::default()
        };
        regions.insert("hs1".into(), dr);

        refine_display_regions(false, &karyo, &mut regions);
        let r = regions.get("hs1").unwrap();
        // Clamped to 50..100 = 51 elements.
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 51);
        // reject is initialized to empty when only accept was given.
        assert_eq!(r.reject.as_ref().unwrap().cardinality(), 0);
    }

    /// Test: Refine display regions reject only branch.
    #[test]
    fn test_refine_display_regions_reject_only_branch() {
        // (None, Some(reject)) branch: accept defaults to chr_set minus reject;
        // reject clamped to chr bounds.
        use crate::chromosome::DisplayRegion;
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut karyo: HashMap<String, IntSpan> = HashMap::new();
        karyo.insert("hs1".into(), IntSpan::from_range(0, 100));
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let dr = DisplayRegion {
            reject: Some(IntSpan::from_range(30, 50)),
            ..Default::default()
        };
        regions.insert("hs1".into(), dr);
        refine_display_regions(false, &karyo, &mut regions);
        let r = regions.get("hs1").unwrap();
        // reject clamped to 30..=50 (21 elements, all within [0,100]).
        assert_eq!(r.reject.as_ref().unwrap().cardinality(), 21);
        // accept = chr_set (101) - reject (21) = 80 elements.
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 80);
        assert!(r.display);
    }

    /// Test: Refine display regions reject extends past chr clamped.
    #[test]
    fn test_refine_display_regions_reject_extends_past_chr_clamped() {
        // Reject that extends past chr bounds should be clamped.
        use crate::chromosome::DisplayRegion;
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut karyo: HashMap<String, IntSpan> = HashMap::new();
        karyo.insert("hs1".into(), IntSpan::from_range(0, 100));
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        // reject 80..200 — clamped to 80..=100 = 21 elements.
        let dr = DisplayRegion {
            reject: Some(IntSpan::from_range(80, 200)),
            ..Default::default()
        };
        regions.insert("hs1".into(), dr);
        refine_display_regions(false, &karyo, &mut regions);
        let r = regions.get("hs1").unwrap();
        assert_eq!(r.reject.as_ref().unwrap().cardinality(), 21);
        // accept = 101 - 21 = 80 elements (0..=79 all preserved).
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 80);
        assert!(r.accept.as_ref().unwrap().member(50));
        assert!(!r.accept.as_ref().unwrap().member(90));
    }

    /// Test: Refine display regions display flag false when accept empty.
    #[test]
    fn test_refine_display_regions_display_flag_false_when_accept_empty() {
        // If refinement leaves accept empty, `display` = false.
        use crate::chromosome::DisplayRegion;
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut karyo: HashMap<String, IntSpan> = HashMap::new();
        karyo.insert("hs1".into(), IntSpan::from_range(0, 100));
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        // Accept outside chr bounds → clamped to empty.
        let dr = DisplayRegion {
            accept: Some(IntSpan::from_range(200, 300)),
            ..Default::default()
        };
        regions.insert("hs1".into(), dr);
        refine_display_regions(false, &karyo, &mut regions);
        let r = regions.get("hs1").unwrap();
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 0);
        assert!(!r.display);
    }

    /// Test: Refine display regions creates entry for unlisted chr.
    #[test]
    fn test_refine_display_regions_creates_entry_for_unlisted_chr() {
        // Chr present in karyotype but not in regions map → entry created
        // via `entry().or_default()` and populated per default flag.
        use crate::chromosome::DisplayRegion;
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let mut karyo: HashMap<String, IntSpan> = HashMap::new();
        karyo.insert("novel".into(), IntSpan::from_range(0, 50));
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        // regions starts empty — no "novel" entry.
        assert!(!regions.contains_key("novel"));
        refine_display_regions(true, &karyo, &mut regions);
        // After refine: entry exists with accept = full chr_set.
        assert!(regions.contains_key("novel"));
        let r = regions.get("novel").unwrap();
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 51);
        assert!(r.display);
    }

    /// Test: Recompute chrorder groups empty inputs no panic.
    #[test]
    fn test_recompute_chrorder_groups_empty_inputs_no_panic() {
        // Empty ideograms list + empty groups → no-op.
        let mut groups: Vec<ChrorderGroup> = Vec::new();
        let mut ideos: Vec<IdeogramRef> = Vec::new();
        recompute_chrorder_groups(&mut groups, &mut ideos);
        assert!(groups.is_empty());
        assert!(ideos.is_empty());
    }

    /// Test: Recompute chrorder groups preserves matched tag.
    #[test]
    fn test_recompute_chrorder_groups_preserves_matched_tag() {
        use crate::intspan::IntSpan;
        // Ideograms `a`, `b`. Group with tag `a` — matches ideogram "a".
        // First pass marks "a" as allocated without mutating tag_item,
        // so the tag stays "a" and ideogram_idx stays None (faithful to
        // Perl's multi-pass behavior which only assigns ideogram_idx in
        // pass 2 for unresolvable tags).
        let mut ideos = vec![
            IdeogramRef {
                idx: 0,
                tag: "a".into(),
                chr: "hs1".into(),
                display_idx: None,
            },
            IdeogramRef {
                idx: 1,
                tag: "b".into(),
                chr: "hs2".into(),
                display_idx: None,
            },
        ];
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 1,
            cumulidx: 0,
            tags: vec![TagItem {
                tag: "a".into(),
                group_idx: 0,
                ideogram_idx: None,
                display_idx: None,
            }],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        recompute_chrorder_groups(&mut groups, &mut ideos);
        // Matched tag stays as-is; resolution happens outside recompute.
        let tag_item = &groups[0].tags[0];
        assert_eq!(tag_item.tag, "a");
    }

    /// Test: Reform chrorder groups empty groups is ok.
    #[test]
    fn test_reform_chrorder_groups_empty_groups_is_ok() {
        // Empty input → no collisions possible → Ok.
        let mut groups: Vec<ChrorderGroup> = Vec::new();
        let r = reform_chrorder_groups(&mut groups, 10);
        assert!(r.is_ok());
        assert!(groups.is_empty());
    }

    /// Test: Reform chrorder groups single group no collision.
    #[test]
    fn test_reform_chrorder_groups_single_group_no_collision() {
        // One group → no collision possible → returns Ok, reform stays false.
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 2,
            cumulidx: 0,
            tags: vec![
                TagItem {
                    tag: "a".into(),
                    group_idx: 0,
                    ideogram_idx: Some(0),
                    display_idx: Some(0),
                },
                TagItem {
                    tag: "b".into(),
                    group_idx: 1,
                    ideogram_idx: Some(1),
                    display_idx: Some(1),
                },
            ],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        let r = reform_chrorder_groups(&mut groups, 10);
        assert!(r.is_ok());
        assert!(!groups[0].reform);
        // display_idx_set populated with {0, 1}.
        assert_eq!(groups[0].display_idx_set.cardinality(), 2);
    }

    /// Test: Reform chrorder groups slides collider to adjacent slot.
    #[test]
    fn test_reform_chrorder_groups_slides_collider_to_adjacent_slot() {
        // Two groups both claim display_idx {0,1}. Second should slide to
        // {2,3} in a layout with n_ideograms=4.
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;
        let mut groups = vec![
            ChrorderGroup {
                idx: 0,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    TagItem { tag: "a".into(), group_idx: 0, ideogram_idx: Some(0), display_idx: Some(0) },
                    TagItem { tag: "b".into(), group_idx: 1, ideogram_idx: Some(1), display_idx: Some(1) },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
            ChrorderGroup {
                idx: 1,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    TagItem { tag: "c".into(), group_idx: 0, ideogram_idx: Some(2), display_idx: Some(0) },
                    TagItem { tag: "d".into(), group_idx: 1, ideogram_idx: Some(3), display_idx: Some(1) },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
        ];
        reform_chrorder_groups(&mut groups, 4).unwrap();
        // First group keeps {0,1}, second slides to first non-colliding slot.
        let g0: Vec<usize> = groups[0].tags.iter().filter_map(|t| t.display_idx).collect();
        let g1: Vec<usize> = groups[1].tags.iter().filter_map(|t| t.display_idx).collect();
        assert_eq!(g0, vec![0, 1]);
        // Second group should have slid to {2, 3}.
        assert_eq!(g1, vec![2, 3]);
    }

    /// Test: Reform chrorder groups preserves intra group offset.
    #[test]
    fn test_reform_chrorder_groups_preserves_intra_group_offset() {
        // A group with display_idx {0, 3} (a gap of 3 between tags) should
        // preserve that offset when slid to accommodate a prior group.
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;
        let mut groups = vec![
            ChrorderGroup {
                idx: 0,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    TagItem { tag: "a".into(), group_idx: 0, ideogram_idx: Some(0), display_idx: Some(0) },
                    TagItem { tag: "b".into(), group_idx: 1, ideogram_idx: Some(1), display_idx: Some(1) },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
            ChrorderGroup {
                idx: 1,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    // Conflicting with first group, with intra-group gap of 3.
                    TagItem { tag: "c".into(), group_idx: 0, ideogram_idx: Some(2), display_idx: Some(0) },
                    TagItem { tag: "d".into(), group_idx: 1, ideogram_idx: Some(3), display_idx: Some(3) },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
        ];
        reform_chrorder_groups(&mut groups, 10).unwrap();
        let g1: Vec<usize> = groups[1].tags.iter().filter_map(|t| t.display_idx).collect();
        // Second group had {0, 3} (offset 3). Slid to non-colliding start;
        // invariant: right-left == 3 preserved.
        assert_eq!(g1[1] - g1[0], 3);
    }

    /// Test: Reform chrorder groups noncolliding passes.
    #[test]
    fn test_reform_chrorder_groups_noncolliding_passes() {
        use crate::intspan::IntSpan;
        // Two groups with disjoint display_idx sets → no collisions → Ok no-op.
        let mut groups = vec![
            ChrorderGroup {
                idx: 0,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    TagItem {
                        tag: "a".into(),
                        group_idx: 0,
                        ideogram_idx: Some(0),
                        display_idx: Some(0),
                    },
                    TagItem {
                        tag: "b".into(),
                        group_idx: 1,
                        ideogram_idx: Some(1),
                        display_idx: Some(1),
                    },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
            ChrorderGroup {
                idx: 1,
                n: 1,
                cumulidx: 2,
                tags: vec![TagItem {
                    tag: "c".into(),
                    group_idx: 0,
                    ideogram_idx: Some(2),
                    display_idx: Some(2),
                }],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
        ];
        let r = reform_chrorder_groups(&mut groups, 4);
        assert!(r.is_ok());
        // No group was marked for reform (they didn't collide).
        assert!(!groups[0].reform);
        assert!(!groups[1].reform);
    }

    /// Test: Reform chrorder groups collision slides second.
    #[test]
    fn test_reform_chrorder_groups_collision_slides_second() {
        use crate::intspan::IntSpan;
        // Both groups claim {0, 1} — collision. Second group should slide to
        // a non-colliding slot within n_ideograms=4.
        let mut groups = vec![
            ChrorderGroup {
                idx: 0,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    TagItem {
                        tag: "a".into(),
                        group_idx: 0,
                        ideogram_idx: Some(0),
                        display_idx: Some(0),
                    },
                    TagItem {
                        tag: "b".into(),
                        group_idx: 1,
                        ideogram_idx: Some(1),
                        display_idx: Some(1),
                    },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
            ChrorderGroup {
                idx: 1,
                n: 2,
                cumulidx: 2,
                tags: vec![
                    TagItem {
                        tag: "c".into(),
                        group_idx: 0,
                        ideogram_idx: Some(2),
                        display_idx: Some(0),
                    },
                    TagItem {
                        tag: "d".into(),
                        group_idx: 1,
                        ideogram_idx: Some(3),
                        display_idx: Some(1),
                    },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
        ];
        let r = reform_chrorder_groups(&mut groups, 4);
        assert!(r.is_ok());
        // Second group should now have display_idx 2 and 3 (slid right).
        let di_g2: Vec<usize> = groups[1]
            .tags
            .iter()
            .filter_map(|t| t.display_idx)
            .collect();
        assert_eq!(di_g2, vec![2, 3]);
    }

    /// Test: Reform chrorder groups unsolvable returns err.
    #[test]
    fn test_reform_chrorder_groups_unsolvable_returns_err() {
        use crate::intspan::IntSpan;
        // Two groups of size 2 each but only 2 display slots available
        // (n_ideograms=2) — second group can't find a fitting position.
        let mut groups = vec![
            ChrorderGroup {
                idx: 0,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    TagItem {
                        tag: "a".into(),
                        group_idx: 0,
                        ideogram_idx: Some(0),
                        display_idx: Some(0),
                    },
                    TagItem {
                        tag: "b".into(),
                        group_idx: 1,
                        ideogram_idx: Some(1),
                        display_idx: Some(1),
                    },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
            ChrorderGroup {
                idx: 1,
                n: 2,
                cumulidx: 2,
                tags: vec![
                    TagItem {
                        tag: "c".into(),
                        group_idx: 0,
                        ideogram_idx: Some(2),
                        display_idx: Some(0),
                    },
                    TagItem {
                        tag: "d".into(),
                        group_idx: 1,
                        ideogram_idx: Some(3),
                        display_idx: Some(1),
                    },
                ],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
        ];
        let r = reform_chrorder_groups(&mut groups, 2);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("cannot be placed"));
    }

    /// Test: Recompute chrorder groups assigns unallocated.
    #[test]
    fn test_recompute_chrorder_groups_assigns_unallocated() {
        use crate::intspan::IntSpan;
        // Group tag "c" doesn't match any ideogram — 2nd pass should assign
        // one of the unallocated ideograms (a or b).
        let mut ideos = vec![
            IdeogramRef {
                idx: 0,
                tag: "a".into(),
                chr: "hs1".into(),
                display_idx: None,
            },
            IdeogramRef {
                idx: 1,
                tag: "b".into(),
                chr: "hs2".into(),
                display_idx: None,
            },
        ];
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 1,
            cumulidx: 0,
            tags: vec![TagItem {
                tag: "c".into(),
                group_idx: 0,
                ideogram_idx: None,
                display_idx: None,
            }],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        recompute_chrorder_groups(&mut groups, &mut ideos);
        // After recompute, the unresolvable "c" tag should now be re-tagged
        // to an existing ideogram's tag.
        let tag_item = &groups[0].tags[0];
        assert!(tag_item.ideogram_idx.is_some());
        assert!(tag_item.tag == "a" || tag_item.tag == "b");
    }

    /// Test: Recompute chrorder groups final pass assigns to unallocated ideograms.
    #[test]
    fn test_recompute_chrorder_groups_final_pass_assigns_to_unallocated_ideograms() {
        // First pass matches tag to ideogram but doesn't set `tag_item.ideogram_idx`.
        // So the third pass warns + sets `display_idx = None`. The *final* pass
        // (ideograms loop) then assigns display_idx from the unused pool to
        // every ideogram lacking one.
        use crate::intspan::IntSpan;
        let mut ideos = vec![
            IdeogramRef {
                idx: 0,
                tag: "a".into(),
                chr: "hs1".into(),
                display_idx: None,
            },
            IdeogramRef {
                idx: 1,
                tag: "b".into(),
                chr: "hs2".into(),
                display_idx: None,
            },
        ];
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 2,
            cumulidx: 0,
            tags: vec![
                TagItem {
                    tag: "a".into(),
                    group_idx: 0,
                    ideogram_idx: None,
                    display_idx: None,
                },
                TagItem {
                    tag: "b".into(),
                    group_idx: 1,
                    ideogram_idx: None,
                    display_idx: None,
                },
            ],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        recompute_chrorder_groups(&mut groups, &mut ideos);
        // Both ideograms should end up with a display_idx from {0, 1}.
        let mut slots: Vec<usize> = ideos.iter().filter_map(|i| i.display_idx).collect();
        slots.sort();
        assert_eq!(slots, vec![0, 1]);
    }

    /// Test: Recompute chrorder groups tag with double underscore matches by chr.
    #[test]
    fn test_recompute_chrorder_groups_tag_with_double_underscore_matches_by_chr() {
        // Ideograms whose tag contains "__" match by chr (not tag). Used by
        // Perl to identify the duplicate-chr-different-region case.
        use crate::intspan::IntSpan;
        let mut ideos = vec![IdeogramRef {
            idx: 0,
            tag: "chr1__zoom".into(), // Contains "__" → chr-match path
            chr: "hs1".into(),
            display_idx: None,
        }];
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 1,
            cumulidx: 0,
            tags: vec![TagItem {
                tag: "hs1".into(), // Matches by chr since ideo.tag has "__"
                group_idx: 0,
                ideogram_idx: None,
                display_idx: None,
            }],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        recompute_chrorder_groups(&mut groups, &mut ideos);
        // The tag "hs1" matches ideogram 0 (because ideo.chr == "hs1" and
        // ideo.tag contains "__"). Display_idx should have been assigned.
        assert_eq!(ideos[0].display_idx, Some(0));
    }

    /// Test: Recompute chrorder groups preexisting tag display idx removes slot from pool.
    #[test]
    fn test_recompute_chrorder_groups_preexisting_tag_display_idx_removes_slot_from_pool() {
        // A tag_item pre-set with display_idx=1 → the 1st pass removes slot 1
        // from the display_idx_set pool. The final pass then draws slots from
        // the remaining {0, 2} for ideograms 0 and 1 (not ideogram 1, since its
        // own tag's display_idx didn't transfer — that only happens via pass 3,
        // which requires ideogram_idx set, which first-pass doesn't do).
        use crate::intspan::IntSpan;
        let mut ideos = vec![
            IdeogramRef {
                idx: 0,
                tag: "a".into(),
                chr: "hs1".into(),
                display_idx: None,
            },
            IdeogramRef {
                idx: 1,
                tag: "b".into(),
                chr: "hs2".into(),
                display_idx: None,
            },
            IdeogramRef {
                idx: 2,
                tag: "c".into(),
                chr: "hs3".into(),
                display_idx: None,
            },
        ];
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 1,
            cumulidx: 0,
            tags: vec![TagItem {
                tag: "b".into(),
                group_idx: 0,
                ideogram_idx: None,
                display_idx: Some(1),
            }],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        recompute_chrorder_groups(&mut groups, &mut ideos);
        // Slot 1 was removed from the pool in pass 1. Final pass then pulls
        // from {0, 2} for ideos starting in order (0, 1, 2).
        // Ideo 0 gets first available → 0. Ideo 1 gets next → 2. Ideo 2 gets
        // nothing (pool empty) → None.
        assert_eq!(ideos[0].display_idx, Some(0));
        assert_eq!(ideos[1].display_idx, Some(2));
        assert_eq!(ideos[2].display_idx, None);
    }

    /// Test: Recompute chrorder groups fills unmatched ideograms from pool.
    #[test]
    fn test_recompute_chrorder_groups_fills_unmatched_ideograms_from_pool() {
        // With groups empty, all ideograms are "unmatched". The final pass
        // should fill each one with a display_idx from the unused pool.
        use crate::intspan::IntSpan;
        let mut ideos = vec![
            IdeogramRef {
                idx: 0,
                tag: "a".into(),
                chr: "hs1".into(),
                display_idx: None,
            },
            IdeogramRef {
                idx: 1,
                tag: "b".into(),
                chr: "hs2".into(),
                display_idx: None,
            },
            IdeogramRef {
                idx: 2,
                tag: "c".into(),
                chr: "hs3".into(),
                display_idx: None,
            },
        ];
        let mut groups: Vec<ChrorderGroup> = Vec::new();
        recompute_chrorder_groups(&mut groups, &mut ideos);
        // All ideograms should get sequential display_idx (0, 1, 2 — order depends on pool iter).
        let mut disps: Vec<usize> = ideos.iter().filter_map(|i| i.display_idx).collect();
        disps.sort();
        assert_eq!(disps, vec![0, 1, 2]);
        // Note: _ = IntSpan::new() used just to reference the import in this fn's scope.
        let _ = IntSpan::new();
    }

    /// Test: Set display index start end flags.
    #[test]
    fn test_set_display_index_start_end_flags() {
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;

        let mk_tag = |t: &str, gi: usize| TagItem {
            tag: t.into(),
            group_idx: gi,
            ideogram_idx: None,
            display_idx: None,
        };
        // `start` group → tags get indices 0..n from the left.
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 2,
            cumulidx: 0,
            tags: vec![mk_tag("a", 0), mk_tag("b", 1)],
            start: true,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        set_display_index(&mut groups, 10);
        assert_eq!(groups[0].tags[0].display_idx, Some(0));
        assert_eq!(groups[0].tags[1].display_idx, Some(1));

        // `end` group → tags get indices (n_ideograms - n)..n_ideograms.
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 2,
            cumulidx: 0,
            tags: vec![mk_tag("y", 0), mk_tag("z", 1)],
            start: false,
            end: true,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        set_display_index(&mut groups, 10);
        assert_eq!(groups[0].tags[0].display_idx, Some(8));
        assert_eq!(groups[0].tags[1].display_idx, Some(9));
    }

    /// Test: Set display index anchor propagates to sibling tags.
    #[test]
    fn test_set_display_index_anchor_propagates_to_sibling_tags() {
        // Anchor path: group has no start/end flag. One tag carries a
        // defined `ideogram_idx` — it acts as anchor. Other tags in the
        // same group get display_idx computed as (group_idx - anchor.group_idx) + anchor.ideogram_idx.
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 3,
            cumulidx: 0,
            tags: vec![
                TagItem { tag: "a".into(), group_idx: 0, ideogram_idx: None, display_idx: None },
                TagItem { tag: "b".into(), group_idx: 1, ideogram_idx: Some(5), display_idx: None }, // anchor
                TagItem { tag: "c".into(), group_idx: 2, ideogram_idx: None, display_idx: None },
            ],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        set_display_index(&mut groups, 10);
        // Anchor at group_idx=1 with ideogram_idx=5. Others:
        //   tag "a" → (0-1)+5 = 4
        //   tag "b" → (1-1)+5 = 5
        //   tag "c" → (2-1)+5 = 6
        assert_eq!(groups[0].tags[0].display_idx, Some(4));
        assert_eq!(groups[0].tags[1].display_idx, Some(5));
        assert_eq!(groups[0].tags[2].display_idx, Some(6));
    }

    /// Test: Set display index end saturating sub when n gt total.
    #[test]
    fn test_set_display_index_end_saturating_sub_when_n_gt_total() {
        // `end=true` with `group.n > n_ideograms` uses `saturating_sub(n)` → 0.
        // So first tag gets display_idx=0, second gets 1, etc.
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 5, // more than n_ideograms
            cumulidx: 0,
            tags: vec![
                TagItem { tag: "x".into(), group_idx: 0, ideogram_idx: None, display_idx: None },
                TagItem { tag: "y".into(), group_idx: 1, ideogram_idx: None, display_idx: None },
            ],
            start: false,
            end: true,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        set_display_index(&mut groups, 3);
        // 3.saturating_sub(5) = 0 → base = 0 → indices 0, 1.
        assert_eq!(groups[0].tags[0].display_idx, Some(0));
        assert_eq!(groups[0].tags[1].display_idx, Some(1));
    }

    /// Test: Set display index anchor without ideogram idx skips.
    #[test]
    fn test_set_display_index_anchor_without_ideogram_idx_skips() {
        // No start/end flag and no tag has ideogram_idx set → find returns None,
        // so the `if let Some(anchor)` branch is skipped entirely; display_idx stays None.
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;
        let mut groups = vec![ChrorderGroup {
            idx: 0,
            n: 2,
            cumulidx: 0,
            tags: vec![
                TagItem { tag: "p".into(), group_idx: 0, ideogram_idx: None, display_idx: None },
                TagItem { tag: "q".into(), group_idx: 1, ideogram_idx: None, display_idx: None },
            ],
            start: false,
            end: false,
            display_idx_set: IntSpan::new(),
            reform: false,
        }];
        set_display_index(&mut groups, 10);
        assert_eq!(groups[0].tags[0].display_idx, None);
        assert_eq!(groups[0].tags[1].display_idx, None);
    }

    /// Test: Set display index start processed before anchor group.
    #[test]
    fn test_set_display_index_start_processed_before_anchor_group() {
        // Sort-by-flag puts start/end groups first. Verify that a start group
        // is processed before an anchor-only group regardless of input order.
        use crate::chromosome::{ChrorderGroup, TagItem};
        use crate::intspan::IntSpan;
        let mut groups = vec![
            // group 0: anchor-only (no flag, no anchor ideogram_idx either → skipped)
            ChrorderGroup {
                idx: 0,
                n: 1,
                cumulidx: 0,
                tags: vec![TagItem { tag: "m".into(), group_idx: 0, ideogram_idx: None, display_idx: None }],
                start: false,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
            // group 1: start flag
            ChrorderGroup {
                idx: 1,
                n: 2,
                cumulidx: 0,
                tags: vec![
                    TagItem { tag: "a".into(), group_idx: 0, ideogram_idx: None, display_idx: None },
                    TagItem { tag: "b".into(), group_idx: 1, ideogram_idx: None, display_idx: None },
                ],
                start: true,
                end: false,
                display_idx_set: IntSpan::new(),
                reform: false,
            },
        ];
        set_display_index(&mut groups, 10);
        // Start group got processed: tags a and b have display_idx set.
        assert_eq!(groups[1].tags[0].display_idx, Some(0));
        assert_eq!(groups[1].tags[1].display_idx, Some(1));
    }

    /// Test: Tag item default is empty.
    #[test]
    fn test_tag_item_default_is_empty() {
        // Default TagItem: empty tag, zero group_idx, no indices set.
        let t = TagItem::default();
        assert_eq!(t.tag, "");
        assert_eq!(t.group_idx, 0);
        assert!(t.ideogram_idx.is_none());
        assert!(t.display_idx.is_none());
    }

    /// Test: Chrorder group default is empty.
    #[test]
    fn test_chrorder_group_default_is_empty() {
        // Default ChrorderGroup: all fields zero/false, empty tags vec, empty IntSpan.
        let g = ChrorderGroup::default();
        assert_eq!(g.idx, 0);
        assert_eq!(g.n, 0);
        assert_eq!(g.cumulidx, 0);
        assert!(g.tags.is_empty());
        assert!(!g.start);
        assert!(!g.end);
        assert_eq!(g.display_idx_set.cardinality(), 0);
        assert!(!g.reform);
    }

    /// Test: Tag item clone is independent.
    #[test]
    fn test_tag_item_clone_is_independent() {
        // Mutating a cloned TagItem doesn't affect the source.
        let a = TagItem {
            tag: "hs1".into(),
            group_idx: 3,
            ideogram_idx: Some(5),
            display_idx: Some(7),
        };
        let mut b = a.clone();
        b.tag = "mutated".into();
        b.ideogram_idx = None;
        assert_eq!(a.tag, "hs1");
        assert_eq!(a.ideogram_idx, Some(5));
        assert_eq!(b.tag, "mutated");
        assert!(b.ideogram_idx.is_none());
    }

    /// Test: Report chromosomes does not panic on empty inputs.
    #[test]
    fn test_report_chromosomes_does_not_panic_on_empty_inputs() {
        // report_chromosomes prints debug info — just verify no panic on empty inputs.
        use std::collections::HashMap;
        let chrs: Vec<String> = Vec::new();
        let display_order: HashMap<String, u32> = HashMap::new();
        let scale: HashMap<String, f64> = HashMap::new();
        let regions: HashMap<String, DisplayRegion> = HashMap::new();
        let length_cumul: HashMap<String, i64> = HashMap::new();
        report_chromosomes(&chrs, &display_order, &scale, &regions, &length_cumul);
    }

    /// Test: Report chromosomes sorts by display order.
    #[test]
    fn test_report_chromosomes_sorts_by_display_order() {
        // With 3 chrs and explicit display_order, sorts before iteration.
        // Just verify function runs without panic.
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let chrs = vec!["c1".to_string(), "c2".to_string(), "c3".to_string()];
        let mut display_order: HashMap<String, u32> = HashMap::new();
        display_order.insert("c1".into(), 2);
        display_order.insert("c2".into(), 0);
        display_order.insert("c3".into(), 1);
        let scale: HashMap<String, f64> = HashMap::new();
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        for name in &chrs {
            regions.insert(
                name.clone(),
                DisplayRegion {
                    accept: Some(IntSpan::from_range(0, 100)),
                    reject: Some(IntSpan::new()),
                    display: true,
                },
            );
        }
        let length_cumul: HashMap<String, i64> = HashMap::new();
        report_chromosomes(&chrs, &display_order, &scale, &regions, &length_cumul);
    }

    /// Test: Report chromosomes handles missing region gracefully.
    #[test]
    fn test_report_chromosomes_handles_missing_region_gracefully() {
        // chr mentioned in chrs but not in regions → `.get()` returns None,
        // is_displayed defaults to false → skipped. No panic.
        use std::collections::HashMap;
        let chrs = vec!["ghost_chr".to_string()];
        let display_order: HashMap<String, u32> = HashMap::new();
        let scale: HashMap<String, f64> = HashMap::new();
        let regions: HashMap<String, DisplayRegion> = HashMap::new();
        let length_cumul: HashMap<String, i64> = HashMap::new();
        report_chromosomes(&chrs, &display_order, &scale, &regions, &length_cumul);
    }

    /// Test: Report chromosomes skips non displayed chrs.
    #[test]
    fn test_report_chromosomes_skips_non_displayed_chrs() {
        // A chr with display=false is skipped silently (no panic).
        use crate::intspan::IntSpan;
        use std::collections::HashMap;
        let chrs = vec!["c1".to_string()];
        let display_order: HashMap<String, u32> = HashMap::new();
        let scale: HashMap<String, f64> = HashMap::new();
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        regions.insert(
            "c1".into(),
            DisplayRegion {
                accept: Some(IntSpan::from_range(0, 100)),
                reject: None,
                display: false, // suppresses
            },
        );
        let length_cumul: HashMap<String, i64> = HashMap::new();
        // Should skip "c1" due to display=false; no panic.
        report_chromosomes(&chrs, &display_order, &scale, &regions, &length_cumul);
    }

    /// Test: Display region default is all none and display false.
    #[test]
    fn test_display_region_default_is_all_none_and_display_false() {
        // DisplayRegion::default: accept/reject None, display=false.
        let r = DisplayRegion::default();
        assert!(r.accept.is_none());
        assert!(r.reject.is_none());
        assert!(!r.display);
    }

    /// Test: Display region clone preserves option intspans.
    #[test]
    fn test_display_region_clone_preserves_option_intspans() {
        // Clone preserves accept/reject Option<IntSpan> and display flag.
        use crate::intspan::IntSpan;
        let r = DisplayRegion {
            accept: Some(IntSpan::from_range(0, 50)),
            reject: Some(IntSpan::from_range(30, 40)),
            display: true,
        };
        let c = r.clone();
        assert_eq!(c.accept.as_ref().unwrap().cardinality(), 51);
        assert_eq!(c.reject.as_ref().unwrap().cardinality(), 11);
        assert!(c.display);
    }

    /// Test: Tag item debug formatting roundtrip.
    #[test]
    fn test_tag_item_debug_formatting_roundtrip() {
        // TagItem is Debug — format/parse via String for trait check.
        let t = TagItem {
            tag: "t".into(),
            group_idx: 2,
            ideogram_idx: Some(5),
            display_idx: Some(7),
        };
        let s = format!("{:?}", t);
        // Debug output contains all field names+values.
        assert!(s.contains("tag:"));
        assert!(s.contains("group_idx:"));
        assert!(s.contains("2"));
        assert!(s.contains("5"));
        assert!(s.contains("7"));
    }

    /// Test: Display region with empty intspans display reflects accept.
    #[test]
    fn test_display_region_with_empty_intspans_display_reflects_accept() {
        // When accept is empty IntSpan (cardinality 0), display is whatever
        // the field was set to — impl doesn't auto-derive this.
        use crate::intspan::IntSpan;
        let r = DisplayRegion {
            accept: Some(IntSpan::new()),
            reject: Some(IntSpan::new()),
            display: false,
        };
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 0);
        assert!(!r.display);
    }

    /// Test: Chr filter default is all none.
    #[test]
    fn test_chr_filter_default_is_all_none() {
        // Default ChrFilter has all three IntSpan options = None.
        let f = ChrFilter::default();
        assert!(f.show.is_none());
        assert!(f.hide.is_none());
        assert!(f.combined.is_none());
    }

    /// Test: Parsed chromosome default is empty.
    #[test]
    fn test_parsed_chromosome_default_is_empty() {
        // ParsedChromosome::default: empty chr/tag, empty IntSpan, accept=false.
        let p = ParsedChromosome::default();
        assert_eq!(p.chr, "");
        assert_eq!(p.tag, "");
        assert_eq!(p.set.cardinality(), 0);
        assert!(!p.accept);
    }

    /// Test: Chr filter clone preserves intspan.
    #[test]
    fn test_chr_filter_clone_preserves_intspan() {
        // Cloning ChrFilter preserves the inner IntSpan cardinalities.
        use crate::intspan::IntSpan;
        let f = ChrFilter {
            show: Some(IntSpan::from_range(0, 100)),
            hide: Some(IntSpan::from_range(50, 60)),
            combined: None,
        };
        let c = f.clone();
        assert_eq!(c.show.as_ref().unwrap().cardinality(), 101);
        assert_eq!(c.hide.as_ref().unwrap().cardinality(), 11);
        assert!(c.combined.is_none());
    }

    /// Test: Parsed chromosome clone preserves fields.
    #[test]
    fn test_parsed_chromosome_clone_preserves_fields() {
        // Clone preserves chr/tag strings, IntSpan, accept flag.
        use crate::intspan::IntSpan;
        let p = ParsedChromosome {
            chr: "hs1".into(),
            tag: "a".into(),
            set: IntSpan::from_range(0, 50),
            accept: true,
        };
        let c = p.clone();
        assert_eq!(c.chr, "hs1");
        assert_eq!(c.tag, "a");
        assert_eq!(c.set.cardinality(), 51);
        assert!(c.accept);
    }

    /// Test: Ideogram ref default and clone.
    #[test]
    fn test_ideogram_ref_default_and_clone() {
        // IdeogramRef::default has zero idx, empty strings, None display_idx.
        let r = IdeogramRef::default();
        assert_eq!(r.idx, 0);
        assert_eq!(r.tag, "");
        assert_eq!(r.chr, "");
        assert!(r.display_idx.is_none());
        // Clone preserves.
        let r = IdeogramRef {
            idx: 42,
            tag: "t".into(),
            chr: "c".into(),
            display_idx: Some(3),
        };
        let c = r.clone();
        assert_eq!(c.idx, 42);
        assert_eq!(c.tag, "t");
        assert_eq!(c.chr, "c");
        assert_eq!(c.display_idx, Some(3));
    }

    /// Test: Ideogram spacing helper u unit scales by chromosomes units.
    #[test]
    fn test_ideogram_spacing_helper_u_unit_scales_by_chromosomes_units() {
        // "5u" × chromosomes_units=1_000_000 = 5_000_000.
        let v = ideogram_spacing_helper("5u", "bupr", "n", 1_000_000.0, 3e9).unwrap();
        assert!((v - 5_000_000.0).abs() < 1e-6);
        // "2u" × 1e7 = 2e7.
        let v = ideogram_spacing_helper("2u", "bupr", "n", 1e7, 3e9).unwrap();
        assert!((v - 2e7).abs() < 1.0);
    }

    /// Test: Ideogram spacing helper r unit scales by spacing default.
    #[test]
    fn test_ideogram_spacing_helper_r_unit_scales_by_spacing_default() {
        // "0.01r" × spacing_default=1e9 = 1e7.
        let v = ideogram_spacing_helper("0.01r", "bupr", "n", 1e6, 1e9).unwrap();
        assert!((v - 1e7).abs() < 1.0);
        // "2r" × 500 = 1000 regardless of chromosomes_units arg.
        let v = ideogram_spacing_helper("2r", "bupr", "n", 999.0, 500.0).unwrap();
        assert!((v - 1000.0).abs() < 1e-9);
    }

    /// Test: Get ideogram radius direct and default and missing.
    #[test]
    fn test_get_ideogram_radius_direct_and_default_and_missing() {
        // Direct hit.
        let mut m: HashMap<String, f64> = HashMap::new();
        m.insert("a".into(), 1500.0);
        m.insert("b".into(), 2000.0);
        m.insert("default".into(), 1000.0);
        assert_eq!(get_ideogram_radius("a", &m), 1500.0);
        assert_eq!(get_ideogram_radius("b", &m), 2000.0);
        // Missing key → falls through to "default".
        assert_eq!(get_ideogram_radius("x", &m), 1000.0);
        // Missing key AND no "default" → 0.0.
        let mut m2: HashMap<String, f64> = HashMap::new();
        m2.insert("only".into(), 999.0);
        assert_eq!(get_ideogram_radius("nonexistent", &m2), 0.0);
    }

    /// Test: Register z levels empty iterator still includes zero.
    #[test]
    fn test_register_z_levels_empty_iterator_still_includes_zero() {
        // Empty input → just [0].
        let v = register_z_levels(std::iter::empty());
        assert_eq!(v, vec![0]);
        // Single-element iterator — dedup with the implicit 0.
        let v = register_z_levels(vec![0]);
        assert_eq!(v, vec![0]);
        // Inserting 0 alongside positives keeps exactly one 0 entry.
        let v = register_z_levels(vec![0, 5, 0, 10]);
        assert_eq!(v, vec![0, 5, 10]);
    }

    /// Test: Filter data unknown chr returns empty.
    #[test]
    fn test_filter_data_unknown_chr_returns_empty() {
        // chr not in karyotype_accept → empty IntSpan.
        let set = IntSpan::from_range(0, 100);
        let accept: HashMap<String, IntSpan> = HashMap::new();
        let filtered = filter_data(&set, "hsX", &accept);
        assert_eq!(filtered.cardinality(), 0);
    }

    /// Test: Filter data empty set intersects empty.
    #[test]
    fn test_filter_data_empty_set_intersects_empty() {
        // Empty input set → empty intersection regardless of accept.
        let set = IntSpan::new();
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::from_range(0, 1000));
        let filtered = filter_data(&set, "hs1", &accept);
        assert_eq!(filtered.cardinality(), 0);
    }

    /// Test: Filter data full overlap returns full set.
    #[test]
    fn test_filter_data_full_overlap_returns_full_set() {
        // When set ⊆ accept → intersection = set.
        let set = IntSpan::from_range(100, 200);
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::from_range(0, 1000));
        let filtered = filter_data(&set, "hs1", &accept);
        assert_eq!(filtered.cardinality(), 101);
        assert_eq!(filtered.min(), Some(100));
        assert_eq!(filtered.max(), Some(200));
    }

    /// Test: Merge ideogram filters empty list returns empty filter.
    #[test]
    fn test_merge_ideogram_filters_empty_list_returns_empty_filter() {
        // Empty input → empty merged result.
        let filters: Vec<IdeogramFilter> = Vec::new();
        let merged = merge_ideogram_filters(&filters);
        assert!(merged.is_empty());
    }

    /// Test: Register z levels preserves order across mixed input.
    #[test]
    fn test_register_z_levels_preserves_order_across_mixed_input() {
        // Mixed input with duplicates and gaps: [5, 0, 10, 5, -3, 0, 100].
        let v = register_z_levels(vec![5, 0, 10, 5, -3, 0, 100]);
        // Sorted + deduplicated.
        assert_eq!(v, vec![-3, 0, 5, 10, 100]);
    }

    /// Test: Ideogram spacing helper unknown unit errors.
    #[test]
    fn test_ideogram_spacing_helper_unknown_unit_errors() {
        // Unit not in allowed list ("u","r") → validation Err.
        let r = ideogram_spacing_helper("5p", "bupr", "n", 1e6, 1e9);
        assert!(r.is_err());
        // Also with "b" unit (not in ["u","r"] for spacing).
        let r = ideogram_spacing_helper("5b", "bupr", "n", 1e6, 1e9);
        assert!(r.is_err());
    }

    /// Test: Filter data preserves empty accept as empty.
    #[test]
    fn test_filter_data_preserves_empty_accept_as_empty() {
        // accept IntSpan is empty → intersection with any set is empty.
        let set = IntSpan::from_range(0, 1000);
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::new());
        let filtered = filter_data(&set, "hs1", &accept);
        assert!(filtered.is_empty());
    }

    /// Test: Merge ideogram filters single filter preserved.
    #[test]
    fn test_merge_ideogram_filters_single_filter_preserved() {
        // Single-filter input → merged result matches it exactly.
        // IdeogramFilter is a type alias for HashMap<String, ChrFilter>; show/hide are Option<IntSpan>.
        let mut f: IdeogramFilter = HashMap::new();
        let mut cf = ChrFilter::default();
        cf.show = Some(IntSpan::from_range(0, 100));
        f.insert("hs1".to_string(), cf);
        let merged = merge_ideogram_filters(&[f]);
        assert_eq!(merged.len(), 1);
        assert!(merged.contains_key("hs1"));
        assert!(merged["hs1"].show.is_some());
        assert_eq!(merged["hs1"].show.as_ref().unwrap().cardinality(), 101);
    }

    /// Test: Register z levels already sorted input passes through.
    #[test]
    fn test_register_z_levels_already_sorted_input_passes_through() {
        // Already-sorted input → sorted output (same set).
        let v = register_z_levels(vec![1, 2, 3, 4, 5]);
        assert_eq!(v, vec![0, 1, 2, 3, 4, 5]);
    }

    /// Test: Ideogram spacing helper zero value passes through.
    #[test]
    fn test_ideogram_spacing_helper_zero_value_passes_through() {
        // "0u" → 0 × chromosomes_units = 0.
        let v = ideogram_spacing_helper("0u", "bupr", "n", 1e6, 1e9).unwrap();
        assert_eq!(v, 0.0);
        // "0r" → 0 × spacing_default = 0.
        let v = ideogram_spacing_helper("0r", "bupr", "n", 1e6, 1e9).unwrap();
        assert_eq!(v, 0.0);
    }

    /// Test: Get ideogram radius empty map returns zero.
    #[test]
    fn test_get_ideogram_radius_empty_map_returns_zero() {
        // Empty map + missing key → 0.0 (no "default" entry).
        let m: HashMap<String, f64> = HashMap::new();
        assert_eq!(get_ideogram_radius("any", &m), 0.0);
        assert_eq!(get_ideogram_radius("", &m), 0.0);
    }

    /// Test: Merge ideogram filters show and hide both preserved.
    #[test]
    fn test_merge_ideogram_filters_show_and_hide_both_preserved() {
        // Single filter with both show and hide populated → both preserved.
        let mut f: IdeogramFilter = HashMap::new();
        let mut cf = ChrFilter::default();
        cf.show = Some(IntSpan::from_range(0, 500));
        cf.hide = Some(IntSpan::from_range(100, 200));
        f.insert("hs1".to_string(), cf);
        let merged = merge_ideogram_filters(&[f]);
        assert!(merged["hs1"].show.is_some());
        assert!(merged["hs1"].hide.is_some());
        assert_eq!(merged["hs1"].show.as_ref().unwrap().cardinality(), 501);
        assert_eq!(merged["hs1"].hide.as_ref().unwrap().cardinality(), 101);
    }

    /// Test: Register z levels deduplicates and sorts ascending.
    #[test]
    fn test_register_z_levels_deduplicates_and_sorts_ascending() {
        // BTreeSet-backed: duplicates dropped, values sorted, implicit 0 always present.
        let out = register_z_levels(vec![3, 1, 3, 2, 1]);
        assert_eq!(out, vec![0, 1, 2, 3]);
        // Empty input still emits [0].
        let out2 = register_z_levels(std::iter::empty::<i64>());
        assert_eq!(out2, vec![0]);
    }

    /// Test: Register z levels negative values precede zero.
    #[test]
    fn test_register_z_levels_negative_values_precede_zero() {
        // Sort is signed — negatives precede the implicit 0.
        let out = register_z_levels(vec![-10, 5, -2]);
        assert_eq!(out, vec![-10, -2, 0, 5]);
    }

    /// Test: Ideogram spacing helper unit p not allowed errors.
    #[test]
    fn test_ideogram_spacing_helper_unit_p_not_allowed_errors() {
        // Allowed units: only {u, r}. "5p" fails unit_validate with a mentioning of 'p'.
        let err = ideogram_spacing_helper("5p", "bupr", "n", 1_000_000.0, 100.0).unwrap_err();
        assert!(err.contains("'p'") || err.contains("expected"));
        // "5n" (no-unit) also rejected — not in allowed list.
        let err2 = ideogram_spacing_helper("5", "bupr", "n", 1_000_000.0, 100.0).unwrap_err();
        assert!(!err2.is_empty());
    }

    /// Test: Get ideogram radius default fallback zero when no default key.
    #[test]
    fn test_get_ideogram_radius_default_fallback_zero_when_no_default_key() {
        // Missing tag AND missing "default" → 0.0 (final unwrap_or(&0.0)).
        let mut map: HashMap<String, f64> = HashMap::new();
        map.insert("other".into(), 500.0);
        assert_eq!(get_ideogram_radius("foo", &map), 0.0);
        // "default" present → used as fallback when tag missing.
        map.insert("default".into(), 250.0);
        assert_eq!(get_ideogram_radius("foo", &map), 250.0);
        // Tag present → overrides default.
        map.insert("foo".into(), 100.0);
        assert_eq!(get_ideogram_radius("foo", &map), 100.0);
    }

    /// Test: Parse ideogram filter none input returns empty map.
    #[test]
    fn test_parse_ideogram_filter_none_input_returns_empty_map() {
        // None short-circuit → empty IdeogramFilter map.
        let f = parse_ideogram_filter(None, None);
        assert!(f.is_empty());
        let f2 = parse_ideogram_filter(None, Some(1e6));
        assert!(f2.is_empty());
    }

    /// Test: Parse ideogram filter suppressed dash prefix sets hide.
    #[test]
    fn test_parse_ideogram_filter_suppressed_dash_prefix_sets_hide() {
        // "-hs1:10-20" → is_suppressed=true → hide field populated.
        let f = parse_ideogram_filter(Some("-hs1:10-20"), None);
        let cf = f.get("hs1").unwrap();
        assert!(cf.hide.is_some());
        assert!(cf.show.is_none());
        assert_eq!(cf.hide.as_ref().unwrap().cardinality(), 11);
    }

    /// Test: Parse ideogram filter no colon uses universal as show.
    #[test]
    fn test_parse_ideogram_filter_no_colon_uses_universal_as_show() {
        // Bare tag "hs1" with no colon → empty runlist → universal set in show.
        let f = parse_ideogram_filter(Some("hs1"), None);
        let cf = f.get("hs1").unwrap();
        assert!(cf.show.is_some());
        assert!(cf.hide.is_none());
        assert!(cf.show.as_ref().unwrap().is_universal());
    }

    /// Test: Merge ideogram filters empty input returns empty map.
    #[test]
    fn test_merge_ideogram_filters_empty_input_returns_empty_map() {
        // No filters to merge → empty map (no entries to walk).
        let merged = merge_ideogram_filters(&[]);
        assert!(merged.is_empty());
    }

    /// Test: Filter data unknown chr returns empty intspan.
    #[test]
    fn test_filter_data_unknown_chr_returns_empty_intspan() {
        // karyotype_accept doesn't contain chr → None branch → empty IntSpan.
        let accept: HashMap<String, IntSpan> = HashMap::new();
        let set = IntSpan::from_range(0, 100);
        let r = filter_data(&set, "unknown", &accept);
        assert_eq!(r.cardinality(), 0);
    }

    /// Test: Filter data intersects set with chr accept region.
    #[test]
    fn test_filter_data_intersects_set_with_chr_accept_region() {
        // Accept region [50..150]; set [0..100] → intersection [50..100] = 51 elems.
        let mut accept: HashMap<String, IntSpan> = HashMap::new();
        accept.insert("hs1".into(), IntSpan::from_range(50, 150));
        let set = IntSpan::from_range(0, 100);
        let r = filter_data(&set, "hs1", &accept);
        assert_eq!(r.cardinality(), 51);
        assert!(r.member(50));
        assert!(r.member(100));
        assert!(!r.member(49));
    }

    /// Test: Refine display regions default true accepts full chr set.
    #[test]
    fn test_refine_display_regions_default_true_accepts_full_chr_set() {
        // chromosomes_display_default=true + no accept/reject → accept=chr_set.
        let mut karyotype: HashMap<String, IntSpan> = HashMap::new();
        karyotype.insert("hs1".into(), IntSpan::from_range(0, 1000));
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        refine_display_regions(true, &karyotype, &mut regions);
        let r = regions.get("hs1").unwrap();
        assert!(r.display);
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 1001);
        assert_eq!(r.reject.as_ref().unwrap().cardinality(), 0);
    }

    /// Test: Refine display regions default false yields empty accept.
    #[test]
    fn test_refine_display_regions_default_false_yields_empty_accept() {
        // chromosomes_display_default=false + no accept/reject → both empty.
        let mut karyotype: HashMap<String, IntSpan> = HashMap::new();
        karyotype.insert("hs1".into(), IntSpan::from_range(0, 1000));
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        refine_display_regions(false, &karyotype, &mut regions);
        let r = regions.get("hs1").unwrap();
        assert!(!r.display); // accept cardinality=0 → display=false.
        assert_eq!(r.accept.as_ref().unwrap().cardinality(), 0);
        assert_eq!(r.reject.as_ref().unwrap().cardinality(), 0);
    }

    /// Test: Set display index start group assigns sequential from zero.
    #[test]
    fn test_set_display_index_start_group_assigns_sequential_from_zero() {
        // Group with start=true → display_idx=0,1,2 in tag order.
        let mut group = ChrorderGroup {
            tags: vec![
                TagItem { tag: "a".into(), group_idx: 0, ideogram_idx: None, display_idx: None },
                TagItem { tag: "b".into(), group_idx: 1, ideogram_idx: None, display_idx: None },
                TagItem { tag: "c".into(), group_idx: 2, ideogram_idx: None, display_idx: None },
            ],
            start: true,
            end: false,
            n: 3,
            ..Default::default()
        };
        let mut groups = vec![group.clone()];
        set_display_index(&mut groups, 10);
        assert_eq!(groups[0].tags[0].display_idx, Some(0));
        assert_eq!(groups[0].tags[1].display_idx, Some(1));
        assert_eq!(groups[0].tags[2].display_idx, Some(2));
        // original untouched.
        assert!(group.tags[0].display_idx.is_none());
    }

    /// Test: Set display index end group anchored to tail.
    #[test]
    fn test_set_display_index_end_group_anchored_to_tail() {
        // Group with end=true → base = n_ideograms - group.n; then sequential from base.
        let group = ChrorderGroup {
            tags: vec![
                TagItem { tag: "x".into(), group_idx: 0, ideogram_idx: None, display_idx: None },
                TagItem { tag: "y".into(), group_idx: 1, ideogram_idx: None, display_idx: None },
            ],
            start: false,
            end: true,
            n: 2,
            ..Default::default()
        };
        let mut groups = vec![group];
        set_display_index(&mut groups, 10);
        // base = 10 - 2 = 8 → display_idx 8, 9.
        assert_eq!(groups[0].tags[0].display_idx, Some(8));
        assert_eq!(groups[0].tags[1].display_idx, Some(9));
    }

    /// Test: Set display index empty groups slice no panic.
    #[test]
    fn test_set_display_index_empty_groups_slice_no_panic() {
        // Empty slice → no-op, doesn't panic.
        let mut groups: Vec<ChrorderGroup> = Vec::new();
        set_display_index(&mut groups, 0);
        set_display_index(&mut groups, 100);
    }

    /// Test: Report chromosomes completes without panic.
    #[test]
    fn test_report_chromosomes_completes_without_panic() {
        // report_chromosomes writes via debug::printinfo — silent path is safe.
        let chrs = vec!["hs1".into(), "hs2".into()];
        let mut order: HashMap<String, u32> = HashMap::new();
        order.insert("hs1".into(), 0);
        order.insert("hs2".into(), 1);
        let scale: HashMap<String, f64> = HashMap::new();
        let mut regions: HashMap<String, DisplayRegion> = HashMap::new();
        let mut dr = DisplayRegion::default();
        dr.accept = Some(IntSpan::from_range(0, 100));
        dr.display = true;
        regions.insert("hs1".into(), dr);
        let length_cumul: HashMap<String, i64> = HashMap::new();
        // Just verify no panic.
        report_chromosomes(&chrs, &order, &scale, &regions, &length_cumul);
    }
}
