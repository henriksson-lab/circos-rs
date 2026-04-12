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
                if k != "condition" && k != "importance" {
                    if let Some(s) = v.as_str() {
                        overrides.insert(k.clone(), s.to_string());
                    }
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
pub fn apply_rules_to_link(
    link: &Link,
    rules: &[Rule],
) -> HashMap<String, String> {
    for rule in rules {
        if expression::evaluate_link_condition(&rule.condition, link) {
            return rule.overrides.clone();
        }
    }
    HashMap::new()
}
