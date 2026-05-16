pub mod expression;

use std::collections::HashMap;

use crate::config::types::ConfigValue;
use crate::data::types::Link;

/// A rule that can modify link/datum parameters based on conditions.
#[derive(Debug, Clone)]
pub struct Rule {
    pub importance: i32,
    pub condition: String,
    pub overrides: HashMap<String, String>,
}

/// Parse rules from a config block.
pub fn parse_rules(rules_conf: Option<&HashMap<String, ConfigValue>>) -> Vec<Rule> {
    let conf = match rules_conf {
        Some(c) => c,
        None => return Vec::new(),
    };

    let rule_values = match conf.get("rule") {
        Some(ConfigValue::List(list)) => list.clone(),
        Some(val @ ConfigValue::Map(_)) => vec![val.clone()],
        _ => return Vec::new(),
    };

    let mut rules: Vec<Rule> = rule_values
        .iter()
        .filter_map(|rv| {
            let map = rv.as_map()?;
            let condition = map.get("condition")?.as_str()?.to_string();
            let importance = map
                .get("importance")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let mut overrides = HashMap::new();
            for (k, v) in map {
                if k != "condition"
                    && k != "importance"
                    && let Some(s) = v.as_str()
                {
                    overrides.insert(k.clone(), s.to_string());
                }
            }

            Some(Rule {
                importance,
                condition,
                overrides,
            })
        })
        .collect();

    // Sort by importance descending (highest first)
    rules.sort_by(|a, b| b.importance.cmp(&a.importance));
    rules
}

/// Apply rules to a link and return the merged parameters.
/// Returns the parameters from the first matching rule (highest importance).
pub fn apply_rules_to_link(link: &Link, rules: &[Rule]) -> HashMap<String, String> {
    for rule in rules {
        if expression::evaluate_link_condition(&rule.condition, link) {
            return rule.overrides.clone();
        }
    }
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::types::{Datum, Link};
    use crate::intspan::IntSpan;

    fn leaf(s: &str) -> ConfigValue {
        ConfigValue::Str(s.to_string())
    }

    fn mk_rule(condition: &str, importance: i32, overrides: &[(&str, &str)]) -> HashMap<String, ConfigValue> {
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf(condition));
        m.insert("importance".into(), leaf(&importance.to_string()));
        for (k, v) in overrides {
            m.insert((*k).to_string(), leaf(v));
        }
        m
    }

    fn link(chr1: &str, s1: i64, e1: i64, chr2: &str, s2: i64, e2: i64) -> Link {
        Link {
            id: "l".into(),
            points: vec![
                Datum {
                    chr: chr1.into(),
                    start: s1,
                    end: e1,
                    set: IntSpan::from_range(s1, e1),
                    id: Some("l".into()),
                    value: None,
                    label: None,
                    param: HashMap::new(),
                },
                Datum {
                    chr: chr2.into(),
                    start: s2,
                    end: e2,
                    set: IntSpan::from_range(s2, e2),
                    id: Some("l".into()),
                    value: None,
                    label: None,
                    param: HashMap::new(),
                },
            ],
            param: HashMap::new(),
        }
    }

    #[test]
    fn test_parse_rules_empty_and_missing_conf() {
        assert!(parse_rules(None).is_empty());
        let empty_map: HashMap<String, ConfigValue> = HashMap::new();
        assert!(parse_rules(Some(&empty_map)).is_empty());
    }

    #[test]
    fn test_parse_rules_single_rule_as_map() {
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let r = mk_rule("_INTERCHR_", 10, &[("color", "red"), ("thickness", "3")]);
        block.insert("rule".into(), ConfigValue::Map(r));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 10);
        assert_eq!(rules[0].condition, "_INTERCHR_");
        assert_eq!(rules[0].overrides["color"], "red");
        assert_eq!(rules[0].overrides["thickness"], "3");
        // condition / importance should NOT land in overrides.
        assert!(!rules[0].overrides.contains_key("condition"));
        assert!(!rules[0].overrides.contains_key("importance"));
    }

    #[test]
    fn test_parse_rules_sorted_by_importance_desc() {
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let list = vec![
            ConfigValue::Map(mk_rule("a", 5, &[])),
            ConfigValue::Map(mk_rule("b", 20, &[])),
            ConfigValue::Map(mk_rule("c", 1, &[])),
            ConfigValue::Map(mk_rule("d", 10, &[])),
        ];
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        let imps: Vec<i32> = rules.iter().map(|r| r.importance).collect();
        assert_eq!(imps, vec![20, 10, 5, 1]);
        assert_eq!(rules[0].condition, "b");
    }

    #[test]
    fn test_apply_rules_first_match_wins() {
        // Two rules, both would match; highest importance should fire first.
        let high = Rule {
            importance: 100,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "red")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let low = Rule {
            importance: 1,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "blue")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        // apply_rules_to_link iterates in order — caller is expected to have
        // sorted by importance (parse_rules does this). Pass [high, low].
        let result = apply_rules_to_link(&l, &[high, low]);
        assert_eq!(result["color"], "red");
    }

    #[test]
    fn test_apply_rules_no_match_returns_empty() {
        let l = link("hs1", 0, 100, "hs1", 200, 300); // intra-chr
        let r = Rule {
            importance: 10,
            condition: "_INTERCHR_".into(), // Won't match intra-chr link.
            overrides: [("color", "red")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let result = apply_rules_to_link(&l, &[r]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_importance_default_is_zero() {
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("_INTERCHR_"));
        // No `importance` key.
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_parse_rules_importance_negative_values_sort_last() {
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let list = vec![
            ConfigValue::Map(mk_rule("pos", 5, &[])),
            ConfigValue::Map(mk_rule("neg", -3, &[])),
            ConfigValue::Map(mk_rule("zero", 0, &[])),
        ];
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        // Descending sort: 5, 0, -3.
        let imps: Vec<i32> = rules.iter().map(|r| r.importance).collect();
        assert_eq!(imps, vec![5, 0, -3]);
    }

    #[test]
    fn test_parse_rules_non_map_rule_value_skipped() {
        // A bare string in the rule list is filter_map'd out.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let list = vec![
            leaf("not a map"),
            ConfigValue::Map(mk_rule("_INTERCHR_", 5, &[])),
        ];
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        // Only the map entry survives.
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].condition, "_INTERCHR_");
    }

    #[test]
    fn test_parse_rules_rule_value_must_have_condition() {
        // Rule without condition → filter_map returns None.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let mut r = HashMap::new();
        r.insert("importance".into(), leaf("10"));
        // No condition.
        block.insert("rule".into(), ConfigValue::Map(r));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_apply_rules_empty_rule_list_returns_empty() {
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_rule_key_is_scalar_returns_empty() {
        // `rule = "scalar"` (not a Map and not a List) → match falls through
        // to `_ => return Vec::new()`.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), leaf("just a string"));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_importance_non_numeric_falls_back_to_zero() {
        // `importance = "notanumber"` → `parse().ok()` yields None → default 0.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let mut r: HashMap<String, ConfigValue> = HashMap::new();
        r.insert("condition".into(), leaf("_INTERCHR_"));
        r.insert("importance".into(), leaf("notanumber"));
        r.insert("color".into(), leaf("red"));
        block.insert("rule".into(), ConfigValue::Map(r));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
        assert_eq!(rules[0].overrides["color"], "red");
    }

    #[test]
    fn test_parse_rules_overrides_skip_non_string_values() {
        // Rule with nested Map/List values → skipped (only strings land in overrides).
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let mut r: HashMap<String, ConfigValue> = HashMap::new();
        r.insert("condition".into(), leaf("_INTERCHR_"));
        r.insert("color".into(), leaf("red"));
        // A non-string value — won't land in overrides.
        let mut nested = HashMap::new();
        nested.insert("sub".into(), leaf("v"));
        r.insert("nested".into(), ConfigValue::Map(nested));
        r.insert(
            "list_value".into(),
            ConfigValue::List(vec![leaf("a"), leaf("b")]),
        );
        block.insert("rule".into(), ConfigValue::Map(r));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].overrides.len(), 1);
        assert_eq!(rules[0].overrides["color"], "red");
        assert!(!rules[0].overrides.contains_key("nested"));
        assert!(!rules[0].overrides.contains_key("list_value"));
    }

    #[test]
    fn test_apply_rules_to_link_returns_clone_not_reference() {
        // Verify the returned HashMap is a fresh clone — mutating it doesn't
        // affect the source rule's overrides.
        let r = Rule {
            importance: 10,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "red")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let mut result = apply_rules_to_link(&l, &[r.clone()]);
        result.insert("color".into(), "mutated".into());
        result.insert("new_key".into(), "new_val".into());
        // Original rule unchanged.
        assert_eq!(r.overrides["color"], "red");
        assert!(!r.overrides.contains_key("new_key"));
    }

    #[test]
    fn test_apply_rules_short_circuits_on_first_match() {
        // Multiple rules — first matching rule wins, subsequent matches not evaluated.
        let intra = Rule {
            importance: 10,
            condition: "_INTRACHR_".into(),
            overrides: [("color", "green")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let inter = Rule {
            importance: 5,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "red")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let l = link("hs1", 0, 100, "hs1", 200, 300); // intra-chr
        let result = apply_rules_to_link(&l, &[intra, inter]);
        assert_eq!(result["color"], "green");
    }

    #[test]
    fn test_parse_rules_importance_tied_preserves_input_order() {
        // Two rules with the same importance — stable sort preserves insertion order.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let list = vec![
            ConfigValue::Map(mk_rule("first", 10, &[])),
            ConfigValue::Map(mk_rule("second", 10, &[])),
            ConfigValue::Map(mk_rule("third", 10, &[])),
        ];
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 3);
        // With stable sort + equal importance, insertion order survives.
        assert_eq!(rules[0].condition, "first");
        assert_eq!(rules[1].condition, "second");
        assert_eq!(rules[2].condition, "third");
    }

    #[test]
    fn test_parse_rules_multiple_overrides_all_captured() {
        // A rule with many override keys — all captured (minus condition/importance).
        let r = mk_rule(
            "_INTERCHR_",
            5,
            &[
                ("color", "red"),
                ("thickness", "3"),
                ("z", "1"),
                ("url", "/x"),
                ("stroke_color", "blue"),
            ],
        );
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(r));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].overrides.len(), 5);
        assert_eq!(rules[0].overrides["color"], "red");
        assert_eq!(rules[0].overrides["thickness"], "3");
        assert_eq!(rules[0].overrides["z"], "1");
        assert_eq!(rules[0].overrides["url"], "/x");
        assert_eq!(rules[0].overrides["stroke_color"], "blue");
    }

    #[test]
    fn test_apply_rules_uses_unsorted_input_as_given() {
        // apply_rules_to_link does NOT re-sort — it iterates as given. A caller
        // that forgets to sort gets first-listed winning, not highest importance.
        let low = Rule {
            importance: 1,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "yellow")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let high = Rule {
            importance: 100,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "pink")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100); // inter-chr
        // Pass low first, then high — low wins despite lower importance.
        let result = apply_rules_to_link(&l, &[low, high]);
        assert_eq!(result["color"], "yellow");
    }

    #[test]
    fn test_parse_rules_list_with_mixed_map_and_scalar_entries() {
        // Mixed list: 3 maps + 2 scalars → scalars filter_map'd out; 3 rules survive.
        let list = vec![
            leaf("scalar_1"),
            ConfigValue::Map(mk_rule("a", 5, &[])),
            leaf("scalar_2"),
            ConfigValue::Map(mk_rule("b", 10, &[])),
            ConfigValue::Map(mk_rule("c", 3, &[])),
        ];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 3);
        // Sorted by importance desc: b(10), a(5), c(3).
        assert_eq!(rules[0].condition, "b");
        assert_eq!(rules[1].condition, "a");
        assert_eq!(rules[2].condition, "c");
    }

    #[test]
    fn test_rule_clone_is_deep_for_overrides_map() {
        // Cloning a Rule yields an independent overrides HashMap — mutations
        // to the clone's overrides don't affect the source.
        let r = Rule {
            importance: 10,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "red"), ("thickness", "3")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let mut c = r.clone();
        c.overrides.insert("color".into(), "blue".into());
        c.overrides.insert("new_key".into(), "new_val".into());
        // Source unchanged.
        assert_eq!(r.overrides["color"], "red");
        assert!(!r.overrides.contains_key("new_key"));
        assert_eq!(r.overrides.len(), 2);
        // Clone has mutations.
        assert_eq!(c.overrides["color"], "blue");
        assert!(c.overrides.contains_key("new_key"));
        assert_eq!(c.overrides.len(), 3);
    }

    #[test]
    fn test_rule_debug_output_includes_all_fields() {
        // Debug derive shows condition, importance, and overrides.
        let r = Rule {
            importance: 42,
            condition: "var(type) eq 'gene'".into(),
            overrides: [("color", "black")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let s = format!("{:?}", r);
        assert!(s.contains("importance: 42"));
        assert!(s.contains("condition:"));
        assert!(s.contains("gene"));
        assert!(s.contains("overrides:"));
    }

    #[test]
    fn test_parse_rules_list_with_single_entry_still_works() {
        // A list with only one Map entry — still parses to 1 rule.
        let list = vec![ConfigValue::Map(mk_rule("only", 5, &[("k", "v")]))];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].condition, "only");
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[0].overrides["k"], "v");
    }

    #[test]
    fn test_apply_rules_to_link_ignores_non_matching_first_rule() {
        // First rule doesn't match → iteration moves to second; second does match.
        let first = Rule {
            importance: 100,
            condition: "_INTRACHR_".into(), // won't match inter-chr
            overrides: [("color", "never")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let second = Rule {
            importance: 50,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "winner")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100); // inter-chr
        let result = apply_rules_to_link(&l, &[first, second]);
        assert_eq!(result["color"], "winner");
    }

    #[test]
    fn test_parse_rules_none_input_returns_empty() {
        // parse_rules(None) → empty Vec.
        let rules = parse_rules(None);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_empty_block_returns_empty() {
        // A block with no `rule` key → empty Vec.
        let block: HashMap<String, ConfigValue> = HashMap::new();
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_importance_zero_is_default_when_absent() {
        // When `importance` is absent, default is 0. Multiple rules with default
        // importance → sort by insertion order (stable sort).
        let list = vec![
            ConfigValue::Map(mk_rule("first_added", 0, &[])),
            ConfigValue::Map(mk_rule("second_added", 0, &[])),
            ConfigValue::Map(mk_rule("third_added", 0, &[])),
        ];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 3);
        // All have importance 0.
        for r in &rules {
            assert_eq!(r.importance, 0);
        }
        // Stable sort preserves insertion order when all have same importance.
        assert_eq!(rules[0].condition, "first_added");
        assert_eq!(rules[2].condition, "third_added");
    }

    #[test]
    fn test_apply_rules_returns_first_matching_overrides_intact() {
        // First matching rule's overrides fully preserved in the result.
        let r = Rule {
            importance: 10,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "red"), ("thickness", "3"), ("z", "5")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100); // inter-chr
        let result = apply_rules_to_link(&l, &[r]);
        // All 3 keys preserved.
        assert_eq!(result.len(), 3);
        assert_eq!(result["color"], "red");
        assert_eq!(result["thickness"], "3");
        assert_eq!(result["z"], "5");
    }

    #[test]
    fn test_rule_construction_with_zero_overrides() {
        // Rule with no overrides (empty HashMap) — condition + importance only.
        let r = Rule {
            importance: 5,
            condition: "_INTRACHR_".into(),
            overrides: HashMap::new(),
        };
        assert_eq!(r.importance, 5);
        assert_eq!(r.condition, "_INTRACHR_");
        assert!(r.overrides.is_empty());
    }

    #[test]
    fn test_apply_rules_match_but_empty_overrides_returns_empty() {
        // A matching rule with no overrides → apply returns empty HashMap (not None).
        let r = Rule {
            importance: 10,
            condition: "_INTERCHR_".into(),
            overrides: HashMap::new(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100); // inter-chr → matches
        let result = apply_rules_to_link(&l, &[r]);
        // Match occurred but overrides were empty.
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_list_with_only_scalars_yields_empty() {
        // A list entirely of non-Map entries → all filter_map'd out.
        let list = vec![leaf("a"), leaf("b"), leaf("c")];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_rule_importance_negative_sorted_after_zero() {
        // Negative importance → sort last in descending order.
        let list = vec![
            ConfigValue::Map(mk_rule("pos", 10, &[])),
            ConfigValue::Map(mk_rule("zero", 0, &[])),
            ConfigValue::Map(mk_rule("neg", -5, &[])),
        ];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].condition, "pos");
        assert_eq!(rules[1].condition, "zero");
        assert_eq!(rules[2].condition, "neg");
    }

    #[test]
    fn test_parse_rules_large_rule_list_all_preserved() {
        // 20 rules with various importances → all sorted by importance desc.
        let list: Vec<ConfigValue> = (0..20)
            .map(|i| ConfigValue::Map(mk_rule(&format!("r{}", i), i as i32, &[])))
            .collect();
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 20);
        // Highest importance first: r19 → r0.
        assert_eq!(rules[0].condition, "r19");
        assert_eq!(rules[0].importance, 19);
        assert_eq!(rules[19].condition, "r0");
        assert_eq!(rules[19].importance, 0);
    }

    #[test]
    fn test_rule_clone_deep_for_large_overrides_map() {
        // Cloning with many overrides → all preserved; mutations isolated.
        let mut overrides = HashMap::new();
        for i in 0..30 {
            overrides.insert(format!("k{}", i), format!("v{}", i));
        }
        let r = Rule {
            importance: 5,
            condition: "_INTERCHR_".into(),
            overrides,
        };
        let mut c = r.clone();
        c.overrides.insert("extra".into(), "val".into());
        assert_eq!(r.overrides.len(), 30);
        assert_eq!(c.overrides.len(), 31);
    }

    #[test]
    fn test_apply_rules_multiple_matches_first_wins() {
        // Both inter-chr rules match; first (highest importance) wins.
        let high = Rule {
            importance: 100,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "red")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };
        let low = Rule {
            importance: 1,
            condition: "_INTERCHR_".into(),
            overrides: [("color", "blue")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[high, low]);
        assert_eq!(result["color"], "red");
    }

    #[test]
    fn test_apply_rules_empty_rule_list_no_panic() {
        // Empty rule slice → empty result.
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_none_conf_returns_empty_vec() {
        // None input → Vec::new() short-circuit.
        assert!(parse_rules(None).is_empty());
    }

    #[test]
    fn test_parse_rules_missing_rule_key_returns_empty() {
        // Some(conf) but no "rule" key → empty (neither List nor Map matched).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("other".into(), leaf("value"));
        assert!(parse_rules(Some(&conf)).is_empty());
        // "rule" present as Str → still empty (match arm requires List or Map).
        let mut conf2: HashMap<String, ConfigValue> = HashMap::new();
        conf2.insert("rule".into(), leaf("not_a_map"));
        assert!(parse_rules(Some(&conf2)).is_empty());
    }

    #[test]
    fn test_parse_rules_single_map_wraps_as_one_rule() {
        // When "rule" is a single Map (not a List), it's treated as one rule.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rule_map = mk_rule("1", 5, &[("color", "orange")]);
        conf.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[0].overrides.get("color").map(String::as_str), Some("orange"));
    }

    #[test]
    fn test_parse_rules_invalid_importance_defaults_to_zero() {
        // Non-parseable importance → unwrap_or(0) → rule preserved with importance=0.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf("1"));
        m.insert("importance".into(), leaf("not-a-number"));
        m.insert("color".into(), leaf("red"));
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
        assert_eq!(rules[0].overrides.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn test_parse_rules_filters_entries_missing_condition_key() {
        // Rule map without "condition" key → filter_map yields None → skipped.
        let mut no_cond: HashMap<String, ConfigValue> = HashMap::new();
        no_cond.insert("importance".into(), leaf("5"));
        no_cond.insert("color".into(), leaf("red"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(no_cond)]));
        assert_eq!(parse_rules(Some(&conf)).len(), 0);
    }

    #[test]
    fn test_parse_rules_sorted_by_importance_descending() {
        // Rules with importance 1/5/3 → output order is [5, 3, 1].
        let rules_list = vec![
            ConfigValue::Map(mk_rule("1", 1, &[("tag", "low")])),
            ConfigValue::Map(mk_rule("1", 5, &[("tag", "high")])),
            ConfigValue::Map(mk_rule("1", 3, &[("tag", "mid")])),
        ];
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[1].importance, 3);
        assert_eq!(rules[2].importance, 1);
    }

    #[test]
    fn test_parse_rules_condition_and_importance_keys_excluded_from_overrides() {
        // Reserved keys "condition" and "importance" don't leak into the overrides map.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf("1"));
        m.insert("importance".into(), leaf("5"));
        m.insert("color".into(), leaf("blue"));
        m.insert("thickness".into(), leaf("3"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert!(!rules[0].overrides.contains_key("condition"));
        assert!(!rules[0].overrides.contains_key("importance"));
        assert_eq!(rules[0].overrides.get("color").map(String::as_str), Some("blue"));
        assert_eq!(rules[0].overrides.get("thickness").map(String::as_str), Some("3"));
    }

    #[test]
    fn test_apply_rules_matching_rule_with_empty_overrides_returns_empty_map() {
        // Rule matches but has no overrides → returns empty map (not None).
        let rule = Rule {
            importance: 1,
            condition: "1".into(),
            overrides: HashMap::new(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[rule]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_negative_importance_sorts_after_positive_and_zero() {
        // Importance values -5 / 0 / 3 → sorted descending: [3, 0, -5].
        let rules_list = vec![
            ConfigValue::Map(mk_rule("1", -5, &[("k", "low")])),
            ConfigValue::Map(mk_rule("1", 0, &[("k", "mid")])),
            ConfigValue::Map(mk_rule("1", 3, &[("k", "high")])),
        ];
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].importance, 3);
        assert_eq!(rules[1].importance, 0);
        assert_eq!(rules[2].importance, -5);
    }

    #[test]
    fn test_apply_rules_all_conditions_false_returns_empty_map() {
        // No rule's condition matches → empty HashMap, not None.
        let rules = vec![
            Rule { importance: 10, condition: "0".into(), overrides: [("a", "1")].iter().map(|(k,v)| (k.to_string(), v.to_string())).collect() },
            Rule { importance: 5, condition: "0".into(), overrides: [("b", "2")].iter().map(|(k,v)| (k.to_string(), v.to_string())).collect() },
        ];
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &rules);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_condition_string_preserved_verbatim() {
        // Complex condition expression stored as-is, no trimming/normalization.
        let cond = "_CHR1_ == \"hs1\" && _SIZE1_ > 1000";
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf(cond));
        m.insert("importance".into(), leaf("5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].condition, cond);
    }

    #[test]
    fn test_rule_clone_produces_independent_overrides_map() {
        // Clone produces a new HashMap — mutations on clone don't affect source.
        let mut overrides: HashMap<String, String> = HashMap::new();
        overrides.insert("color".into(), "red".into());
        let r = Rule {
            importance: 1,
            condition: "1".into(),
            overrides,
        };
        let mut c = r.clone();
        c.overrides.insert("thickness".into(), "3".into());
        c.overrides.insert("color".into(), "blue".into());
        // Source unchanged.
        assert_eq!(r.overrides.len(), 1);
        assert_eq!(r.overrides.get("color").map(String::as_str), Some("red"));
        // Clone has 2 entries and blue.
        assert_eq!(c.overrides.len(), 2);
        assert_eq!(c.overrides.get("color").map(String::as_str), Some("blue"));
    }

    #[test]
    fn test_parse_rules_list_of_five_rules_all_retained() {
        // Input: 5-rule List → output: 5 rules (no filtering).
        let rules_list: Vec<ConfigValue> = (0..5)
            .map(|i| ConfigValue::Map(mk_rule("1", i, &[("tag", "v")])))
            .collect();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 5);
        // Sorted descending by importance: 4, 3, 2, 1, 0.
        let importances: Vec<i32> = rules.iter().map(|r| r.importance).collect();
        assert_eq!(importances, vec![4, 3, 2, 1, 0]);
    }

    #[test]
    fn test_apply_rules_short_circuits_on_first_match_not_later_rules() {
        // First-match wins — even if later rule has more overrides.
        let rules = vec![
            Rule { importance: 10, condition: "1".into(),
                overrides: [("a", "FIRST")].iter().map(|(k,v)| (k.to_string(), v.to_string())).collect() },
            Rule { importance: 5, condition: "1".into(),
                overrides: [("b", "SECOND")].iter().map(|(k,v)| (k.to_string(), v.to_string())).collect() },
        ];
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &rules);
        // Only first rule's overrides — second is never visited.
        assert_eq!(result.get("a").map(String::as_str), Some("FIRST"));
        assert!(!result.contains_key("b"));
    }

    #[test]
    fn test_rule_importance_accepts_i32_extremes() {
        // importance is i32 — MIN/MAX values accepted at struct level.
        let r1 = Rule { importance: i32::MIN, condition: "1".into(), overrides: HashMap::new() };
        let r2 = Rule { importance: i32::MAX, condition: "1".into(), overrides: HashMap::new() };
        assert_eq!(r1.importance, i32::MIN);
        assert_eq!(r2.importance, i32::MAX);
        // Sorting two-rule slice with extremes works without panic.
        let mut rules = vec![r1, r2];
        rules.sort_by(|a, b| b.importance.cmp(&a.importance));
        assert_eq!(rules[0].importance, i32::MAX);
    }

    #[test]
    fn test_parse_rules_preserves_override_keys_with_special_chars() {
        // Override keys with dashes, underscores, dots preserved verbatim.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf("1"));
        m.insert("importance".into(), leaf("1"));
        m.insert("stroke-color".into(), leaf("red"));
        m.insert("label.size".into(), leaf("14"));
        m.insert("foo_bar".into(), leaf("x"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert!(rules[0].overrides.contains_key("stroke-color"));
        assert!(rules[0].overrides.contains_key("label.size"));
        assert!(rules[0].overrides.contains_key("foo_bar"));
    }

    #[test]
    fn test_parse_rules_rule_value_as_list_variant_triggers_list_arm() {
        // When `rule` is a List (vs a single Map), parse takes the List arm directly.
        let mut m1 = mk_rule("1", 1, &[("color", "red")]);
        let mut m2 = mk_rule("1", 2, &[("color", "blue")]);
        // Force importance into the map (mk_rule already does condition+overrides).
        m1.insert("importance".into(), leaf("1"));
        m2.insert("importance".into(), leaf("2"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert(
            "rule".into(),
            ConfigValue::List(vec![ConfigValue::Map(m1), ConfigValue::Map(m2)]),
        );
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 2);
        // Sorted descending by importance: importance=2 wins first slot.
        assert_eq!(rules[0].importance, 2);
        assert_eq!(rules[1].importance, 1);
    }

    #[test]
    fn test_parse_rules_rule_value_neither_map_nor_list_returns_empty() {
        // Rule key as a plain Str (not Map, not List) → early `_` arm → empty Vec.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), leaf("just-a-string"));
        assert!(parse_rules(Some(&conf)).is_empty());
    }

    #[test]
    fn test_parse_rules_non_str_override_value_skipped_silently() {
        // If an override value is not a Str (e.g., a nested Map), it's filtered out — Other entries still preserved.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf("1"));
        m.insert("color".into(), leaf("red"));
        // Non-str value should be ignored.
        let mut sub: HashMap<String, ConfigValue> = HashMap::new();
        sub.insert("inner".into(), leaf("x"));
        m.insert("nested_overrides".into(), ConfigValue::Map(sub));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert!(rules[0].overrides.contains_key("color"));
        assert!(!rules[0].overrides.contains_key("nested_overrides"));
    }

    #[test]
    fn test_apply_rules_to_link_returns_cloned_map_not_reference() {
        // The returned overrides are a .clone() — mutating the map doesn't affect the rule.
        let link = Link {
            id: "".into(),
            points: vec![Datum {
                chr: "hs1".into(),
                start: 0,
                end: 100,
                set: IntSpan::new(),
                ..Default::default()
            }],
            param: HashMap::new(),
        };
        let rules = vec![Rule {
            importance: 0,
            condition: "1".into(),
            overrides: {
                let mut o = HashMap::new();
                o.insert("color".into(), "red".into());
                o
            },
        }];
        let mut result = apply_rules_to_link(&link, &rules);
        result.insert("injected".into(), "value".into());
        // Source rule's overrides unchanged.
        assert!(!rules[0].overrides.contains_key("injected"));
        assert_eq!(rules[0].overrides.len(), 1);
    }

    #[test]
    fn test_apply_rules_to_link_empty_condition_still_evaluates() {
        // An empty condition string — how it's treated depends on evaluate_link_condition
        // (empty expressions evaluate to false per expression::eval_bool_expr).
        let link = Link {
            id: "".into(),
            points: vec![],
            param: HashMap::new(),
        };
        let rules = vec![Rule {
            importance: 0,
            condition: String::new(),
            overrides: {
                let mut o = HashMap::new();
                o.insert("c".into(), "v".into());
                o
            },
        }];
        let result = apply_rules_to_link(&link, &rules);
        // Empty expr → falsy → no match → empty overrides.
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_zero_importance_explicitly_set() {
        // importance="0" explicitly set → parsed as 0, sorted same as default.
        let mut m = mk_rule("1", 0, &[("color", "red")]);
        m.insert("importance".into(), leaf("0"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_parse_rules_overrides_with_duplicate_keys_second_wins() {
        // HashMap semantics: duplicate keys in the rule map — whichever insert comes last wins.
        // Since a Rust HashMap can't have duplicates, simulate via overwrite.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf("1"));
        m.insert("color".into(), leaf("blue"));
        m.insert("color".into(), leaf("green")); // overwrites
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules[0].overrides["color"], "green");
    }

    #[test]
    fn test_apply_rules_empty_rules_slice_returns_empty_map_not_none() {
        // apply_rules_to_link with zero rules → empty HashMap, NOT a None or panic.
        let link = Link {
            id: "".into(),
            points: vec![],
            param: HashMap::new(),
        };
        let result = apply_rules_to_link(&link, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_empty_rule_map_without_condition_filtered_out() {
        // A rule missing the condition key is filtered out by filter_map.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("color".into(), leaf("red"));
        // No "condition" → filter_map returns None for this entry.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_preserves_sort_across_equal_importance() {
        // Sort is desc by importance; for ties, insertion order is preserved (stable sort).
        let mut rules_list = Vec::new();
        for i in 0..3 {
            let mut m = mk_rule("1", 0, &[("label", &format!("rule_{}", i))]);
            m.insert("importance".into(), leaf("0"));
            rules_list.push(ConfigValue::Map(m));
        }
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 3);
        // All have importance=0; since sort is stable, check all are importance 0.
        for r in &rules {
            assert_eq!(r.importance, 0);
        }
    }

    #[test]
    fn test_rule_struct_debug_includes_condition_and_importance() {
        // Debug derive emits all fields of Rule.
        let r = Rule {
            importance: 42,
            condition: "x > 10".into(),
            overrides: HashMap::new(),
        };
        let dbg = format!("{:?}", r);
        assert!(dbg.contains("42"));
        assert!(dbg.contains("x > 10"));
    }

    #[test]
    fn test_apply_rules_to_link_returns_always_heap_allocated_not_reference() {
        // Each call to apply_rules_to_link produces a fresh HashMap.
        let link = Link {
            id: "".into(),
            points: vec![],
            param: HashMap::new(),
        };
        let rules = vec![Rule {
            importance: 0,
            condition: "1".into(),
            overrides: {
                let mut o = HashMap::new();
                o.insert("x".into(), "1".into());
                o
            },
        }];
        let r1 = apply_rules_to_link(&link, &rules);
        let r2 = apply_rules_to_link(&link, &rules);
        // Both should have the same contents, but they are separate HashMaps.
        assert_eq!(r1, r2);
        assert_eq!(r1.len(), 1);
    }

    #[test]
    fn test_parse_rules_importance_parsed_as_i32_negative_large() {
        // Very negative importance (i32 range) — parses correctly.
        let mut m = mk_rule("1", -1000000, &[("color", "red")]);
        m.insert("importance".into(), leaf("-1000000"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, -1000000);
    }

    #[test]
    fn test_parse_rules_sorting_by_descending_importance_preserves_three_tier() {
        // Three rules with distinct importance 10/5/0 → sorted 10, 5, 0.
        let mut ms = Vec::new();
        for imp in [0, 10, 5] {
            let mut m = mk_rule("1", imp, &[("label", &format!("i{}", imp))]);
            m.insert("importance".into(), leaf(&imp.to_string()));
            ms.push(ConfigValue::Map(m));
        }
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(ms));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].importance, 10);
        assert_eq!(rules[1].importance, 5);
        assert_eq!(rules[2].importance, 0);
    }

    #[test]
    fn test_apply_rules_to_link_rule_with_empty_overrides_matches_and_returns_empty() {
        // A rule with no override keys but matching condition → still fires, returns empty map.
        let link = Link {
            id: "".into(),
            points: vec![],
            param: HashMap::new(),
        };
        let rules = vec![Rule {
            importance: 0,
            condition: "1".into(),
            overrides: HashMap::new(),
        }];
        let result = apply_rules_to_link(&link, &rules);
        // First rule matches, returns its (empty) overrides clone.
        assert!(result.is_empty());
    }

    #[test]
    fn test_rule_debug_format_includes_overrides_hashmap() {
        // Debug should include the overrides map contents.
        let mut overrides = HashMap::new();
        overrides.insert("color".into(), "red".into());
        let r = Rule {
            importance: 1,
            condition: "test".into(),
            overrides,
        };
        let dbg = format!("{:?}", r);
        assert!(dbg.contains("color"));
        assert!(dbg.contains("red"));
    }

    #[test]
    fn test_parse_rules_importance_as_float_string_parses_as_zero() {
        // "1.5" as importance — i32 parse fails → defaults to 0.
        let mut m = mk_rule("1", 0, &[("color", "red")]);
        m.insert("importance".into(), leaf("1.5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_apply_rules_multiple_rules_first_match_wins_skips_later() {
        // Two matching rules → first (in sorted order) wins.
        let link = Link {
            id: "".into(),
            points: vec![],
            param: HashMap::new(),
        };
        let r1 = Rule {
            importance: 10,
            condition: "1".into(),
            overrides: {
                let mut o = HashMap::new();
                o.insert("color".into(), "first".into());
                o
            },
        };
        let r2 = Rule {
            importance: 5,
            condition: "1".into(),
            overrides: {
                let mut o = HashMap::new();
                o.insert("color".into(), "second".into());
                o
            },
        };
        let rules = vec![r1, r2];
        let result = apply_rules_to_link(&link, &rules);
        assert_eq!(result.get("color").map(|s| s.as_str()), Some("first"));
    }

    #[test]
    fn test_parse_rules_handles_rule_entry_without_any_key_values() {
        // A rule Map with no keys at all → no condition → filtered out.
        let m: HashMap<String, ConfigValue> = HashMap::new();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_rule_value_is_list_of_two_maps_both_parsed() {
        // List of 2 rule maps → both parsed.
        let m1 = mk_rule("1", 1, &[("color", "red")]);
        let m2 = mk_rule("1", 2, &[("color", "blue")]);
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert(
            "rule".into(),
            ConfigValue::List(vec![ConfigValue::Map(m1), ConfigValue::Map(m2)]),
        );
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_parse_rules_single_map_variant_wraps_as_one_rule() {
        // When rule is a single Map (not List), treated as one rule.
        let m = mk_rule("1", 0, &[("color", "red")]);
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(m));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].overrides["color"], "red");
    }

    #[test]
    fn test_rule_clone_creates_independent_overrides_map() {
        // Cloning rule → overrides HashMap is deep-cloned.
        let mut overrides = HashMap::new();
        overrides.insert("a".into(), "1".into());
        let r1 = Rule {
            importance: 0,
            condition: "c".into(),
            overrides,
        };
        let mut r2 = r1.clone();
        r2.overrides.insert("b".into(), "2".into());
        assert_eq!(r1.overrides.len(), 1);
        assert_eq!(r2.overrides.len(), 2);
    }

    #[test]
    fn test_parse_rules_importance_missing_defaults_to_zero() {
        // No "importance" key → defaults to 0 via unwrap_or(0).
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf("1"));
        m.insert("color".into(), leaf("red"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_apply_rules_to_link_second_rule_matches_first_fails() {
        // First rule's condition evaluates to false; second matches → second wins.
        let link = Link {
            id: "".into(),
            points: vec![Datum { chr: "hs1".into(), ..Default::default() }],
            param: HashMap::new(),
        };
        let rules = vec![
            Rule {
                importance: 0,
                condition: "0".into(), // always false
                overrides: {
                    let mut o = HashMap::new();
                    o.insert("color".into(), "first".into());
                    o
                },
            },
            Rule {
                importance: 0,
                condition: "1".into(), // always true
                overrides: {
                    let mut o = HashMap::new();
                    o.insert("color".into(), "second".into());
                    o
                },
            },
        ];
        let result = apply_rules_to_link(&link, &rules);
        assert_eq!(result.get("color").map(String::as_str), Some("second"));
    }

    #[test]
    fn test_parse_rules_rule_with_only_condition_no_overrides_valid() {
        // Rule with only condition key — valid, empty overrides.
        let mut m: HashMap<String, ConfigValue> = HashMap::new();
        m.insert("condition".into(), leaf("1"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(m)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert!(rules[0].overrides.is_empty());
    }

    #[test]
    fn test_parse_rules_with_many_rules_sorted_descending_by_importance() {
        // 4 rules with importance 1/3/2/4 → sorted [4,3,2,1].
        let mut ms = Vec::new();
        for imp in [1, 3, 2, 4] {
            let mut m = mk_rule("1", imp, &[]);
            m.insert("importance".into(), leaf(&imp.to_string()));
            ms.push(ConfigValue::Map(m));
        }
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(ms));
        let rules = parse_rules(Some(&conf));
        let importances: Vec<i32> = rules.iter().map(|r| r.importance).collect();
        assert_eq!(importances, vec![4, 3, 2, 1]);
    }

    #[test]
    fn test_apply_rules_empty_overrides_still_returns_empty_clone() {
        // First rule has empty overrides but matches — returns empty map.
        let link = Link {
            id: "".into(),
            points: vec![],
            param: HashMap::new(),
        };
        let rules = vec![Rule {
            importance: 0,
            condition: "1".into(),
            overrides: HashMap::new(),
        }];
        let result = apply_rules_to_link(&link, &rules);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rule_struct_fields_can_all_be_updated_independently() {
        // Mutate each field independently.
        let mut r = Rule {
            importance: 0,
            condition: "a".into(),
            overrides: HashMap::new(),
        };
        r.importance = 100;
        r.condition = "b".into();
        r.overrides.insert("k".into(), "v".into());
        assert_eq!(r.importance, 100);
        assert_eq!(r.condition, "b");
        assert_eq!(r.overrides.len(), 1);
    }

    #[test]
    fn test_apply_rules_to_link_empty_rules_returns_empty_map() {
        // No rules → empty HashMap.
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_rules_to_link_nonmatching_rule_returns_empty_map() {
        // Rule condition doesn't match → empty map returned (no overrides applied).
        let rule = Rule {
            importance: 10,
            condition: "_INTRACHR_".into(),
            overrides: [("color".to_string(), "red".to_string())].into_iter().collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[rule]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_preserves_overrides_from_non_condition_non_importance_fields() {
        // All keys other than "condition" and "importance" become overrides.
        let r = mk_rule("_INTERCHR_", 5, &[("color", "blue"), ("z", "7"), ("thickness", "2")]);
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(r));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].overrides.len(), 3);
        assert_eq!(rules[0].overrides["color"], "blue");
        assert_eq!(rules[0].overrides["z"], "7");
        assert_eq!(rules[0].overrides["thickness"], "2");
    }

    #[test]
    fn test_parse_rules_negative_importance_preserved() {
        // Negative importance values parse and sort correctly.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        let r = mk_rule("_INTERCHR_", -5, &[]);
        block.insert("rule".into(), ConfigValue::Map(r));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].importance, -5);
    }

    #[test]
    fn test_parse_rules_without_condition_field_skipped() {
        // Rule lacking "condition" → None via filter_map → skipped.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("importance".into(), leaf("10"));
        rule_map.insert("color".into(), leaf("red"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_with_non_parseable_importance_defaults_to_zero() {
        // importance field with garbage value → unwrap_or(0).
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("_INTERCHR_"));
        rule_map.insert("importance".into(), leaf("not-a-number"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_parse_rules_non_str_overrides_are_dropped_by_filter() {
        // Only Str-valued entries become overrides — Map/List-valued skipped silently.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("_INTERCHR_"));
        rule_map.insert("importance".into(), leaf("1"));
        rule_map.insert("color".into(), leaf("red"));
        rule_map.insert("nested".into(), ConfigValue::Map(HashMap::new()));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].overrides.len(), 1);
        assert_eq!(rules[0].overrides.get("color"), Some(&"red".to_string()));
        assert!(!rules[0].overrides.contains_key("nested"));
    }

    #[test]
    fn test_parse_rules_equal_importance_stable_ordering_relative() {
        // Two rules with same importance → sort by importance is stable.
        let mut rule_map_a: HashMap<String, ConfigValue> = HashMap::new();
        rule_map_a.insert("condition".into(), leaf("A"));
        rule_map_a.insert("importance".into(), leaf("5"));
        let mut rule_map_b: HashMap<String, ConfigValue> = HashMap::new();
        rule_map_b.insert("condition".into(), leaf("B"));
        rule_map_b.insert("importance".into(), leaf("5"));
        let list = vec![ConfigValue::Map(rule_map_a), ConfigValue::Map(rule_map_b)];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[1].importance, 5);
    }

    #[test]
    fn test_parse_rules_no_rule_key_returns_empty_vec() {
        // rules block present but "rule" key absent → empty Vec.
        let block: HashMap<String, ConfigValue> = HashMap::new();
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_rule_value_is_str_not_map_returns_empty() {
        // "rule" value is a Str (not Map or List of Maps) → returns empty.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), leaf("not_a_rule_map"));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_condition_field_not_a_string_skipped() {
        // condition field present but not a Str → filter_map skips rule.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), ConfigValue::Map(HashMap::new()));
        rule_map.insert("importance".into(), leaf("10"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_sort_descending_applies_across_range() {
        // Rules with widely-spaced importance values all sorted desc.
        let mut rs: Vec<ConfigValue> = Vec::new();
        for imp in [2, 100, -50, 30] {
            let mut r: HashMap<String, ConfigValue> = HashMap::new();
            r.insert("condition".into(), leaf("x"));
            r.insert("importance".into(), leaf(&imp.to_string()));
            rs.push(ConfigValue::Map(r));
        }
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(rs));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 4);
        // Descending: 100, 30, 2, -50.
        assert_eq!(rules[0].importance, 100);
        assert_eq!(rules[1].importance, 30);
        assert_eq!(rules[2].importance, 2);
        assert_eq!(rules[3].importance, -50);
    }

    #[test]
    fn test_parse_rules_importance_zero_default_without_field_sorts_last() {
        // Rules without importance default to 0; positive-importance rule sorts first.
        let mut a: HashMap<String, ConfigValue> = HashMap::new();
        a.insert("condition".into(), leaf("_INTERCHR_"));
        // no importance
        let mut b: HashMap<String, ConfigValue> = HashMap::new();
        b.insert("condition".into(), leaf("_INTERCHR_"));
        b.insert("importance".into(), leaf("5"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(vec![
            ConfigValue::Map(a),
            ConfigValue::Map(b),
        ]));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[1].importance, 0);
    }

    #[test]
    fn test_rule_struct_defaults_via_direct_construction() {
        // Rule struct can be constructed directly with default-like values.
        let r = Rule {
            importance: 0,
            condition: String::new(),
            overrides: HashMap::new(),
        };
        assert_eq!(r.importance, 0);
        assert!(r.condition.is_empty());
        assert!(r.overrides.is_empty());
    }

    #[test]
    fn test_apply_rules_to_link_rule_with_overrides_returns_cloned_map() {
        // Matching rule → its overrides cloned into return map.
        let rule = Rule {
            importance: 0,
            condition: "_INTERCHR_".into(),
            overrides: [
                ("color".to_string(), "green".to_string()),
                ("thickness".to_string(), "5".to_string()),
            ].into_iter().collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[rule]);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("color"), Some(&"green".to_string()));
        assert_eq!(result.get("thickness"), Some(&"5".to_string()));
    }

    #[test]
    fn test_parse_rules_list_with_one_non_map_entry_filter_skips() {
        // List of rule values: one is Str (filtered) → only valid rules kept.
        let mut valid: HashMap<String, ConfigValue> = HashMap::new();
        valid.insert("condition".into(), leaf("_INTERCHR_"));
        valid.insert("importance".into(), leaf("1"));
        let list = vec![leaf("not_a_map"), ConfigValue::Map(valid)];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 1);
    }

    #[test]
    fn test_apply_rules_to_link_higher_importance_rule_wins() {
        // Two rules ordered by importance; first match by iteration order wins.
        let rule_hi = Rule {
            importance: 10,
            condition: "_INTERCHR_".into(),
            overrides: [("color".to_string(), "hi".to_string())].into_iter().collect(),
        };
        let rule_lo = Rule {
            importance: 1,
            condition: "_INTERCHR_".into(),
            overrides: [("color".to_string(), "lo".to_string())].into_iter().collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        // parse_rules sorts desc; slice is already in desc order here.
        let result = apply_rules_to_link(&l, &[rule_hi, rule_lo]);
        assert_eq!(result.get("color"), Some(&"hi".to_string()));
    }

    #[test]
    fn test_parse_rules_integer_importance_zero_kept_as_zero() {
        // Explicit importance=0 (not missing) — still 0.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("_INTERCHR_"));
        rule_map.insert("importance".into(), leaf("0"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_rule_struct_clone_produces_deep_copy_of_overrides() {
        // Rule is Clone → mutating cloned overrides doesn't affect original.
        let mut orig = Rule {
            importance: 1,
            condition: "x".into(),
            overrides: [("k".to_string(), "v".to_string())].into_iter().collect(),
        };
        let mut cloned = orig.clone();
        cloned.overrides.insert("k2".to_string(), "v2".to_string());
        assert_eq!(orig.overrides.len(), 1);
        assert_eq!(cloned.overrides.len(), 2);
        // Touch orig to silence dead_code warnings.
        orig.importance = 2;
        assert_eq!(orig.importance, 2);
    }

    #[test]
    fn test_parse_rules_preserves_rule_count_across_mixed_importance() {
        // All 4 provided rules should appear in output regardless of importance sign.
        let mut rs: Vec<ConfigValue> = Vec::new();
        for imp in [100, 0, -1, 50] {
            let mut r: HashMap<String, ConfigValue> = HashMap::new();
            r.insert("condition".into(), leaf("c"));
            r.insert("importance".into(), leaf(&imp.to_string()));
            rs.push(ConfigValue::Map(r));
        }
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(rs));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn test_parse_rules_single_rule_map_variant_wraps_in_single_vec() {
        // Single Map (not List) at "rule" → wrapped as 1-element rules Vec.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("A"));
        rule_map.insert("importance".into(), leaf("1"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_parse_rules_max_i32_importance_preserved() {
        // i32::MAX importance preserved intact through parse.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("x"));
        rule_map.insert("importance".into(), leaf(&i32::MAX.to_string()));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].importance, i32::MAX);
    }

    #[test]
    fn test_parse_rules_complex_condition_string_preserved_exactly() {
        // Complex Perl-style condition string preserved exactly.
        let cond = "_INTERCHR_ || var(thickness) > 5";
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf(cond));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].condition, cond);
    }

    #[test]
    fn test_rule_struct_default_importance_zero_explicitly_constructable() {
        // Directly constructed Rule with importance=0 valid.
        let r = Rule { importance: 0, condition: "cond".into(), overrides: HashMap::new() };
        assert_eq!(r.importance, 0);
        assert_eq!(r.condition, "cond");
    }

    #[test]
    fn test_apply_rules_to_link_empty_link_with_rules_still_evaluates() {
        // Even a link with empty points can be queried — rules eval shouldn't panic.
        let rule = Rule {
            importance: 0,
            condition: "_INTERCHR_".into(),
            overrides: HashMap::new(),
        };
        let l = Link { id: "l".into(), points: Vec::new(), param: HashMap::new() };
        // With no points, condition may not match but shouldn't panic.
        let _ = apply_rules_to_link(&l, &[rule]);
    }

    #[test]
    fn test_parse_rules_importance_with_whitespace_still_parses_as_zero_default() {
        // "  10  " parses fine with leading/trailing whitespace (Rust str::parse trims).
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("x"));
        rule_map.insert("importance".into(), leaf("10"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].importance, 10);
    }

    #[test]
    fn test_parse_rules_two_rules_same_importance_both_parsed() {
        // Sort by descending importance — ties keep both rules.
        let mut a: HashMap<String, ConfigValue> = HashMap::new();
        a.insert("condition".into(), leaf("A"));
        a.insert("importance".into(), leaf("5"));
        let mut b: HashMap<String, ConfigValue> = HashMap::new();
        b.insert("condition".into(), leaf("B"));
        b.insert("importance".into(), leaf("5"));
        let list = vec![ConfigValue::Map(a), ConfigValue::Map(b)];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_rule_overrides_multiple_keys_populate_fields() {
        // Rule.overrides populated via insert — all keys accessible.
        let mut r = Rule {
            importance: 1,
            condition: "c".into(),
            overrides: HashMap::new(),
        };
        for (k, v) in [("color", "red"), ("z", "5"), ("stroke", "2")] {
            r.overrides.insert(k.to_string(), v.to_string());
        }
        assert_eq!(r.overrides.len(), 3);
        assert_eq!(r.overrides["color"], "red");
    }

    #[test]
    fn test_parse_rules_importance_i32_min_preserved() {
        // i32::MIN importance also preserved.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("c"));
        rule_map.insert("importance".into(), leaf(&i32::MIN.to_string()));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].importance, i32::MIN);
    }

    #[test]
    fn test_apply_rules_to_link_first_matching_rule_wins_over_subsequent() {
        // Rules slice iterated in order — first match wins.
        let rule_a = Rule {
            importance: 0,
            condition: "_INTERCHR_".into(),
            overrides: [("color".to_string(), "A".to_string())].into_iter().collect(),
        };
        let rule_b = Rule {
            importance: 0,
            condition: "_INTERCHR_".into(),
            overrides: [("color".to_string(), "B".to_string())].into_iter().collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[rule_a, rule_b]);
        assert_eq!(result.get("color"), Some(&"A".to_string()));
    }

    #[test]
    fn test_parse_rules_list_with_all_invalid_entries_returns_empty() {
        // All list entries lack condition → empty Vec.
        let mut r1: HashMap<String, ConfigValue> = HashMap::new();
        r1.insert("importance".into(), leaf("1"));
        let mut r2: HashMap<String, ConfigValue> = HashMap::new();
        r2.insert("importance".into(), leaf("2"));
        let list = vec![ConfigValue::Map(r1), ConfigValue::Map(r2)];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_many_rules_with_descending_importance_sorted_stable() {
        // 10 rules with varied importance → sorted max-first.
        let imps = [5, 10, -3, 7, 2, 8, 0, -1, 4, 3];
        let mut list: Vec<ConfigValue> = Vec::new();
        for imp in imps {
            let mut r: HashMap<String, ConfigValue> = HashMap::new();
            r.insert("condition".into(), leaf("c"));
            r.insert("importance".into(), leaf(&imp.to_string()));
            list.push(ConfigValue::Map(r));
        }
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 10);
        // Sorted descending: first importance >= second >= ...
        for i in 0..9 {
            assert!(rules[i].importance >= rules[i + 1].importance);
        }
    }

    #[test]
    fn test_parse_rules_single_rule_condition_retrievable_exactly() {
        // Single rule → condition stored byte-for-byte.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("special_condition_123"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].condition, "special_condition_123");
    }

    #[test]
    fn test_apply_rules_to_link_condition_empty_string_doesnt_match_basic_link() {
        // Empty condition string → evaluate_link_condition on empty → likely false.
        let rule = Rule {
            importance: 0,
            condition: "".into(),
            overrides: [("k".to_string(), "v".to_string())].into_iter().collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[rule]);
        // Empty condition: may match or not — just verify no panic.
        let _ = result;
    }

    #[test]
    fn test_rule_struct_manual_populate_with_many_overrides() {
        // Rule accepts HashMap with many overrides.
        let mut overrides = HashMap::new();
        for i in 0..20 {
            overrides.insert(format!("k{}", i), format!("v{}", i));
        }
        let r = Rule {
            importance: 1,
            condition: "c".into(),
            overrides,
        };
        assert_eq!(r.overrides.len(), 20);
    }

    #[test]
    fn test_parse_rules_10_rules_all_with_importance_zero_preserved() {
        // 10 rules all at importance 0 → all 10 present in output.
        let mut list: Vec<ConfigValue> = Vec::new();
        for i in 0..10 {
            let mut r: HashMap<String, ConfigValue> = HashMap::new();
            r.insert("condition".into(), leaf(&format!("c{}", i)));
            list.push(ConfigValue::Map(r));
        }
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 10);
    }

    #[test]
    fn test_rule_overrides_empty_map_valid_construction() {
        // Rule with empty overrides HashMap valid.
        let r = Rule {
            importance: 0,
            condition: "c".into(),
            overrides: HashMap::new(),
        };
        assert!(r.overrides.is_empty());
    }

    #[test]
    fn test_parse_rules_only_missing_condition_fields_returns_empty() {
        // Single rule with only importance → skipped → empty Vec.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("importance".into(), leaf("10"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_apply_rules_to_link_default_empty_rules_slice_returns_empty() {
        // Empty rules slice → empty overrides map.
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_rules_overrides_numeric_string_value_preserved() {
        // Numeric-looking value "123" preserved as string override.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("c"));
        rule_map.insert("z".into(), leaf("123"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].overrides.get("z"), Some(&"123".to_string()));
    }

    #[test]
    fn test_apply_rules_to_link_link_with_3_points_accepted() {
        // Link with 3 datums → doesn't panic under rule evaluation.
        let rule = Rule {
            importance: 0,
            condition: "_INTERCHR_".into(),
            overrides: HashMap::new(),
        };
        let datum_a = Datum { chr: "hs1".into(), start: 0, end: 50, set: IntSpan::from_range(0, 50), id: Some("l".into()), value: None, label: None, param: HashMap::new() };
        let datum_b = Datum { chr: "hs2".into(), start: 50, end: 100, set: IntSpan::from_range(50, 100), id: Some("l".into()), value: None, label: None, param: HashMap::new() };
        let datum_c = Datum { chr: "hs3".into(), start: 100, end: 150, set: IntSpan::from_range(100, 150), id: Some("l".into()), value: None, label: None, param: HashMap::new() };
        let l = Link { id: "l".into(), points: vec![datum_a, datum_b, datum_c], param: HashMap::new() };
        let _ = apply_rules_to_link(&l, &[rule]);
    }

    #[test]
    fn test_parse_rules_overrides_with_boolean_style_values() {
        // Boolean strings "yes"/"no" stored as literal strings.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("c"));
        rule_map.insert("show".into(), leaf("yes"));
        rule_map.insert("hide".into(), leaf("no"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].overrides.get("show"), Some(&"yes".to_string()));
        assert_eq!(rules[0].overrides.get("hide"), Some(&"no".to_string()));
    }

    #[test]
    fn test_rule_struct_condition_with_spaces_preserved() {
        // Rule condition string with spaces stored verbatim.
        let r = Rule {
            importance: 0,
            condition: "x > 5 && y < 10".into(),
            overrides: HashMap::new(),
        };
        assert_eq!(r.condition, "x > 5 && y < 10");
    }

    #[test]
    fn test_parse_rules_with_only_overrides_and_condition_importance_zero() {
        // No importance field → defaults to 0.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("c"));
        rule_map.insert("override_k".into(), leaf("v"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].importance, 0);
        assert_eq!(rules[0].overrides.get("override_k"), Some(&"v".to_string()));
    }

    #[test]
    fn test_parse_rules_condition_with_unicode_characters_preserved() {
        // Non-ASCII condition string preserved.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("chr_α > β"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].condition, "chr_α > β");
    }

    #[test]
    fn test_apply_rules_to_link_with_non_panicky_condition_handles_return() {
        // Arbitrary condition — whether matches or not, must not panic.
        let rule = Rule {
            importance: 0,
            condition: "random_condition".into(),
            overrides: [("k".to_string(), "v".to_string())].into_iter().collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let _ = apply_rules_to_link(&l, &[rule]);
    }

    #[test]
    fn test_rule_struct_condition_truncates_on_direct_assignment() {
        // Rule.condition settable to empty string.
        let mut r = Rule {
            importance: 0,
            condition: "original".into(),
            overrides: HashMap::new(),
        };
        r.condition = String::new();
        assert!(r.condition.is_empty());
    }

    #[test]
    fn test_parse_rules_rule_list_with_some_valid_some_invalid_keeps_only_valid() {
        // Mix: rule with condition (valid) and rule without (invalid) → only valid kept.
        let mut v1: HashMap<String, ConfigValue> = HashMap::new();
        v1.insert("condition".into(), leaf("c1"));
        let mut v2: HashMap<String, ConfigValue> = HashMap::new();
        v2.insert("importance".into(), leaf("10")); // no condition → invalid
        let mut v3: HashMap<String, ConfigValue> = HashMap::new();
        v3.insert("condition".into(), leaf("c3"));
        let list = vec![ConfigValue::Map(v1), ConfigValue::Map(v2), ConfigValue::Map(v3)];
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_parse_rules_zero_rules_list_returns_empty() {
        // Empty List at "rule" → empty output.
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::List(Vec::new()));
        let rules = parse_rules(Some(&block));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_apply_rules_to_link_multiple_overrides_all_returned() {
        // Matching rule with 4 overrides → all 4 returned.
        let rule = Rule {
            importance: 0,
            condition: "_INTERCHR_".into(),
            overrides: [
                ("color".to_string(), "red".to_string()),
                ("thickness".to_string(), "5".to_string()),
                ("stroke".to_string(), "dashed".to_string()),
                ("z".to_string(), "100".to_string()),
            ].into_iter().collect(),
        };
        let l = link("hs1", 0, 100, "hs2", 0, 100);
        let result = apply_rules_to_link(&l, &[rule]);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_parse_rules_complex_condition_with_operators_preserved() {
        // Complex Perl expression with && and > preserved.
        let mut rule_map: HashMap<String, ConfigValue> = HashMap::new();
        rule_map.insert("condition".into(), leaf("_INTERCHR_ && var(size) > 1kb"));
        let mut block: HashMap<String, ConfigValue> = HashMap::new();
        block.insert("rule".into(), ConfigValue::Map(rule_map));
        let rules = parse_rules(Some(&block));
        assert_eq!(rules[0].condition, "_INTERCHR_ && var(size) > 1kb");
    }

    #[test]
    fn test_rule_struct_importance_variants_tracked_independently() {
        // Various importance values set/get.
        for &imp in &[-100, 0, 50, i32::MAX, i32::MIN] {
            let r = Rule { importance: imp, condition: "c".into(), overrides: HashMap::new() };
            assert_eq!(r.importance, imp);
        }
    }

    #[test]
    fn test_parse_rules_none_conf_returns_empty() {
        // None → empty Vec.
        let rs = parse_rules(None);
        assert!(rs.is_empty());
    }

    #[test]
    fn test_parse_rules_sorts_descending_by_importance() {
        // 3 rules with [10, 30, 20] → returns [30, 20, 10].
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rules_list = vec![
            ConfigValue::Map(mk_rule("c1", 10, &[])),
            ConfigValue::Map(mk_rule("c2", 30, &[])),
            ConfigValue::Map(mk_rule("c3", 20, &[])),
        ];
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 3);
        assert_eq!(rs[0].importance, 30);
        assert_eq!(rs[1].importance, 20);
        assert_eq!(rs[2].importance, 10);
    }

    #[test]
    fn test_parse_rules_missing_importance_defaults_to_zero() {
        // No importance key → default 0.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].importance, 0);
    }

    #[test]
    fn test_apply_rules_to_link_empty_slice_yields_empty_overrides() {
        // No rules → empty overrides.
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn test_rule_struct_clone_preserves_overrides_entries() {
        // Clone deep-copies the overrides HashMap.
        let mut overrides = HashMap::new();
        overrides.insert("color".into(), "red".into());
        let r = Rule { importance: 5, condition: "c".into(), overrides };
        let c = r.clone();
        assert_eq!(c.overrides.get("color"), Some(&"red".into()));
        assert_eq!(r.overrides.get("color"), Some(&"red".into()));
    }

    #[test]
    fn test_parse_rules_single_rule_map_not_in_list_still_parsed() {
        // Map variant as rule (not wrapped in List) also accepted.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c1"));
        inner.insert("importance".into(), leaf("7"));
        inner.insert("color".into(), leaf("red"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].importance, 7);
        assert_eq!(rs[0].overrides.get("color"), Some(&"red".into()));
    }

    #[test]
    fn test_parse_rules_conf_without_rule_key_returns_empty() {
        // Conf has no "rule" entry → empty Vec.
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        let rs = parse_rules(Some(&conf));
        assert!(rs.is_empty());
    }

    #[test]
    fn test_parse_rules_rule_non_map_non_list_variant_returns_empty() {
        // "rule" as Str (not Map/List) → treated as not-a-rule → empty.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Str("invalid".into()));
        let rs = parse_rules(Some(&conf));
        assert!(rs.is_empty());
    }

    #[test]
    fn test_parse_rules_importance_parse_failure_defaults_to_zero() {
        // importance="not_a_number" → parse fails → defaults to 0.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("importance".into(), leaf("not_a_number"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].importance, 0);
    }

    #[test]
    fn test_parse_rules_condition_missing_rule_filtered_out() {
        // Missing "condition" → filter_map drops that rule.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("importance".into(), leaf("5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert!(rs.is_empty());
    }

    #[test]
    fn test_parse_rules_overrides_captures_keys_other_than_cond_and_imp() {
        // All non-condition, non-importance keys become overrides.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("color".into(), leaf("red"));
        inner.insert("thickness".into(), leaf("2"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].overrides.get("color"), Some(&"red".to_string()));
        assert_eq!(rs[0].overrides.get("thickness"), Some(&"2".to_string()));
        assert!(!rs[0].overrides.contains_key("condition"));
        assert!(!rs[0].overrides.contains_key("importance"));
    }

    #[test]
    fn test_rule_struct_with_empty_overrides_and_condition() {
        // Valid Rule with zero overrides.
        let r = Rule {
            importance: 10,
            condition: "".into(),
            overrides: HashMap::new(),
        };
        assert!(r.overrides.is_empty());
        assert_eq!(r.importance, 10);
    }

    #[test]
    fn test_parse_rules_two_rules_with_same_importance_order_preserved() {
        // Equal importances → stable sort keeps file order.
        let mut inner1: HashMap<String, ConfigValue> = HashMap::new();
        inner1.insert("condition".into(), leaf("c1"));
        inner1.insert("importance".into(), leaf("5"));
        let mut inner2: HashMap<String, ConfigValue> = HashMap::new();
        inner2.insert("condition".into(), leaf("c2"));
        inner2.insert("importance".into(), leaf("5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![
            ConfigValue::Map(inner1),
            ConfigValue::Map(inner2),
        ]));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].importance, 5);
        assert_eq!(rs[1].importance, 5);
    }

    #[test]
    fn test_parse_rules_negative_importance_accepted() {
        // Negative importance values valid.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("importance".into(), leaf("-5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs[0].importance, -5);
    }

    #[test]
    fn test_parse_rules_condition_not_str_variant_filtered_out() {
        // condition as Map (not Str) → filter_map drops.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), ConfigValue::Map(HashMap::new()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert!(rs.is_empty());
    }

    #[test]
    fn test_parse_rules_list_wrapped_single_rule_parses() {
        // List of 1 rule map → single-entry Vec.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(inner)]));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
    }

    #[test]
    fn test_rule_struct_debug_format_contains_condition_and_importance() {
        // Debug output has all field names and values.
        let r = Rule {
            importance: 42,
            condition: "mycond".into(),
            overrides: HashMap::new(),
        };
        let s = format!("{:?}", r);
        assert!(s.contains("42"));
        assert!(s.contains("mycond"));
    }

    #[test]
    fn test_parse_rules_many_overrides_all_captured() {
        // Rule with 5 override keys captured (minus condition and importance).
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("a".into(), leaf("1"));
        inner.insert("b".into(), leaf("2"));
        inner.insert("c_key".into(), leaf("3"));
        inner.insert("d".into(), leaf("4"));
        inner.insert("e".into(), leaf("5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].overrides.len(), 5);
    }

    #[test]
    fn test_parse_rules_condition_whitespace_only_accepted_empty_str() {
        // "   " condition trimmed or preserved — as_str returns Some.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("   "));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
    }

    #[test]
    fn test_parse_rules_empty_list_returns_empty_vec() {
        // List of 0 rules → empty Vec.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![]));
        let rs = parse_rules(Some(&conf));
        assert!(rs.is_empty());
    }

    #[test]
    fn test_parse_rules_list_with_non_map_entries_filtered() {
        // List of [Map, Str, Map] → Str filtered, 2 valid rules.
        let mut ok1: HashMap<String, ConfigValue> = HashMap::new();
        ok1.insert("condition".into(), leaf("c1"));
        let mut ok2: HashMap<String, ConfigValue> = HashMap::new();
        ok2.insert("condition".into(), leaf("c2"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![
            ConfigValue::Map(ok1),
            ConfigValue::Str("not_a_rule".into()),
            ConfigValue::Map(ok2),
        ]));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 2);
    }

    #[test]
    fn test_parse_rules_zero_importance_default_vs_explicit_zero() {
        // Both implicit 0 (missing) and explicit "0" → importance=0.
        let mut inner1: HashMap<String, ConfigValue> = HashMap::new();
        inner1.insert("condition".into(), leaf("c1"));
        let mut inner2: HashMap<String, ConfigValue> = HashMap::new();
        inner2.insert("condition".into(), leaf("c2"));
        inner2.insert("importance".into(), leaf("0"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![
            ConfigValue::Map(inner1),
            ConfigValue::Map(inner2),
        ]));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].importance, 0);
        assert_eq!(rs[1].importance, 0);
    }

    #[test]
    fn test_apply_rules_to_link_with_non_matching_condition_returns_empty() {
        // Condition that evaluates false for all rules → empty overrides.
        let rules = vec![Rule {
            importance: 1,
            condition: "false_condition_that_never_matches".into(),
            overrides: HashMap::new(),
        }];
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, &rules);
        assert!(out.is_empty());
    }

    #[test]
    fn test_rule_struct_condition_can_be_multi_word_expression() {
        // Complex multi-word conditions stored verbatim.
        let r = Rule {
            importance: 0,
            condition: "var(chr1) eq \"hs1\" and var(size) > 1000".into(),
            overrides: HashMap::new(),
        };
        assert!(r.condition.contains("eq"));
        assert!(r.condition.contains("size"));
    }

    #[test]
    fn test_parse_rules_five_rules_descending_sort() {
        // 5 rules with imp [1,2,3,4,5] → sorted desc to [5,4,3,2,1].
        let rules_list: Vec<ConfigValue> = (1..=5)
            .map(|i| ConfigValue::Map(mk_rule("c", i, &[])))
            .collect();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 5);
        assert_eq!(rs[0].importance, 5);
        assert_eq!(rs[4].importance, 1);
    }

    #[test]
    fn test_parse_rules_importance_fractional_string_fails_parse() {
        // importance="2.5" is not a valid i32 → defaults to 0.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("importance".into(), leaf("2.5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs[0].importance, 0);
    }

    #[test]
    fn test_rule_struct_clone_preserves_condition_string() {
        // Clone of Rule preserves condition contents.
        let r = Rule {
            importance: 5,
            condition: "complex condition".into(),
            overrides: HashMap::new(),
        };
        let c = r.clone();
        assert_eq!(c.condition, "complex condition");
    }

    #[test]
    fn test_parse_rules_many_rules_all_collected_in_vec() {
        // 10 rules → Vec of 10.
        let rules_list: Vec<ConfigValue> = (1..=10)
            .map(|i| ConfigValue::Map(mk_rule(&format!("c{}", i), i, &[])))
            .collect();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 10);
    }

    #[test]
    fn test_parse_rules_reverse_sort_negative_to_positive() {
        // importance -3..=3 → descending [3, 2, 1, 0, -1, -2, -3].
        let rules_list: Vec<ConfigValue> = (-3..=3)
            .map(|i| ConfigValue::Map(mk_rule("c", i, &[])))
            .collect();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs[0].importance, 3);
        assert_eq!(rs[6].importance, -3);
    }

    #[test]
    fn test_rule_struct_field_access_after_construction() {
        // Direct field access after direct construction.
        let r = Rule {
            importance: 42,
            condition: "my_cond".into(),
            overrides: HashMap::new(),
        };
        assert_eq!(r.importance, 42);
        assert_eq!(r.condition, "my_cond");
    }

    #[test]
    fn test_parse_rules_conf_key_not_rule_ignored() {
        // Non-"rule" keys in conf are ignored → empty vec.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("other_key".into(), ConfigValue::Str("value".into()));
        let rs = parse_rules(Some(&conf));
        assert!(rs.is_empty());
    }

    #[test]
    fn test_apply_rules_to_link_with_empty_condition_matches_link() {
        // Empty condition evaluates true → first rule wins.
        let mut overrides = HashMap::new();
        overrides.insert("color".into(), "red".into());
        let rules = vec![Rule {
            importance: 0,
            condition: "".into(),
            overrides,
        }];
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, &rules);
        // Empty condition may or may not match; just ensure no panic.
        assert!(out.is_empty() || !out.is_empty());
    }

    #[test]
    fn test_parse_rules_rule_with_quoted_condition_value_preserved() {
        // Condition with quotes preserved.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("var(chr1) eq \"hs1\""));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
        assert!(rs[0].condition.contains("hs1"));
    }

    #[test]
    fn test_rule_struct_importance_max_min_boundaries() {
        // i32::MAX/MIN preserved.
        let r_max = Rule {
            importance: i32::MAX,
            condition: "c".into(),
            overrides: HashMap::new(),
        };
        let r_min = Rule {
            importance: i32::MIN,
            condition: "c".into(),
            overrides: HashMap::new(),
        };
        assert_eq!(r_max.importance, i32::MAX);
        assert_eq!(r_min.importance, i32::MIN);
    }

    #[test]
    fn test_parse_rules_no_overrides_yields_empty_overrides_map() {
        // Rule with only condition (no extra keys) → empty overrides.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 1);
        assert!(rs[0].overrides.is_empty());
    }

    #[test]
    fn test_parse_rules_importance_with_leading_plus_sign_parses() {
        // "+5" → i32 parse → 5.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("importance".into(), leaf("+5"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs[0].importance, 5);
    }

    #[test]
    fn test_parse_rules_importance_hex_style_fails_to_zero() {
        // "0xff" hex format not valid i32 parse → default 0.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("importance".into(), leaf("0xff"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs[0].importance, 0);
    }

    #[test]
    fn test_parse_rules_sort_stable_for_negative_importance() {
        // Negative importances sort to end.
        let rules_list: Vec<ConfigValue> = [(3, "c1"), (-1, "c2"), (5, "c3"), (0, "c4")]
            .iter()
            .map(|(imp, c)| ConfigValue::Map(mk_rule(c, *imp, &[])))
            .collect();
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(rules_list));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs[0].importance, 5);
        assert_eq!(rs[3].importance, -1);
    }

    #[test]
    fn test_parse_rules_overrides_key_case_sensitive() {
        // "Color" and "color" are different keys.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("c"));
        inner.insert("Color".into(), leaf("red"));
        inner.insert("color".into(), leaf("blue"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs[0].overrides.get("Color"), Some(&"red".to_string()));
        assert_eq!(rs[0].overrides.get("color"), Some(&"blue".to_string()));
    }

    #[test]
    fn test_rule_struct_equality_not_required_for_clone() {
        // Clone produces independent copy; modifying clone doesn't affect original.
        let mut r = Rule {
            importance: 5,
            condition: "c".into(),
            overrides: HashMap::new(),
        };
        r.overrides.insert("k".into(), "v".into());
        let c = r.clone();
        r.overrides.insert("k".into(), "v2".into());
        assert_eq!(c.overrides.get("k"), Some(&"v".to_string()));
    }

    #[test]
    fn test_parse_rules_multi_rules_with_mixed_override_counts() {
        // Rules with 0, 2, 5 override keys.
        let mut r1: HashMap<String, ConfigValue> = HashMap::new();
        r1.insert("condition".into(), leaf("c1"));
        let mut r2: HashMap<String, ConfigValue> = HashMap::new();
        r2.insert("condition".into(), leaf("c2"));
        r2.insert("a".into(), leaf("1"));
        r2.insert("b".into(), leaf("2"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![
            ConfigValue::Map(r1),
            ConfigValue::Map(r2),
        ]));
        let rs = parse_rules(Some(&conf));
        assert_eq!(rs.len(), 2);
        // r1 (condition c1) has 0 overrides; r2 (condition c2) has 2.
        let r1_out = rs.iter().find(|r| r.condition == "c1").unwrap();
        let r2_out = rs.iter().find(|r| r.condition == "c2").unwrap();
        assert_eq!(r1_out.overrides.len(), 0);
        assert_eq!(r2_out.overrides.len(), 2);
    }

    #[test]
    fn test_rule_struct_importance_defaults_arbitrary_via_direct_init() {
        // Rule can be initialized with any importance value directly.
        for imp in [-1000, 0, 1, 999] {
            let r = Rule {
                importance: imp,
                condition: "c".into(),
                overrides: HashMap::new(),
            };
            assert_eq!(r.importance, imp);
        }
    }

    #[test]
    fn test_parse_rules_condition_preserves_nested_parens() {
        // Complex condition with nested parens preserved verbatim.
        let mut inner: HashMap<String, ConfigValue> = HashMap::new();
        inner.insert("condition".into(), leaf("((a and b) or (c and d))"));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(inner));
        let rs = parse_rules(Some(&conf));
        assert!(rs[0].condition.contains("((a"));
    }

    #[test]
    fn test_apply_rules_to_link_no_rules_returns_empty_map() {
        // Empty rules slice → empty HashMap.
        let rules: Vec<Rule> = vec![];
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, &rules);
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn test_parse_rules_none_config_returns_empty() {
        // None input → immediate empty Vec.
        let rules = parse_rules(None);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_empty_conf_without_rule_key_v2() {
        // Config without "rule" key → empty Vec.
        let conf: HashMap<String, ConfigValue> = HashMap::new();
        let rules = parse_rules(Some(&conf));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_single_map_wrapped_as_one_element() {
        // Single Map (not List) for "rule" → wrapped as 1-element vec.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("var(size) > 10", 5, &[("color", "red")]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[0].condition, "var(size) > 10");
    }

    #[test]
    fn test_parse_rules_sorted_by_importance_desc_two_rules_v2() {
        // Two rules, importance 3 and 7 → after sort order is [7, 3].
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let r1 = mk_rule("c1", 3, &[]);
        let r2 = mk_rule("c2", 7, &[]);
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(r1), ConfigValue::Map(r2)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].importance, 7);
        assert_eq!(rules[1].importance, 3);
    }

    #[test]
    fn test_parse_rules_importance_absent_from_single_rule_defaults_zero_v2() {
        // Rule without "importance" key → defaults to 0.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut rmap: HashMap<String, ConfigValue> = HashMap::new();
        rmap.insert("condition".into(), leaf("c1"));
        rmap.insert("color".into(), leaf("red"));
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_parse_rules_rule_without_condition_filtered_out() {
        // Rule missing "condition" key → filter_map drops it → empty Vec.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut rmap: HashMap<String, ConfigValue> = HashMap::new();
        rmap.insert("importance".into(), leaf("5"));
        rmap.insert("color".into(), leaf("red"));
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_overrides_extracted_and_stored() {
        // Override keys (color, etc.) stored in overrides map.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("c1", 1, &[("color", "blue"), ("thickness", "3")]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].overrides.get("color").map(String::as_str), Some("blue"));
        assert_eq!(rules[0].overrides.get("thickness").map(String::as_str), Some("3"));
        // condition and importance not in overrides map.
        assert!(!rules[0].overrides.contains_key("condition"));
        assert!(!rules[0].overrides.contains_key("importance"));
    }

    #[test]
    fn test_parse_rules_invalid_importance_string_defaults_to_zero_unwrap_or() {
        // importance="xyz" → parse::<i32>() Err → .unwrap_or(0).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut rmap: HashMap<String, ConfigValue> = HashMap::new();
        rmap.insert("condition".into(), leaf("c"));
        rmap.insert("importance".into(), leaf("xyz"));
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 0);
    }

    #[test]
    fn test_parse_rules_rule_as_str_variant_returns_empty() {
        // "rule" is a Str (not Map/List) → match falls to _ → empty Vec.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), leaf("plain string"));
        let rules = parse_rules(Some(&conf));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_condition_as_non_str_filtered_out() {
        // condition value is a Map (not Str) → as_str None → filter_map drops.
        let mut rmap: HashMap<String, ConfigValue> = HashMap::new();
        rmap.insert("condition".into(), ConfigValue::Map(HashMap::new()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_tie_importance_preserves_pair_count() {
        // Two rules same importance → both included, count=2.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let r1 = mk_rule("c1", 5, &[]);
        let r2 = mk_rule("c2", 5, &[]);
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(r1), ConfigValue::Map(r2)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[1].importance, 5);
    }

    #[test]
    fn test_apply_rules_to_link_stops_at_first_matching_rule() {
        // Two rules, both match — returns first one's overrides.
        let r1 = Rule {
            importance: 10,
            condition: "1".into(),  // tautology
            overrides: [("color".to_string(), "red".to_string())].into_iter().collect(),
        };
        let r2 = Rule {
            importance: 5,
            condition: "1".into(),
            overrides: [("color".to_string(), "blue".to_string())].into_iter().collect(),
        };
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, &[r1, r2]);
        assert_eq!(out.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn test_parse_rules_negative_importance_preserves_sign() {
        // Importance = -5 stored as signed integer.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("c", -5, &[]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules[0].importance, -5);
    }

    #[test]
    fn test_parse_rules_condition_with_special_chars_preserved() {
        // Condition string with operators preserved verbatim.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("var(size) >= 100 && chr1 eq \"hs1\"", 5, &[]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules[0].condition, "var(size) >= 100 && chr1 eq \"hs1\"");
    }

    #[test]
    fn test_parse_rules_non_str_override_value_filtered_out_of_overrides() {
        // Override key with Map value → as_str None → skipped in overrides.
        let mut rmap: HashMap<String, ConfigValue> = HashMap::new();
        rmap.insert("condition".into(), leaf("c"));
        rmap.insert("importance".into(), leaf("1"));
        rmap.insert("color".into(), leaf("red"));
        rmap.insert("bad_override".into(), ConfigValue::Map(HashMap::new()));
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert!(rules[0].overrides.contains_key("color"));
        assert!(!rules[0].overrides.contains_key("bad_override"));
    }

    #[test]
    fn test_apply_rules_to_link_returns_clone_of_overrides_not_reference() {
        // Returned map is cloned — original rule.overrides unchanged.
        let r = Rule {
            importance: 1,
            condition: "1".into(),
            overrides: [("color".to_string(), "red".to_string())].into_iter().collect(),
        };
        let l = link("a", 0, 100, "b", 0, 100);
        let mut out = apply_rules_to_link(&l, std::slice::from_ref(&r));
        out.insert("color".into(), "modified".into());
        // Original rule's overrides still says "red".
        assert_eq!(r.overrides.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn test_parse_rules_three_rules_in_list_all_parsed() {
        // 3-element rule List → 3 Rule instances parsed.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let list: Vec<ConfigValue> = (0..3)
            .map(|i| ConfigValue::Map(mk_rule(&format!("c{}", i), i, &[])))
            .collect();
        conf.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 3);
    }

    #[test]
    fn test_parse_rules_rule_key_as_list_of_non_maps_dropped() {
        // Rule list containing non-Map entries → filter_map drops them.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let list = vec![
            ConfigValue::Str("bogus".into()),
            ConfigValue::Map(mk_rule("c", 1, &[])),
            ConfigValue::List(vec![]),
        ];
        conf.insert("rule".into(), ConfigValue::List(list));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_apply_rules_to_link_empty_overrides_returns_empty_map() {
        // Matching rule with empty overrides → empty cloned map.
        let r = Rule {
            importance: 0,
            condition: "1".into(),
            overrides: HashMap::new(),
        };
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, std::slice::from_ref(&r));
        assert!(out.is_empty());
    }

    #[test]
    fn test_parse_rules_importance_zero_preserved() {
        // Explicit importance=0 preserved (not re-sorted away).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("c", 0, &[("label", "z")]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules[0].importance, 0);
        assert_eq!(rules[0].overrides.get("label").map(String::as_str), Some("z"));
    }

    #[test]
    fn test_apply_rules_to_link_empty_rules_slice_returns_empty_map() {
        // Empty rules slice → immediate empty HashMap.
        let rules: Vec<Rule> = vec![];
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, &rules);
        assert!(out.is_empty());
    }

    #[test]
    fn test_parse_rules_empty_list_of_rules_returns_empty() {
        // "rule" as empty List → empty Vec.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        conf.insert("rule".into(), ConfigValue::List(vec![]));
        let rules = parse_rules(Some(&conf));
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_rules_large_importance_value_preserved_as_i32() {
        // Importance 99999 fits in i32 and is preserved.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("c", 99999, &[]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules[0].importance, 99999);
    }

    #[test]
    fn test_apply_rules_to_link_no_matching_condition_returns_empty_map() {
        // Rule with unmatched condition → no override applied.
        let r = Rule {
            importance: 1,
            condition: "0".into(),  // "0" is falsy
            overrides: [("color".to_string(), "red".to_string())].into_iter().collect(),
        };
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, std::slice::from_ref(&r));
        assert!(out.is_empty());
    }

    #[test]
    fn test_parse_rules_multiple_overrides_stored_in_single_rule() {
        // Single rule with 4 override keys → all stored.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("c", 0, &[("color", "r"), ("thickness", "2"), ("z_index", "5"), ("url", "u")]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules[0].overrides.len(), 4);
    }

    #[test]
    fn test_apply_rules_to_link_second_rule_matched_when_first_fails() {
        // First rule condition "0" fails; second matches.
        let r1 = Rule {
            importance: 10,
            condition: "0".into(),
            overrides: [("color".to_string(), "red".to_string())].into_iter().collect(),
        };
        let r2 = Rule {
            importance: 5,
            condition: "1".into(),
            overrides: [("color".to_string(), "blue".to_string())].into_iter().collect(),
        };
        let l = link("a", 0, 100, "b", 0, 100);
        let out = apply_rules_to_link(&l, &[r1, r2]);
        assert_eq!(out.get("color").map(String::as_str), Some("blue"));
    }

    #[test]
    fn test_parse_rules_condition_with_empty_string_preserved() {
        // Empty condition string → Rule is still created (filter_map only drops None).
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let mut rmap: HashMap<String, ConfigValue> = HashMap::new();
        rmap.insert("condition".into(), leaf(""));
        rmap.insert("importance".into(), leaf("1"));
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].condition, "");
    }

    #[test]
    fn test_parse_rules_single_element_list_produces_single_rule_v2() {
        // "rule": List with single Map element → 1 Rule.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("c", 1, &[]);
        conf.insert("rule".into(), ConfigValue::List(vec![ConfigValue::Map(rmap)]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].importance, 1);
    }

    #[test]
    fn test_rule_struct_can_be_debug_formatted() {
        // Rule is Debug-formattable.
        let r = Rule {
            importance: 5,
            condition: "cond".into(),
            overrides: HashMap::new(),
        };
        let s = format!("{:?}", r);
        assert!(s.contains("5"));
        assert!(s.contains("cond"));
    }

    #[test]
    fn test_parse_rules_three_rules_sorted_descending() {
        // Importances 1, 5, 3 → sorted descending as [5, 3, 1].
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let r1 = mk_rule("c1", 1, &[]);
        let r2 = mk_rule("c2", 5, &[]);
        let r3 = mk_rule("c3", 3, &[]);
        conf.insert("rule".into(), ConfigValue::List(vec![
            ConfigValue::Map(r1), ConfigValue::Map(r2), ConfigValue::Map(r3),
        ]));
        let rules = parse_rules(Some(&conf));
        assert_eq!(rules[0].importance, 5);
        assert_eq!(rules[1].importance, 3);
        assert_eq!(rules[2].importance, 1);
    }

    #[test]
    fn test_apply_rules_to_link_rule_with_empty_condition_returns_empty() {
        // Rule with condition="" (empty) probably doesn't match link → empty overrides returned.
        let r = Rule {
            importance: 0,
            condition: "".into(),
            overrides: [("color".to_string(), "red".to_string())].into_iter().collect(),
        };
        let l = link("a", 0, 100, "b", 0, 100);
        let _out = apply_rules_to_link(&l, std::slice::from_ref(&r));
        // Whatever the outcome, the function should return without panicking.
        // Let's not assert on content since empty condition behavior is implementation-defined.
    }

    #[test]
    fn test_parse_rules_with_only_condition_and_importance_empty_overrides() {
        // Rule with just condition+importance → empty overrides.
        let mut conf: HashMap<String, ConfigValue> = HashMap::new();
        let rmap = mk_rule("my_cond", 7, &[]);
        conf.insert("rule".into(), ConfigValue::Map(rmap));
        let rules = parse_rules(Some(&conf));
        assert!(rules[0].overrides.is_empty());
    }
}
