//! Port of Perl Circos::printinfo/printdebug/printwarning/printout/printsvg/printdumper.
//!
//! The Perl originals read from `%CONF` globals; here we mirror the same
//! interface with a thread-safe state handle that `run()` can populate. When
//! the state is not initialized the functions degrade to no-ops (matching
//! Perl's `if $CONF{...}` guards).

use std::sync::RwLock;

#[derive(Default, Debug, Clone)]
pub struct DebugState {
    pub silent: bool,
    pub debug: u8,
    pub warnings: bool,
    pub debug_group: String,
    pub svg_make: bool,
}

static STATE: RwLock<Option<DebugState>> = RwLock::new(None);

/// Install the global debug state used by the print* helpers.
pub fn set_state(s: DebugState) {
    let mut w = STATE.write().unwrap();
    *w = Some(s);
}

/// Run `f` against the installed `DebugState` if present; otherwise return `default`.
fn with_state<R>(f: impl FnOnce(&DebugState) -> R, default: R) -> R {
    let g = STATE.read().unwrap();
    match g.as_ref() {
        Some(s) => f(s),
        None => default,
    }
}

/// Port of Perl `printout`: print unless silent.
pub fn printout(msg: &str) {
    if !with_state(|s| s.silent, false) {
        println!("{}", msg);
    }
}

/// Port of Perl `printinfo(@_)`: space-joined tokens, subject to `printout`'s silent gate.
pub fn printinfo(parts: &[&str]) {
    printout(&parts.join(" "));
}

/// Port of Perl `printdebug(@_)`: like printinfo but gated on `$CONF{debug}`.
pub fn printdebug(parts: &[&str]) {
    if with_state(|s| s.debug > 0, false) {
        let mut v = vec!["debug"];
        v.extend_from_slice(parts);
        printinfo(&v);
    }
}

/// Port of Perl `printwarning(@_)`: gated on `$CONF{warnings}`.
pub fn printwarning(parts: &[&str]) {
    if with_state(|s| s.warnings, false) {
        let mut v = vec!["warning"];
        v.extend_from_slice(parts);
        printinfo(&v);
    }
}

/// Port of Perl `printdumper(@_)`: debug-dump a serializable value.
/// Rust analogue: print via Debug formatter.
pub fn printdumper<T: std::fmt::Debug>(v: &T) {
    printinfo(&[&format!("{:#?}", v)]);
}

/// Port of Perl `printsvg`: write raw SVG bytes to the output stream iff SVG_MAKE.
/// In this Rust port SVG is collected into a buffer by the renderer; this helper
/// routes to stderr when `svg_make` is false (matching Perl's silent path) and
/// otherwise appends to whatever SVG writer is installed. For now, stdout-less no-op.
pub fn printsvg(_msg: &str) {
    // Collected by `SvgDocument` in the current architecture; no-op placeholder
    // kept for 1-1 correspondence with Perl.
}

/// Port of Perl `debug_or_group(group)`: true iff debug is on OR debug_group matches.
pub fn debug_or_group(group: &str) -> bool {
    with_state(
        |s| s.debug > 0 || (!s.debug_group.is_empty() && s.debug_group.contains(group)),
        false,
    )
}

/// Port of Perl `show_element(param)`: returns true iff the param hash does NOT
/// have `hide=true` AND does not have `show=false`. Used to decide whether a
/// config block is enabled.
pub fn show_element(
    param: &std::collections::HashMap<String, crate::config::types::ConfigValue>,
) -> bool {
    // hide=true always suppresses
    if let Some(hide) = param.get("hide").and_then(|v| v.as_str())
        && matches!(hide, "1" | "yes" | "true")
    {
        return false;
    }
    // show missing = default true; show=<falsy> = false
    match param.get("show").and_then(|v| v.as_str()) {
        None => true,
        Some(s) => matches!(s, "1" | "yes" | "true"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::ConfigValue;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Serialize tests that mutate the RwLock-protected STATE to prevent
    // parallel test runs from racing each other's `set_state`/`*STATE = None`.
    static STATE_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Build a `param` HashMap of string ConfigValues from `(key, value)` pairs for tests.
    fn mk(pairs: &[(&str, &str)]) -> HashMap<String, ConfigValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), ConfigValue::Str(v.to_string())))
            .collect()
    }

    /// Verify `show_element` returns true when the param map is empty.
    #[test]
    fn test_show_element_default_true_when_empty() {
        // No `show` / `hide` keys → visible.
        let p = mk(&[]);
        assert!(show_element(&p));
    }

    /// `hide=true` should win over `show=true` (Perl semantics: hide checked first).
    #[test]
    fn test_show_element_hide_precedence_over_show() {
        // hide=true must win over show=true (Perl checks hide first).
        let p = mk(&[("hide", "true"), ("show", "yes")]);
        assert!(!show_element(&p));
        let p = mk(&[("hide", "1"), ("show", "1")]);
        assert!(!show_element(&p));
    }

    /// Exercise the set of strings treated as truthy vs falsy by `show`.
    #[test]
    fn test_show_element_show_values() {
        // show values that mean true
        for v in ["1", "yes", "true"] {
            assert!(show_element(&mk(&[("show", v)])), "show={}", v);
        }
        // show values that mean false
        for v in ["0", "no", "false", "off", ""] {
            assert!(!show_element(&mk(&[("show", v)])), "show={}", v);
        }
    }

    /// Falsy `hide` values must NOT suppress an element (visibility falls through).
    #[test]
    fn test_show_element_hide_only_when_true() {
        // hide=false / hide=no / hide=0 should NOT suppress → visible (absent show).
        for v in ["0", "no", "false"] {
            assert!(show_element(&mk(&[("hide", v)])), "hide={}", v);
        }
    }

    /// When global STATE is None, `debug_or_group` defaults to false.
    #[test]
    fn test_debug_or_group_defaults_off_without_state() {
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Reset state to None so helper's no-state branch is exercised.
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group("anything"));
    }

    /// `debug_or_group` returns true if debug level > 0 or the group matches the configured CSV.
    #[test]
    fn test_debug_or_group_with_state() {
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // debug level > 0 → always true.
        set_state(DebugState {
            debug: 1,
            ..Default::default()
        });
        assert!(debug_or_group("nomatter"));
        // debug=0 but debug_group matches the query → true.
        set_state(DebugState {
            debug: 0,
            debug_group: "links,ticks".to_string(),
            ..Default::default()
        });
        assert!(debug_or_group("ticks"));
        // debug=0 and debug_group doesn't match → false.
        assert!(!debug_or_group("plots"));
        // Reset state so subsequent tests that expect no-state behavior aren't
        // surprised (tests are serialized via the run mutex for image-map, but
        // debug state lives in its own RwLock; clear defensively).
        *STATE.write().unwrap() = None;
    }

    /// Construct a fresh `DebugState` and verify every field is settable and clones intact.
    #[test]
    fn test_debug_state_new_cleared_field_by_field() {
        // Construct a fresh DebugState and verify each field flips as set.
        let s = DebugState {
            silent: true,
            debug: 3,
            warnings: true,
            debug_group: "links,plots".into(),
            svg_make: true,
        };
        assert!(s.silent);
        assert_eq!(s.debug, 3);
        assert!(s.warnings);
        assert_eq!(s.debug_group, "links,plots");
        assert!(s.svg_make);
        // Clone preserves all fields.
        let s2 = s.clone();
        assert_eq!(s2.debug_group, s.debug_group);
        assert_eq!(s2.debug, s.debug);
    }

    /// `hide` values that are not in the strict truthy set leave the element visible.
    #[test]
    fn test_show_element_hide_only_non_strict_falsy_values_allow_show() {
        // `hide` field values that aren't truthy (`"1"/"yes"/"true"`) → not
        // suppressed. Test "off", "False" (uppercased), and blank string.
        let p = mk(&[("hide", "off")]);
        assert!(show_element(&p));
        let p = mk(&[("hide", "False")]);
        assert!(show_element(&p));
        let p = mk(&[("hide", "")]);
        assert!(show_element(&p));
        // Explicit "true" does suppress.
        let p = mk(&[("hide", "true")]);
        assert!(!show_element(&p));
    }

    /// `show` truthiness uses a strict (case-sensitive) match — `"Yes"`/`"True"` don't count.
    #[test]
    fn test_show_element_show_uppercase_not_truthy() {
        // Strict match: "Yes"/"TRUE" don't match against lowercase-only patterns.
        let p = mk(&[("show", "YES")]);
        assert!(!show_element(&p));
        let p = mk(&[("show", "True")]);
        assert!(!show_element(&p));
    }

    /// `DebugState::default()` yields all-zero/empty fields.
    #[test]
    fn test_debug_state_default_values() {
        // Default DebugState: everything zero/false/empty.
        let s = DebugState::default();
        assert!(!s.silent);
        assert_eq!(s.debug, 0);
        assert!(!s.warnings);
        assert_eq!(s.debug_group, "");
        assert!(!s.svg_make);
    }

    /// Test: Debug or group substring match not tokenized.
    #[test]
    fn test_debug_or_group_substring_match_not_tokenized() {
        // debug_or_group uses `contains()` — substring, not CSV-tokenized.
        // So "linksticks" debug_group matches "links", "ticks", "ksti" (substring).
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "linksticks".to_string(),
            ..Default::default()
        });
        assert!(debug_or_group("links"));
        assert!(debug_or_group("ticks"));
        // Substring "ksti" spans the boundary — contains() matches.
        assert!(debug_or_group("ksti"));
        // A non-substring "plots" — not present.
        assert!(!debug_or_group("plots"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group empty query matches any nonempty group.
    #[test]
    fn test_debug_or_group_empty_query_matches_any_nonempty_group() {
        // Empty query string is always contained in any non-empty string per
        // `contains()` semantics — so an empty query → true whenever the group is non-empty.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "anything".to_string(),
            ..Default::default()
        });
        assert!(debug_or_group(""));
        // But when debug_group is empty, the `!is_empty()` guard short-circuits
        // the contains() check — empty query + empty group → false.
        set_state(DebugState {
            debug: 0,
            debug_group: "".to_string(),
            ..Default::default()
        });
        assert!(!debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Set state overwrites previous state.
    #[test]
    fn test_set_state_overwrites_previous_state() {
        // Calling set_state twice — the second call fully replaces the first.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 5,
            warnings: true,
            debug_group: "first".into(),
            ..Default::default()
        });
        assert!(debug_or_group("anything")); // debug=5 > 0 → always true
        set_state(DebugState {
            debug: 0,
            warnings: false,
            debug_group: "second".into(),
            ..Default::default()
        });
        // Now debug=0 and debug_group="second" — "anything" not a substring.
        assert!(!debug_or_group("anything"));
        assert!(debug_or_group("second"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element unknown hide value with explicit show.
    #[test]
    fn test_show_element_unknown_hide_value_with_explicit_show() {
        // hide="maybe" (not in `1|yes|true` truthy set) → hide gate doesn't trip.
        // Then explicit show="yes" evaluates truthy → visible.
        let p = mk(&[("hide", "maybe"), ("show", "yes")]);
        assert!(show_element(&p));
        // hide="2" (also not truthy literal — strict match only 1/yes/true) → visible.
        let p = mk(&[("hide", "2")]);
        assert!(show_element(&p));
        // hide="true" (truthy) + show=yes → still hidden (hide wins).
        let p = mk(&[("hide", "true"), ("show", "yes")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element non string child values treated as absent.
    #[test]
    fn test_show_element_non_string_child_values_treated_as_absent() {
        // `hide` or `show` value as non-string (Map/List) → as_str() None → falls
        // through to absence → default visible.
        let mut p: HashMap<String, ConfigValue> = HashMap::new();
        p.insert("hide".into(), ConfigValue::Map(HashMap::new()));
        p.insert("show".into(), ConfigValue::List(vec![]));
        // Neither read as str, so hide is treated as absent + show absent → visible.
        assert!(show_element(&p));
    }

    /// Test: Debug state clone produces independent state.
    #[test]
    fn test_debug_state_clone_produces_independent_state() {
        // Clone a DebugState + mutate — source untouched.
        let s = DebugState {
            silent: false,
            debug: 1,
            warnings: true,
            debug_group: "foo".into(),
            svg_make: false,
        };
        let mut c = s.clone();
        c.silent = true;
        c.debug = 5;
        c.debug_group = "bar".into();
        // Source preserved.
        assert!(!s.silent);
        assert_eq!(s.debug, 1);
        assert_eq!(s.debug_group, "foo");
        // Clone has mutations.
        assert!(c.silent);
        assert_eq!(c.debug, 5);
        assert_eq!(c.debug_group, "bar");
    }

    /// Test: Set state after clear state reappears.
    #[test]
    fn test_set_state_after_clear_state_reappears() {
        // Set → clear → set again: after clear, debug_or_group returns false;
        // after re-set, returns the new value.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // First set.
        set_state(DebugState {
            debug: 2,
            ..Default::default()
        });
        assert!(debug_or_group("any"));
        // Clear.
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group("any"));
        // Set with group but no debug=0 level.
        set_state(DebugState {
            debug: 0,
            debug_group: "plots,highlights".to_string(),
            ..Default::default()
        });
        assert!(debug_or_group("plots"));
        assert!(!debug_or_group("unrelated"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printout no panic under various states.
    #[test]
    fn test_printout_no_panic_under_various_states() {
        // printout must not panic under any state configuration — silent or not,
        // state-present or None. We can't easily capture stdout but invoking
        // without panic is the main invariant.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        printout("state-none"); // default silent=false → prints
        set_state(DebugState { silent: true, ..Default::default() });
        printout("silent-true"); // silent gate fires → no print
        set_state(DebugState { silent: false, ..Default::default() });
        printout("silent-false"); // silent gate false → prints
        // printinfo + printdebug + printwarning also should not panic.
        printinfo(&["hello", "world"]);
        set_state(DebugState { debug: 1, warnings: true, ..Default::default() });
        printdebug(&["d", "bg"]);
        printwarning(&["warn"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printdebug silent when state none or debug zero.
    #[test]
    fn test_printdebug_silent_when_state_none_or_debug_zero() {
        // printdebug gated on debug > 0. With no state → no prints/panics.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        printdebug(&["hello"]); // no panic, no print
        // With state but debug=0 → also no print.
        set_state(DebugState { debug: 0, ..Default::default() });
        printdebug(&["world"]);
        // debug=1 → prints (can't verify stdout but no panic).
        set_state(DebugState { debug: 1, ..Default::default() });
        printdebug(&["visible"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning silent when warnings off.
    #[test]
    fn test_printwarning_silent_when_warnings_off() {
        // printwarning only fires when warnings=true.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        printwarning(&["x"]); // state=None → no-op
        set_state(DebugState { warnings: false, ..Default::default() });
        printwarning(&["y"]); // warnings=false → no-op
        set_state(DebugState { warnings: true, ..Default::default() });
        printwarning(&["z"]); // warnings=true → prints
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element only hide set and truthy.
    #[test]
    fn test_show_element_only_hide_set_and_truthy() {
        // hide="1" alone (no show) → hidden.
        let p = mk(&[("hide", "1")]);
        assert!(!show_element(&p));
        // hide="yes" alone → hidden.
        let p = mk(&[("hide", "yes")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element show alone truthy or falsy.
    #[test]
    fn test_show_element_show_alone_truthy_or_falsy() {
        // show=1 alone (no hide) → visible.
        let p = mk(&[("show", "1")]);
        assert!(show_element(&p));
        // show=true alone → visible.
        let p = mk(&[("show", "true")]);
        assert!(show_element(&p));
        // show=0 alone → hidden (falsy).
        let p = mk(&[("show", "0")]);
        assert!(!show_element(&p));
        // show=no alone → hidden.
        let p = mk(&[("show", "no")]);
        assert!(!show_element(&p));
    }

    /// Test: Printdumper debuggable variants no panic.
    #[test]
    fn test_printdumper_debuggable_variants_no_panic() {
        // printdumper works for any Debug value; doesn't panic on various types.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        printdumper(&42i32);
        printdumper(&"hello");
        printdumper(&vec![1, 2, 3]);
        let mut m: HashMap<String, i32> = HashMap::new();
        m.insert("a".into(), 1);
        printdumper(&m);
    }

    /// Test: Printsvg always noop no panic.
    #[test]
    fn test_printsvg_always_noop_no_panic() {
        // printsvg is a Perl-compat stub — doesn't panic, doesn't emit.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        printsvg("<svg/>");
        printsvg("");
        set_state(DebugState { svg_make: true, ..Default::default() });
        printsvg("should-not-panic");
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state fields independent.
    #[test]
    fn test_debug_state_fields_independent() {
        // Each field on DebugState can be set independently.
        let s = DebugState {
            silent: true,
            debug: 3,
            warnings: false,
            debug_group: "links".into(),
            svg_make: true,
        };
        assert!(s.silent);
        assert_eq!(s.debug, 3);
        assert!(!s.warnings);
        assert_eq!(s.debug_group, "links");
        assert!(s.svg_make);
    }

    /// Test: Show element empty param returns true.
    #[test]
    fn test_show_element_empty_param_returns_true() {
        // Empty param HashMap → neither hide nor show set → visible by default.
        let p: HashMap<String, ConfigValue> = HashMap::new();
        assert!(show_element(&p));
    }

    /// Test: Debug or group multiple groups csv.
    #[test]
    fn test_debug_or_group_multiple_groups_csv() {
        // debug_group as CSV: "a,b,c" — each query matches substring.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "alpha,beta,gamma".to_string(),
            ..Default::default()
        });
        assert!(debug_or_group("alpha"));
        assert!(debug_or_group("beta"));
        assert!(debug_or_group("gamma"));
        assert!(!debug_or_group("delta"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element with capitalized truthy ignored.
    #[test]
    fn test_show_element_with_capitalized_truthy_ignored() {
        // Strict string comparison: "Yes"/"True" don't match lowercase-only patterns.
        let p = mk(&[("show", "Yes")]);
        assert!(!show_element(&p)); // mixed case → not matched.
        let p = mk(&[("show", "True")]);
        assert!(!show_element(&p));
    }

    /// Test: Printdumper with complex nested value.
    #[test]
    fn test_printdumper_with_complex_nested_value() {
        // printdumper should handle deeply nested debug values without panic.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        let nested: Vec<Vec<Vec<i32>>> = vec![
            vec![vec![1, 2, 3], vec![4, 5]],
            vec![vec![6], vec![7, 8, 9]],
        ];
        printdumper(&nested);
    }

    /// Test: Debug state default fields individually.
    #[test]
    fn test_debug_state_default_fields_individually() {
        // Each Default field is a specific value; test all 5.
        let s = DebugState::default();
        assert!(!s.silent);
        assert_eq!(s.debug, 0u8);
        assert!(!s.warnings);
        assert!(s.debug_group.is_empty());
        assert!(!s.svg_make);
    }

    /// Test: Set state second call replaces first.
    #[test]
    fn test_set_state_second_call_replaces_first() {
        // Second set_state overwrites — verify observable effect via debug_or_group.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 5, ..Default::default() });
        assert!(debug_or_group("anything"));
        set_state(DebugState { debug: 0, debug_group: String::new(), ..Default::default() });
        assert!(!debug_or_group("anything"));
        // Cleanup: clear STATE.
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group none state returns false.
    #[test]
    fn test_debug_or_group_none_state_returns_false() {
        // STATE=None → with_state returns the `default` arg (false here).
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group(""));
        assert!(!debug_or_group("any_group_name"));
    }

    /// Test: Show element hide falsy values fall to show check.
    #[test]
    fn test_show_element_hide_falsy_values_fall_to_show_check() {
        // hide=0/no/false → guard fails → falls through; absent show → default true.
        for falsy in ["0", "no", "false", "off", ""] {
            let p = mk(&[("hide", falsy)]);
            assert!(show_element(&p), "hide={} should fall to show default", falsy);
        }
    }

    /// Test: Show element hide uppercase yes not matched.
    #[test]
    fn test_show_element_hide_uppercase_yes_not_matched() {
        // matches!() is case-sensitive — "YES"/"TRUE"/"True" don't match lowercase patterns.
        let p = mk(&[("hide", "YES")]);
        assert!(show_element(&p));
        let p = mk(&[("hide", "True")]);
        assert!(show_element(&p));
        // Lowercase "yes"/"true" DO match → hide=true → element hidden.
        let p = mk(&[("hide", "yes")]);
        assert!(!show_element(&p));
    }

    /// Test: Printout no panic in both state none and set.
    #[test]
    fn test_printout_no_panic_in_both_state_none_and_set() {
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // STATE=None → printout still succeeds without panic.
        *STATE.write().unwrap() = None;
        printout("test msg with None state");
        // silent=true path also fine.
        set_state(DebugState { silent: true, ..Default::default() });
        printout("silenced msg");
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state clone mutation isolated from source.
    #[test]
    fn test_debug_state_clone_mutation_isolated_from_source() {
        // Cloning DebugState yields an independent struct — mutating the clone
        // doesn't affect the original.
        let a = DebugState {
            silent: true,
            debug: 3,
            warnings: true,
            debug_group: "alpha,beta".into(),
            svg_make: true,
        };
        let mut b = a.clone();
        b.silent = false;
        b.debug = 0;
        b.debug_group = "changed".into();
        // Source unchanged.
        assert!(a.silent);
        assert_eq!(a.debug, 3);
        assert_eq!(a.debug_group, "alpha,beta");
        // Clone reflects mutations.
        assert!(!b.silent);
        assert_eq!(b.debug, 0);
        assert_eq!(b.debug_group, "changed");
    }

    /// Test: Debug or group zero debug and empty group returns false.
    #[test]
    fn test_debug_or_group_zero_debug_and_empty_group_returns_false() {
        // debug=0 AND debug_group="" → both conditions false → returns false.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: String::new(),
            ..Default::default()
        });
        assert!(!debug_or_group(""));
        assert!(!debug_or_group("anything"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group matches substring within csv group.
    #[test]
    fn test_debug_or_group_matches_substring_within_csv_group() {
        // "alpha,beta,gamma" — contains("alpha") true, contains("delta") false.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "alpha,beta,gamma".into(),
            ..Default::default()
        });
        assert!(debug_or_group("alpha"));
        assert!(debug_or_group("beta"));
        assert!(!debug_or_group("delta"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element every truthy show value visible.
    #[test]
    fn test_show_element_every_truthy_show_value_visible() {
        // show truthy set is {"1","yes","true"} — note "on" is NOT recognized
        // (asymmetric with hide which uses the same set but for hiding).
        for v in ["1", "yes", "true"] {
            assert!(show_element(&mk(&[("show", v)])), "show={}", v);
        }
        // "on" falls to false (Some(s) not matching).
        assert!(!show_element(&mk(&[("show", "on")])));
    }

    /// Test: Show element every falsy show value hidden.
    #[test]
    fn test_show_element_every_falsy_show_value_hidden() {
        // Every falsy value → hidden.
        for v in ["0", "no", "false", "off", ""] {
            assert!(!show_element(&mk(&[("show", v)])), "show={}", v);
        }
    }

    /// Test: Printsvg is noop no panic.
    #[test]
    fn test_printsvg_is_noop_no_panic() {
        // printsvg placeholder — no panic, no observable output.
        printsvg("anything");
        printsvg("");
    }

    /// Test: Printwarning no panic when warnings disabled.
    #[test]
    fn test_printwarning_no_panic_when_warnings_disabled() {
        // warnings=false → guard fails → printwarning silently returns.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { warnings: false, silent: true, ..Default::default() });
        printwarning(&["a warning"]);
        // And with warnings=true + silent=true, printwarning still completes.
        set_state(DebugState { warnings: true, silent: true, ..Default::default() });
        printwarning(&["another warning"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printdebug no panic when debug level zero.
    #[test]
    fn test_printdebug_no_panic_when_debug_level_zero() {
        // debug=0 → guard fails → printdebug silently returns.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 0, silent: true, ..Default::default() });
        printdebug(&["noisy", "debug"]);
        // debug=3 + silent=true → still completes without panic.
        set_state(DebugState { debug: 3, silent: true, ..Default::default() });
        printdebug(&["noisy", "debug"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element both hide true and show true hide wins.
    #[test]
    fn test_show_element_both_hide_true_and_show_true_hide_wins() {
        // hide is checked first; when true, short-circuits to false regardless of show.
        let p = mk(&[("hide", "1"), ("show", "1")]);
        assert!(!show_element(&p));
        // Case: hide="yes" + show="true" → hidden.
        let p = mk(&[("hide", "yes"), ("show", "true")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug state default fields all falsy or zero.
    #[test]
    fn test_debug_state_default_fields_all_falsy_or_zero() {
        // Default for each field: bools=false, u8=0, String="".
        let d = DebugState::default();
        assert!(!d.silent);
        assert_eq!(d.debug, 0u8);
        assert!(!d.warnings);
        assert!(d.debug_group.is_empty());
        assert!(!d.svg_make);
    }

    /// Test: Printinfo joins multiple parts and nil no panic.
    #[test]
    fn test_printinfo_joins_multiple_parts_and_nil_no_panic() {
        // With silent=true, printinfo writes nothing observable — just verify no panic.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, ..Default::default() });
        printinfo(&[]); // empty slice
        printinfo(&["alpha"]);
        printinfo(&["alpha", "beta", "gamma", "delta"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide nonstandard value falls through to show check.
    #[test]
    fn test_show_element_hide_nonstandard_value_falls_through_to_show_check() {
        // hide="maybe" does NOT match the "1"/"yes"/"true" triple → hide check skipped.
        // show missing → default true.
        let p = mk(&[("hide", "maybe")]);
        assert!(show_element(&p));
        // hide="2" also not matched → true.
        let p2 = mk(&[("hide", "2")]);
        assert!(show_element(&p2));
    }

    /// Test: Debug or group state with debug nonzero always returns true regardless of group.
    #[test]
    fn test_debug_or_group_state_with_debug_nonzero_always_returns_true_regardless_of_group() {
        // When debug > 0, the group filter is bypassed — ANY group name matches.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 5, debug_group: String::new(), ..Default::default() });
        assert!(debug_or_group("anything"));
        assert!(debug_or_group("completely-unrelated-group"));
        assert!(debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group zero debug with exact group match returns true.
    #[test]
    fn test_debug_or_group_zero_debug_with_exact_group_match_returns_true() {
        // debug=0 but debug_group contains exact group name → true.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "chrparse,layout,render".into(),
            ..Default::default()
        });
        assert!(debug_or_group("layout"));
        assert!(debug_or_group("render"));
        // Non-member returns false (when debug==0).
        assert!(!debug_or_group("unrelated"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide case sensitive uppercase not matched.
    #[test]
    fn test_show_element_hide_case_sensitive_uppercase_not_matched() {
        // Only lowercase "1"/"yes"/"true" match — "YES"/"TRUE" do NOT trigger hide.
        let p = mk(&[("hide", "YES")]);
        assert!(show_element(&p));
        let p2 = mk(&[("hide", "TRUE")]);
        assert!(show_element(&p2));
        // But exact lowercase still hides.
        let p3 = mk(&[("hide", "yes")]);
        assert!(!show_element(&p3));
    }

    /// Test: Show element show zero explicit hides element.
    #[test]
    fn test_show_element_show_zero_explicit_hides_element() {
        // show="0" → NOT in ("1"/"yes"/"true") → returns false.
        let p = mk(&[("show", "0")]);
        assert!(!show_element(&p));
        // show="false" also falsy via not-matching.
        let p2 = mk(&[("show", "false")]);
        assert!(!show_element(&p2));
    }

    /// Test: Show element show all three truthy values.
    #[test]
    fn test_show_element_show_all_three_truthy_values() {
        // "1"/"yes"/"true" all match — show element.
        for v in ["1", "yes", "true"] {
            let p = mk(&[("show", v)]);
            assert!(show_element(&p), "expected true for show={}", v);
        }
    }

    /// Test: Debug state all fields independently settable.
    #[test]
    fn test_debug_state_all_fields_independently_settable() {
        // Each DebugState field can be set independently without touching others.
        let s = DebugState {
            debug: 3,
            debug_group: "chr,link".into(),
            silent: true,
            warnings: false,
            ..Default::default()
        };
        assert_eq!(s.debug, 3);
        assert_eq!(s.debug_group, "chr,link");
        assert!(s.silent);
        assert!(!s.warnings);
    }

    /// Test: Debug or group empty group input matches via substring.
    #[test]
    fn test_debug_or_group_empty_group_input_matches_via_substring() {
        // debug=0, debug_group="chr,link", group="" — "" is a substring of everything.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "chr,link".into(),
            ..Default::default()
        });
        // Empty group query matches any non-empty debug_group via `.contains("")`.
        assert!(debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide with non str child values treated as absent.
    #[test]
    fn test_show_element_hide_with_non_str_child_values_treated_as_absent() {
        // If `hide` is a Map (not a Str), .as_str() returns None → treated as absent → not hidden.
        let mut p: HashMap<String, ConfigValue> = HashMap::new();
        p.insert("hide".into(), ConfigValue::Map(HashMap::new()));
        assert!(show_element(&p));
        // Same for List values.
        let mut p2: HashMap<String, ConfigValue> = HashMap::new();
        p2.insert("hide".into(), ConfigValue::List(vec![]));
        assert!(show_element(&p2));
    }

    /// Test: Show element show with non str value treated as none so default true.
    #[test]
    fn test_show_element_show_with_non_str_value_treated_as_none_so_default_true() {
        // show value that's a Map/List → as_str returns None → None arm → default true.
        let mut p: HashMap<String, ConfigValue> = HashMap::new();
        p.insert("show".into(), ConfigValue::List(vec![]));
        assert!(show_element(&p));
    }

    /// Test: Debug state clone is independent from source.
    #[test]
    fn test_debug_state_clone_is_independent_from_source() {
        // Clone of DebugState — mutating clone doesn't affect source.
        let s = DebugState {
            debug: 1,
            debug_group: "a".into(),
            silent: false,
            warnings: true,
            ..Default::default()
        };
        let mut c = s.clone();
        c.debug = 5;
        c.debug_group = "b".into();
        assert_eq!(s.debug, 1);
        assert_eq!(s.debug_group, "a");
        assert_eq!(c.debug, 5);
        assert_eq!(c.debug_group, "b");
    }

    /// Test: Debug or group without any state set returns false.
    #[test]
    fn test_debug_or_group_without_any_state_set_returns_false() {
        // When global state is None (not yet set), debug_or_group returns false default.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group("anything"));
        assert!(!debug_or_group(""));
    }

    /// Test: Show element hide with all three truthy strings.
    #[test]
    fn test_show_element_hide_with_all_three_truthy_strings() {
        // "1", "yes", "true" all trigger hide precedence.
        for v in ["1", "yes", "true"] {
            let p = mk(&[("hide", v)]);
            assert!(!show_element(&p), "hide={} should hide", v);
        }
    }

    /// Test: Show element show value numeric zero string hides.
    #[test]
    fn test_show_element_show_value_numeric_zero_string_hides() {
        // show="0" → does NOT match "1"/"yes"/"true" → false → hidden.
        let p = mk(&[("show", "0")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug state default all fields falsy or empty.
    #[test]
    fn test_debug_state_default_all_fields_falsy_or_empty() {
        // Default::default gives debug=0, debug_group="", silent=false, warnings=false.
        let s = DebugState::default();
        assert_eq!(s.debug, 0);
        assert!(s.debug_group.is_empty());
        assert!(!s.silent);
        assert!(!s.warnings);
    }

    /// Test: Set state then debug or group reads current state.
    #[test]
    fn test_set_state_then_debug_or_group_reads_current_state() {
        // set_state installs a state; subsequent calls read it.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "match_me,other".into(),
            ..Default::default()
        });
        assert!(debug_or_group("match_me"));
        assert!(!debug_or_group("not_present"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide partial truthy match not accepted.
    #[test]
    fn test_show_element_hide_partial_truthy_match_not_accepted() {
        // "yesss" doesn't match "yes" exactly → NOT in truthy set → hide check fails → show.
        let p = mk(&[("hide", "yesss")]);
        assert!(show_element(&p));
        // "tru" prefix.
        let p2 = mk(&[("hide", "tru")]);
        assert!(show_element(&p2));
    }

    /// Test: Printout in silent mode no panic.
    #[test]
    fn test_printout_in_silent_mode_no_panic() {
        // printout in silent mode doesn't panic; returns no observable output.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, ..Default::default() });
        printout("test message");
        printout("");
        printout("multi\nline");
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning silent mode drops warning.
    #[test]
    fn test_printwarning_silent_mode_drops_warning() {
        // Warnings off → printwarning does nothing observable.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { warnings: false, silent: true, ..Default::default() });
        printwarning(&["test warning"]);
        printwarning(&[]);
        printwarning(&["a", "b", "c"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state clone shares no mutable state.
    #[test]
    fn test_debug_state_clone_shares_no_mutable_state() {
        // Clone yields independent Strings — not shared across clone.
        let s1 = DebugState {
            debug: 5,
            debug_group: "chr".into(),
            ..Default::default()
        };
        let mut s2 = s1.clone();
        s2.debug_group = "different".into();
        // s1 unaffected.
        assert_eq!(s1.debug_group, "chr");
        assert_eq!(s2.debug_group, "different");
    }

    /// Test: Show element empty hashmap returns true by default.
    #[test]
    fn test_show_element_empty_hashmap_returns_true_by_default() {
        // No hide/show → default is true (visible).
        let p: HashMap<String, ConfigValue> = HashMap::new();
        assert!(show_element(&p));
    }

    /// Test: Printdebug silent mode no panic.
    #[test]
    fn test_printdebug_silent_mode_no_panic() {
        // printdebug in silent mode doesn't panic.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, debug: 3, ..Default::default() });
        printdebug(&["msg1"]);
        printdebug(&["a", "b", "c"]);
        printdebug(&[]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printinfo with state none still no panic.
    #[test]
    fn test_printinfo_with_state_none_still_no_panic() {
        // STATE is None — printinfo should not panic.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        printinfo(&["test"]);
        printinfo(&[]);
    }

    /// Test: Printdumper with various types no panic.
    #[test]
    fn test_printdumper_with_various_types_no_panic() {
        // printdumper should accept various Debug-able types.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, ..Default::default() });
        printdumper(&42_i32);
        printdumper(&"hello");
        printdumper(&vec![1, 2, 3]);
        printdumper(&HashMap::<String, i32>::new());
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element both hide and show set truthy hide wins.
    #[test]
    fn test_show_element_both_hide_and_show_set_truthy_hide_wins() {
        // Both hide=true and show=true → hide precedence.
        let p = mk(&[("hide", "1"), ("show", "1")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element hide falsy show falsy returns false.
    #[test]
    fn test_show_element_hide_falsy_show_falsy_returns_false() {
        // Both hide and show falsy → show=0 branch → false.
        let p = mk(&[("hide", "0"), ("show", "0")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug or group multi groups all match.
    #[test]
    fn test_debug_or_group_multi_groups_all_match() {
        // Multiple groups in debug_group — all should match.
        let _lock = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState {
            debug: 0,
            debug_group: "a,b,c,d".into(),
            ..Default::default()
        });
        for g in ["a", "b", "c", "d"] {
            assert!(debug_or_group(g));
        }
        // Non-member → false.
        assert!(!debug_or_group("e"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printsvg noop no panic.
    #[test]
    fn test_printsvg_noop_no_panic() {
        // printsvg is a placeholder no-op — just verify it doesn't panic.
        printsvg("svg content here");
        printsvg("");
    }

    /// Test: Show element hide yes and true literal both suppress.
    #[test]
    fn test_show_element_hide_yes_and_true_literal_both_suppress() {
        // Both "yes" and "true" count as truthy for hide.
        let p_yes = mk(&[("hide", "yes")]);
        assert!(!show_element(&p_yes));
        let p_true = mk(&[("hide", "true")]);
        assert!(!show_element(&p_true));
    }

    /// Test: Show element show arbitrary nonzero string is false.
    #[test]
    fn test_show_element_show_arbitrary_nonzero_string_is_false() {
        // show="maybe" is not one of 1/yes/true → falsy → returns false.
        let p = mk(&[("show", "maybe")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug or group debug flag on matches any group.
    #[test]
    fn test_debug_or_group_debug_flag_on_matches_any_group() {
        // debug > 0 → every group returns true regardless of debug_group content.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug: 3,
            debug_group: "".into(),
            ..Default::default()
        });
        for g in ["anything", "other", "random"] {
            assert!(debug_or_group(g));
        }
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group when no state returns false.
    #[test]
    fn test_debug_or_group_when_no_state_returns_false() {
        // When STATE is None → with_state default=false → any group → false.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group("any"));
    }

    /// Test: Set state stores all fields retrievably.
    #[test]
    fn test_set_state_stores_all_fields_retrievably() {
        // After set_state, each with_state read sees the set values.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            silent: true,
            debug: 7,
            warnings: true,
            debug_group: "test_group".into(),
            svg_make: true,
        });
        with_state(
            |s| {
                assert!(s.silent);
                assert_eq!(s.debug, 7);
                assert!(s.warnings);
                assert_eq!(s.debug_group, "test_group");
                assert!(s.svg_make);
            },
            (),
        );
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state default all falsy or zero.
    #[test]
    fn test_debug_state_default_all_falsy_or_zero() {
        // Default DebugState has silent=false, debug=0, warnings=false, empty debug_group, svg_make=false.
        let s = DebugState::default();
        assert!(!s.silent);
        assert_eq!(s.debug, 0);
        assert!(!s.warnings);
        assert!(s.debug_group.is_empty());
        assert!(!s.svg_make);
    }

    /// Test: Debug or group empty debug group string always returns false.
    #[test]
    fn test_debug_or_group_empty_debug_group_string_always_returns_false() {
        // debug=0 AND empty debug_group → both conditions fail → false.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug: 0,
            debug_group: String::new(),
            ..Default::default()
        });
        assert!(!debug_or_group("a"));
        assert!(!debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element empty hide falsy but show 1 returns true.
    #[test]
    fn test_show_element_empty_hide_falsy_but_show_1_returns_true() {
        // hide="" → not in truthy set → check show → show="1" → true.
        let p = mk(&[("hide", ""), ("show", "1")]);
        assert!(show_element(&p));
    }

    /// Test: Printout silent mode drops message no output.
    #[test]
    fn test_printout_silent_mode_drops_message_no_output() {
        // silent=true → no panic; output suppression can't be verified directly
        // but the call completes without error.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            silent: true,
            ..Default::default()
        });
        printout("test message");
        printout("");
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state clone shares independent debug group.
    #[test]
    fn test_debug_state_clone_shares_independent_debug_group() {
        // DebugState is Clone → clone has independent debug_group field.
        let s1 = DebugState {
            debug_group: "original".into(),
            ..Default::default()
        };
        let mut s2 = s1.clone();
        s2.debug_group = "modified".into();
        assert_eq!(s1.debug_group, "original");
        assert_eq!(s2.debug_group, "modified");
    }

    /// Test: Debug or group debug group substring match succeeds.
    #[test]
    fn test_debug_or_group_debug_group_substring_match_succeeds() {
        // debug_group string contains target as substring → true.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug: 0,
            debug_group: "group_a,group_b,group_c".into(),
            ..Default::default()
        });
        // "group_b" is a substring.
        assert!(debug_or_group("group_b"));
        // "group_d" not in string.
        assert!(!debug_or_group("group_d"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide integer string 1 suppresses.
    #[test]
    fn test_show_element_hide_integer_string_1_suppresses() {
        // hide="1" is in truthy set → false.
        let p = mk(&[("hide", "1")]);
        assert!(!show_element(&p));
    }

    /// Test: Printinfo with no state still runs.
    #[test]
    fn test_printinfo_with_no_state_still_runs() {
        // printinfo uses printout — with no state, println! is the fallback; no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        printinfo(&["multi", "parts", "here"]);
        printinfo(&[]);
    }

    /// Test: Printdebug only runs when debug gt zero.
    #[test]
    fn test_printdebug_only_runs_when_debug_gt_zero() {
        // debug=0 → printdebug silent (no panic); we can't assert output,
        // but verify no panic for both cases.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug: 0,
            ..Default::default()
        });
        printdebug(&["msg"]);
        set_state(DebugState {
            debug: 3,
            silent: true, // suppress output
            ..Default::default()
        });
        printdebug(&["msg"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning warnings off suppresses warning.
    #[test]
    fn test_printwarning_warnings_off_suppresses_warning() {
        // warnings=false → printwarning noop; warnings=true w/ silent=true also no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            warnings: false,
            ..Default::default()
        });
        printwarning(&["suppressed"]);
        set_state(DebugState {
            warnings: true,
            silent: true,
            ..Default::default()
        });
        printwarning(&["silent path"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element show zero string is falsy regardless of hide absence.
    #[test]
    fn test_show_element_show_zero_string_is_falsy_regardless_of_hide_absence() {
        // show="0" is not in truthy set → false, even when hide absent.
        let p = mk(&[("show", "0")]);
        assert!(!show_element(&p));
    }

    /// Test: Printdumper variants no panic across types.
    #[test]
    fn test_printdumper_variants_no_panic_across_types() {
        // printdumper uses Debug fmt — various types exercised.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { silent: true, ..Default::default() });
        printdumper(&42);
        printdumper(&"string_literal");
        printdumper(&vec![1, 2, 3]);
        printdumper(&Some(3.14));
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state default then overridden mutably.
    #[test]
    fn test_debug_state_default_then_overridden_mutably() {
        // Struct starts default; fields settable post-construction.
        let mut s = DebugState::default();
        assert!(!s.silent);
        s.silent = true;
        s.debug = 2;
        assert!(s.silent);
        assert_eq!(s.debug, 2);
    }

    /// Test: Show element only hide field set to falsy passes.
    #[test]
    fn test_show_element_only_hide_field_set_to_falsy_passes() {
        // hide="0" alone → not truthy → falls to show (missing) → default true.
        let p = mk(&[("hide", "0")]);
        assert!(show_element(&p));
    }

    /// Test: Debug or group matches single entry group.
    #[test]
    fn test_debug_or_group_matches_single_entry_group() {
        // debug_group with one entry, lookup succeeds.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug_group: "only_this".into(),
            ..Default::default()
        });
        assert!(debug_or_group("only_this"));
        assert!(!debug_or_group("something_else"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element unknown hide value falls to show default.
    #[test]
    fn test_show_element_unknown_hide_value_falls_to_show_default() {
        // hide="maybe" not in truthy set → check show (missing) → default true.
        let p = mk(&[("hide", "maybe")]);
        assert!(show_element(&p));
    }

    /// Test: Show element show yes literal accepted as truthy.
    #[test]
    fn test_show_element_show_yes_literal_accepted_as_truthy() {
        // show="yes" is in truthy set {1,yes,true} → true.
        let p = mk(&[("show", "yes")]);
        assert!(show_element(&p));
    }

    /// Test: Debug state svg make field roundtrip.
    #[test]
    fn test_debug_state_svg_make_field_roundtrip() {
        // svg_make field settable + retrievable via set_state/with_state.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { svg_make: true, ..Default::default() });
        let val = with_state(|s| s.svg_make, false);
        assert!(val);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printsvg no panic across large input.
    #[test]
    fn test_printsvg_no_panic_across_large_input() {
        // No-op shim accepts arbitrary-length strings.
        printsvg(&"x".repeat(10_000));
        printsvg("<path d='...'/>");
    }

    /// Test: Debug state debug u8 max value accepted.
    #[test]
    fn test_debug_state_debug_u8_max_value_accepted() {
        // debug is u8 → max 255.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: u8::MAX, ..Default::default() });
        let v = with_state(|s| s.debug, 0);
        assert_eq!(v, u8::MAX);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element both hide and show missing returns true.
    #[test]
    fn test_show_element_both_hide_and_show_missing_returns_true() {
        // Empty hashmap → hide None, show None → default true.
        let p = mk(&[]);
        assert!(show_element(&p));
    }

    /// Test: Show element show on literal returns false.
    #[test]
    fn test_show_element_show_on_literal_returns_false() {
        // "on" is NOT in truthy {1,yes,true} set → false.
        let p = mk(&[("show", "on")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug or group with comma separated groups finds each.
    #[test]
    fn test_debug_or_group_with_comma_separated_groups_finds_each() {
        // debug_group string with multiple comma-separated names, all findable.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug_group: "alpha,beta,gamma".into(),
            ..Default::default()
        });
        assert!(debug_or_group("alpha"));
        assert!(debug_or_group("beta"));
        assert!(debug_or_group("gamma"));
        assert!(!debug_or_group("delta"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printinfo empty slice still runs.
    #[test]
    fn test_printinfo_empty_slice_still_runs() {
        // Empty parts slice → printout("") with trailing newline.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { silent: true, ..Default::default() });
        printinfo(&[]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide value case sensitive yes matches exactly.
    #[test]
    fn test_show_element_hide_value_case_sensitive_yes_matches_exactly() {
        // "YES" (uppercase) not in truthy set — case-sensitive.
        let p = mk(&[("hide", "YES")]);
        // Not truthy → falls through to show missing → true.
        assert!(show_element(&p));
    }

    /// Test: Printdumper empty slice like value no panic.
    #[test]
    fn test_printdumper_empty_slice_like_value_no_panic() {
        // Debug on empty Vec, empty String, unit — all fine.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { silent: true, ..Default::default() });
        let v: Vec<i32> = Vec::new();
        printdumper(&v);
        printdumper(&String::new());
        printdumper(&());
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group trailing comma in debug group still matches.
    #[test]
    fn test_debug_or_group_trailing_comma_in_debug_group_still_matches() {
        // "alpha,beta," — trailing comma doesn't break substring match.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug_group: "alpha,beta,".into(),
            ..Default::default()
        });
        assert!(debug_or_group("alpha"));
        assert!(debug_or_group("beta"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide falsy show missing falls to default true.
    #[test]
    fn test_show_element_hide_falsy_show_missing_falls_to_default_true() {
        // hide="0" (falsy) + show missing → default true.
        let p = mk(&[("hide", "0")]);
        assert!(show_element(&p));
    }

    /// Test: Debug state all fields mutable via assignment.
    #[test]
    fn test_debug_state_all_fields_mutable_via_assignment() {
        // DebugState fields all mutable.
        let mut s = DebugState::default();
        s.silent = true;
        s.debug = 5;
        s.warnings = true;
        s.debug_group = "x".into();
        s.svg_make = true;
        assert!(s.silent && s.debug == 5 && s.warnings && s.debug_group == "x" && s.svg_make);
    }

    /// Test: Debug or group single member substring match in group string.
    #[test]
    fn test_debug_or_group_single_member_substring_match_in_group_string() {
        // debug_group containing only "exact" should match "exact".
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug_group: "exact".into(),
            ..Default::default()
        });
        assert!(debug_or_group("exact"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printdebug without state skipped silently.
    #[test]
    fn test_printdebug_without_state_skipped_silently() {
        // No state → debug=0 via default → printdebug is no-op (no panic).
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        printdebug(&["skipped"]);
    }

    /// Test: Printwarning with multi token no panic.
    #[test]
    fn test_printwarning_with_multi_token_no_panic() {
        // printwarning takes a slice of parts.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: true, silent: true, ..Default::default() });
        printwarning(&["part1", "part2", "part3"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state default state returns false for debug or group.
    #[test]
    fn test_debug_state_default_state_returns_false_for_debug_or_group() {
        // Default state: debug=0, empty group → always false for any group.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState::default());
        assert!(!debug_or_group("any"));
        assert!(!debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide and show both falsy returns false.
    #[test]
    fn test_show_element_hide_and_show_both_falsy_returns_false() {
        // hide="0" + show="0" — show explicitly falsy → false.
        let p = mk(&[("hide", "0"), ("show", "0")]);
        assert!(!show_element(&p));
    }

    /// Test: Printsvg called before set state no panic.
    #[test]
    fn test_printsvg_called_before_set_state_no_panic() {
        // printsvg called without state → no-op no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        printsvg("some svg content");
    }

    /// Test: Printout without state no panic.
    #[test]
    fn test_printout_without_state_no_panic() {
        // No state + printout → should not panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        printout("hello");
    }

    /// Test: Show element show true explicit truthy returns true.
    #[test]
    fn test_show_element_show_true_explicit_truthy_returns_true() {
        // show="true" → in truthy set.
        let p = mk(&[("show", "true")]);
        assert!(show_element(&p));
    }

    /// Test: Debug or group empty string query with non empty group matches.
    #[test]
    fn test_debug_or_group_empty_string_query_with_non_empty_group_matches() {
        // Querying "" against non-empty group → contains("") always true.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState {
            debug_group: "alpha".into(),
            ..Default::default()
        });
        // contains("") returns true for any string.
        assert!(debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state struct clone preserves fields.
    #[test]
    fn test_debug_state_struct_clone_preserves_fields() {
        // DebugState Clone → all 5 fields copied.
        let s1 = DebugState {
            silent: true,
            debug: 3,
            warnings: true,
            debug_group: "g".into(),
            svg_make: true,
        };
        let s2 = s1.clone();
        assert_eq!(s2.silent, s1.silent);
        assert_eq!(s2.debug, s1.debug);
        assert_eq!(s2.warnings, s1.warnings);
        assert_eq!(s2.debug_group, s1.debug_group);
        assert_eq!(s2.svg_make, s1.svg_make);
    }

    /// Test: Debug state default all fields false and zero.
    #[test]
    fn test_debug_state_default_all_fields_false_and_zero() {
        // DebugState::default() — all fields at zero/false.
        let s = DebugState::default();
        assert!(!s.silent);
        assert_eq!(s.debug, 0);
        assert!(!s.warnings);
        assert!(s.debug_group.is_empty());
        assert!(!s.svg_make);
    }

    /// Test: Show element with many keys plus hide true.
    #[test]
    fn test_show_element_with_many_keys_plus_hide_true() {
        // Other keys shouldn't influence show/hide logic.
        let p = mk(&[("color", "red"), ("hide", "1"), ("thickness", "3")]);
        assert!(!show_element(&p));
    }

    /// Test: Printsvg accepts multiline svg content.
    #[test]
    fn test_printsvg_accepts_multiline_svg_content() {
        // Multi-line content → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { svg_make: false, ..Default::default() });
        printsvg("<svg>\n<rect/>\n</svg>");
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group check state after reset to none.
    #[test]
    fn test_debug_or_group_check_state_after_reset_to_none() {
        // Set then reset; debug_or_group returns false after reset.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: 5, ..Default::default() });
        assert!(debug_or_group("any"));
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group("any"));
    }

    /// Test: Set state overwrites existing state.
    #[test]
    fn test_set_state_overwrites_existing_state() {
        // Set twice; second set overwrites first.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: 1, ..Default::default() });
        set_state(DebugState { debug: 99, ..Default::default() });
        let v = with_state(|s| s.debug, 0);
        assert_eq!(v, 99);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide and show both set empty string.
    #[test]
    fn test_show_element_hide_and_show_both_set_empty_string() {
        // Both set to empty → hide falsy, show falsy → false.
        let p = mk(&[("hide", ""), ("show", "")]);
        assert!(!show_element(&p));
    }

    /// Test: Printwarning no state no panic.
    #[test]
    fn test_printwarning_no_state_no_panic() {
        // No state → warnings=false via default → printwarning silent.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        printwarning(&["skipped"]);
    }

    /// Test: Debug or group reset state none gives false for any group.
    #[test]
    fn test_debug_or_group_reset_state_none_gives_false_for_any_group() {
        // None state → default false.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        for name in ["x", "", "very_long_name", "alpha,beta"] {
            assert!(!debug_or_group(name));
        }
    }

    /// Test: Show element show explicit false returns false.
    #[test]
    fn test_show_element_show_explicit_false_returns_false() {
        // show="no" (non-truthy) → false.
        let p = mk(&[("show", "no")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element hide false passes through to default true.
    #[test]
    fn test_show_element_hide_false_passes_through_to_default_true() {
        // hide="false" does NOT suppress; no show key → default true.
        let p = mk(&[("hide", "false")]);
        assert!(show_element(&p));
    }

    /// Test: Debug or group with debug positive any group true.
    #[test]
    fn test_debug_or_group_with_debug_positive_any_group_true() {
        // debug > 0 → short-circuits to true regardless of group.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: 5, ..Default::default() });
        assert!(debug_or_group("any_group_name"));
        assert!(debug_or_group("another"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printout with state warnings true no panic.
    #[test]
    fn test_printout_with_state_warnings_true_no_panic() {
        // State with warnings=true still works for printout.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: true, ..Default::default() });
        printout("test message");
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide yes truthy returns false.
    #[test]
    fn test_show_element_hide_yes_truthy_returns_false() {
        // hide="yes" → truthy → false.
        let p = mk(&[("hide", "yes")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element show true with hide true hide wins.
    #[test]
    fn test_show_element_show_true_with_hide_true_hide_wins() {
        // hide="true" overrides show="true" → false.
        let p = mk(&[("hide", "true"), ("show", "true")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug or group with debug zero empty groups false.
    #[test]
    fn test_debug_or_group_with_debug_zero_empty_groups_false() {
        // debug=0, empty debug_group → debug_or_group any name → false.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState::default());
        assert!(!debug_or_group("any"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element show true explicit without hide returns true.
    #[test]
    fn test_show_element_show_true_explicit_without_hide_returns_true() {
        // show="true" alone → true.
        let p = mk(&[("show", "true")]);
        assert!(show_element(&p));
    }

    /// Test: Show element hide one numeric truthy.
    #[test]
    fn test_show_element_hide_one_numeric_truthy() {
        // hide="1" → truthy → false.
        let p = mk(&[("hide", "1")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug or group specific group name in debug group state.
    #[test]
    fn test_debug_or_group_specific_group_name_in_debug_group_state() {
        // debug_group = "mygroup" → debug_or_group("mygroup") → true.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug_group: "mygroup".into(), ..Default::default() });
        assert!(debug_or_group("mygroup"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printsvg with state accepts short content.
    #[test]
    fn test_printsvg_with_state_accepts_short_content() {
        // printsvg + existing state + short content → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState::default());
        printsvg("<g/>");
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning with warnings state true no panic.
    #[test]
    fn test_printwarning_with_warnings_state_true_no_panic() {
        // printwarning with warnings=true state → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: true, ..Default::default() });
        printwarning(&["warning message"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning with warnings false no panic.
    #[test]
    fn test_printwarning_with_warnings_false_no_panic() {
        // warnings=false → printwarning is a no-op.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: false, ..Default::default() });
        printwarning(&["ignored", "msg"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group with comma delimited group member match.
    #[test]
    fn test_debug_or_group_with_comma_delimited_group_member_match() {
        // debug_group="a,b,c" → "a" matches via contains.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug_group: "a,b,c".into(), ..Default::default() });
        assert!(debug_or_group("a"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element with no hide no show keys default true.
    #[test]
    fn test_show_element_with_no_hide_no_show_keys_default_true() {
        // Empty params map → default true (no hide, no show).
        let p: HashMap<String, ConfigValue> = HashMap::new();
        assert!(show_element(&p));
    }

    /// Test: Set state overwrites debug value.
    #[test]
    fn test_set_state_overwrites_debug_value() {
        // set_state twice → second overwrites first.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: 1, ..Default::default() });
        set_state(DebugState { debug: 42, ..Default::default() });
        let state = STATE.read().unwrap();
        assert_eq!(state.as_ref().unwrap().debug, 42);
        drop(state);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element show not truthy random string returns false.
    #[test]
    fn test_show_element_show_not_truthy_random_string_returns_false() {
        // show="random" not in truthy set → false.
        let p = mk(&[("show", "random")]);
        assert!(!show_element(&p));
    }

    /// Test: Printwarning empty parts slice no panic.
    #[test]
    fn test_printwarning_empty_parts_slice_no_panic() {
        // printwarning with 0 parts → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: true, ..Default::default() });
        printwarning(&[]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group with debug group not matching target.
    #[test]
    fn test_debug_or_group_with_debug_group_not_matching_target() {
        // debug_group="other" does not contain "target" → false.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug_group: "other".into(), ..Default::default() });
        assert!(!debug_or_group("target"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printsvg long content no panic.
    #[test]
    fn test_printsvg_long_content_no_panic() {
        // printsvg with long content → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState::default());
        let long: String = "<g/>".repeat(100);
        printsvg(&long);
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug state default has debug zero and no warnings.
    #[test]
    fn test_debug_state_default_has_debug_zero_and_no_warnings() {
        // Default DebugState has debug=0, warnings=false, empty debug_group.
        let d = DebugState::default();
        assert_eq!(d.debug, 0);
        assert!(!d.warnings);
        assert!(d.debug_group.is_empty());
    }

    /// Test: Show element show no keyword returns false.
    #[test]
    fn test_show_element_show_no_keyword_returns_false() {
        // show="no" → false.
        let p = mk(&[("show", "no")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug or group with many comma groups matches middle.
    #[test]
    fn test_debug_or_group_with_many_comma_groups_matches_middle() {
        // debug_group="a,b,c,d" → "c" matches via contains.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug_group: "a,b,c,d".into(), ..Default::default() });
        assert!(debug_or_group("c"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning multi part msg no panic.
    #[test]
    fn test_printwarning_multi_part_msg_no_panic() {
        // printwarning with multiple parts → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: true, ..Default::default() });
        printwarning(&["part1", " ", "part2", " ", "part3"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element show zero string returns false.
    #[test]
    fn test_show_element_show_zero_string_returns_false() {
        // show="0" → falsy → false.
        let p = mk(&[("show", "0")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element hide empty string no suppress.
    #[test]
    fn test_show_element_hide_empty_string_no_suppress() {
        // hide="" → empty not in truthy set → doesn't suppress → default true.
        let p = mk(&[("hide", "")]);
        assert!(show_element(&p));
    }

    /// Test: Debug state clone preserves all fields.
    #[test]
    fn test_debug_state_clone_preserves_all_fields() {
        // Clone DebugState preserves debug, warnings, debug_group.
        let s1 = DebugState { debug: 3, warnings: true, debug_group: "g1".into(), ..Default::default() };
        let s2 = s1.clone();
        assert_eq!(s2.debug, 3);
        assert!(s2.warnings);
        assert_eq!(s2.debug_group, "g1");
    }

    /// Test: Debug or group with large group string matches contained.
    #[test]
    fn test_debug_or_group_with_large_group_string_matches_contained() {
        // Very long debug_group string → still matches contained name.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        let long = "g1,g2,g3,g4,g5,g6,g7,g8,g9,g10,target,g11,g12";
        set_state(DebugState { debug_group: long.into(), ..Default::default() });
        assert!(debug_or_group("target"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Set state with all fields default then check values.
    #[test]
    fn test_set_state_with_all_fields_default_then_check_values() {
        // Default DebugState set then read back.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState::default());
        let s = STATE.read().unwrap();
        assert_eq!(s.as_ref().unwrap().debug, 0);
        assert!(!s.as_ref().unwrap().warnings);
        drop(s);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element show yes explicit truthy returns true.
    #[test]
    fn test_show_element_show_yes_explicit_truthy_returns_true() {
        // show="yes" → truthy → true.
        let p = mk(&[("show", "yes")]);
        assert!(show_element(&p));
    }

    /// Test: Printsvg empty content no panic.
    #[test]
    fn test_printsvg_empty_content_no_panic() {
        // printsvg with "" no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState::default());
        printsvg("");
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group with debug and group set true.
    #[test]
    fn test_debug_or_group_with_debug_and_group_set_true() {
        // debug>0 AND a group matches → still true (short-circuits on debug).
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: 1, debug_group: "g1".into(), ..Default::default() });
        assert!(debug_or_group("g1"));
        assert!(debug_or_group("other"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide numeric zero no suppress default true.
    #[test]
    fn test_show_element_hide_numeric_zero_no_suppress_default_true() {
        // hide="0" → not truthy → default show=true.
        let p = mk(&[("hide", "0")]);
        assert!(show_element(&p));
    }

    /// Test: Debug or group with max debug value any group true.
    #[test]
    fn test_debug_or_group_with_max_debug_value_any_group_true() {
        // u8::MAX debug value → still short-circuits any group query to true.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: 255, ..Default::default() });
        assert!(debug_or_group("anything"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printout no state returns without panic.
    #[test]
    fn test_printout_no_state_returns_without_panic() {
        // printout with None state → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        printout("silent");
    }

    /// Test: Printsvg no state returns without panic.
    #[test]
    fn test_printsvg_no_state_returns_without_panic() {
        // printsvg with None state → no panic.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        printsvg("<g/>");
    }

    /// Test: Debug or group with no state returns false any group.
    #[test]
    fn test_debug_or_group_with_no_state_returns_false_any_group() {
        // No state set → debug_or_group returns false always.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group("any"));
        assert!(!debug_or_group(""));
    }

    /// Test: Set state then reset multiple times.
    #[test]
    fn test_set_state_then_reset_multiple_times() {
        // Multiple set/reset cycles → each set overrides previous.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug: 1, ..Default::default() });
        set_state(DebugState { debug: 2, ..Default::default() });
        set_state(DebugState { debug: 3, ..Default::default() });
        let s = STATE.read().unwrap();
        assert_eq!(s.as_ref().unwrap().debug, 3);
        drop(s);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element with both show false and hide false shown.
    #[test]
    fn test_show_element_with_both_show_false_and_hide_false_shown() {
        // show="false" is falsy → false. hide="false" is falsy → not suppressed.
        // So with hide="false" + show="false" → show branch applies → false.
        let p = mk(&[("hide", "false"), ("show", "false")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element with only hide keyword no panic.
    #[test]
    fn test_show_element_with_only_hide_keyword_no_panic() {
        // Just "hide" with no show → hide decides.
        let p = mk(&[("hide", "no")]);
        // "no" not truthy → not suppressed → default true.
        assert!(show_element(&p));
    }

    /// Test: Debug or group empty query with non empty group.
    #[test]
    fn test_debug_or_group_empty_query_with_non_empty_group() {
        // Empty query "" in non-empty debug_group → contains("") always matches.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { debug_group: "mygroup".into(), ..Default::default() });
        // contains("") is always true.
        assert!(debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide and show both truthy hide wins.
    #[test]
    fn test_show_element_hide_and_show_both_truthy_hide_wins() {
        // hide="true" + show="yes" → hide always wins → false.
        let p = mk(&[("hide", "true"), ("show", "yes")]);
        assert!(!show_element(&p));
    }

    /// Test: Set state with warnings only preserved.
    #[test]
    fn test_set_state_with_warnings_only_preserved() {
        // DebugState with only warnings=true preserved.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: true, ..Default::default() });
        let s = STATE.read().unwrap();
        assert!(s.as_ref().unwrap().warnings);
        assert_eq!(s.as_ref().unwrap().debug, 0);
        drop(s);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning with single part msg no panic.
    #[test]
    fn test_printwarning_with_single_part_msg_no_panic() {
        // Single-part warning message.
        let _guard = STATE_TEST_LOCK.lock().expect("state lock");
        set_state(DebugState { warnings: true, ..Default::default() });
        printwarning(&["single"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group no state returns false.
    #[test]
    fn test_debug_or_group_no_state_returns_false() {
        // State = None → with_state default path → false.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *STATE.write().unwrap() = None;
        assert!(!debug_or_group("anything"));
    }

    /// Test: Debug or group debug on returns true for any group.
    #[test]
    fn test_debug_or_group_debug_on_returns_true_for_any_group() {
        // debug > 0 → short-circuits to true regardless of group.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 1, ..Default::default() });
        assert!(debug_or_group("unrelated"));
        assert!(debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide with non truthy keyword not hidden.
    #[test]
    fn test_show_element_hide_with_non_truthy_keyword_not_hidden() {
        // hide="0" doesn't match truthy set {1,yes,true} → element NOT hidden.
        let p = mk(&[("hide", "0")]);
        assert!(show_element(&p));
    }

    /// Test: Show element show key uppercase false falls through to default.
    #[test]
    fn test_show_element_show_key_uppercase_false_falls_through_to_default() {
        // Rust matches only literal "1","yes","true" — "YES" not matched → false.
        let p = mk(&[("show", "YES")]);
        assert!(!show_element(&p));
    }

    /// Test: Debug or group empty debug group with empty query returns false.
    #[test]
    fn test_debug_or_group_empty_debug_group_with_empty_query_returns_false() {
        // debug=0, debug_group empty → (!empty && contains) short-circuits false.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 0, debug_group: "".into(), ..Default::default() });
        assert!(!debug_or_group(""));
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group group matches substring of debug group.
    #[test]
    fn test_debug_or_group_group_matches_substring_of_debug_group() {
        // debug_group="config_cascade" contains "config" → true.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 0, debug_group: "config_cascade".into(), ..Default::default() });
        assert!(debug_or_group("config"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printinfo does not panic on empty parts.
    #[test]
    fn test_printinfo_does_not_panic_on_empty_parts() {
        // Empty parts slice → join("") → printout of "" (gated on silent).
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, ..Default::default() });
        printinfo(&[]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printdebug gated off when debug is zero.
    #[test]
    fn test_printdebug_gated_off_when_debug_is_zero() {
        // debug=0 → printdebug's gate `s.debug > 0` false → printinfo not called, no panic.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 0, ..Default::default() });
        printdebug(&["should_not_print"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printsvg is no op any input no panic.
    #[test]
    fn test_printsvg_is_no_op_any_input_no_panic() {
        // printsvg is a no-op placeholder — any input works without panic.
        printsvg("");
        printsvg("<svg></svg>");
        printsvg("large content 12345");
    }

    /// Test: Printout silent true suppresses output.
    #[test]
    fn test_printout_silent_true_suppresses_output() {
        // silent=true → printout gated off, no panic.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, ..Default::default() });
        printout("this should not emit");
        *STATE.write().unwrap() = None;
    }

    /// Test: Printdumper on tuple no panic.
    #[test]
    fn test_printdumper_on_tuple_no_panic() {
        // printdumper accepts any Debug value; no panic for silent state.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, ..Default::default() });
        printdumper(&(1, 2, "hi"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Printwarning gated off when warnings false.
    #[test]
    fn test_printwarning_gated_off_when_warnings_false() {
        // warnings=false → printwarning gated off, no panic.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { warnings: false, ..Default::default() });
        printwarning(&["should_not_print"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element both hide and show absent defaults shown.
    #[test]
    fn test_show_element_both_hide_and_show_absent_defaults_shown() {
        // No hide or show keys → element shown by default.
        let p = mk(&[("color", "red"), ("thickness", "2")]);
        assert!(show_element(&p));
    }

    /// Test: Show element show zero is falsy.
    #[test]
    fn test_show_element_show_zero_is_falsy() {
        // show="0" not in truthy → false.
        let p = mk(&[("show", "0")]);
        assert!(!show_element(&p));
    }

    /// Test: Show element show true is truthy element shown.
    #[test]
    fn test_show_element_show_true_is_truthy_element_shown() {
        // show="true" in truthy → shown.
        let p = mk(&[("show", "true")]);
        assert!(show_element(&p));
    }

    /// Test: Show element hide non matching value not hidden.
    #[test]
    fn test_show_element_hide_non_matching_value_not_hidden() {
        // hide="NO" not in truthy set → not hidden; show missing → default true.
        let p = mk(&[("hide", "NO")]);
        assert!(show_element(&p));
    }

    /// Test: Show element show yes makes visible.
    #[test]
    fn test_show_element_show_yes_makes_visible() {
        // show="yes" in truthy → visible.
        let p = mk(&[("show", "yes")]);
        assert!(show_element(&p));
    }

    /// Test: Show element hide yes overrides show true.
    #[test]
    fn test_show_element_hide_yes_overrides_show_true() {
        // hide="yes" wins over show="true".
        let p = mk(&[("hide", "yes"), ("show", "true")]);
        assert!(!show_element(&p));
    }

    /// Test: Set state multiple times preserves last set.
    #[test]
    fn test_set_state_multiple_times_preserves_last_set() {
        // Last set_state wins.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 1, ..Default::default() });
        set_state(DebugState { debug: 5, ..Default::default() });
        assert!(debug_or_group("anything"));  // Still debug > 0.
        *STATE.write().unwrap() = None;
    }

    /// Test: Debug or group with nonempty group nonmatching query false.
    #[test]
    fn test_debug_or_group_with_nonempty_group_nonmatching_query_false() {
        // debug_group="foo", query="bar" — bar not in foo → debug=0 short-circuits false.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 0, debug_group: "foo".into(), ..Default::default() });
        assert!(!debug_or_group("bar"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element show 1 is truthy.
    #[test]
    fn test_show_element_show_1_is_truthy() {
        // show="1" → truthy → visible.
        let p = mk(&[("show", "1")]);
        assert!(show_element(&p));
    }

    /// Test: Show element hide 1 is truthy hide.
    #[test]
    fn test_show_element_hide_1_is_truthy_hide() {
        // hide="1" → truthy hide → hidden.
        let p = mk(&[("hide", "1")]);
        assert!(!show_element(&p));
    }

    /// Test: Printinfo gated silent with multiple parts no panic.
    #[test]
    fn test_printinfo_gated_silent_with_multiple_parts_no_panic() {
        // Silent state → join multiple parts, no panic.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { silent: true, ..Default::default() });
        printinfo(&["part1", "part2", "part3"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Printdebug with debug 1 and silent gates correctly.
    #[test]
    fn test_printdebug_with_debug_1_and_silent_gates_correctly() {
        // debug=1 allows printdebug; silent=true gates printinfo output.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 1, silent: true, ..Default::default() });
        printdebug(&["msg"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element with multiple ignored keys and no hide show default.
    #[test]
    fn test_show_element_with_multiple_ignored_keys_and_no_hide_show_default() {
        // Extra param keys unrelated to hide/show → default (visible).
        let p = mk(&[("color", "red"), ("width", "2"), ("label", "x")]);
        assert!(show_element(&p));
    }

    /// Test: Debug or group with debug plus matching group true.
    #[test]
    fn test_debug_or_group_with_debug_plus_matching_group_true() {
        // Both debug > 0 and group matches → true.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 1, debug_group: "foo".into(), ..Default::default() });
        assert!(debug_or_group("foo"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Set state and immediate get through debug or group.
    #[test]
    fn test_set_state_and_immediate_get_through_debug_or_group() {
        // After set_state, debug_or_group reflects new state.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { debug: 3, ..Default::default() });
        assert!(debug_or_group("any"));
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element hide false string keeps element visible.
    #[test]
    fn test_show_element_hide_false_string_keeps_element_visible() {
        // hide="false" not in {1,yes,true} → not hidden; show default → visible.
        let p = mk(&[("hide", "false")]);
        assert!(show_element(&p));
    }

    /// Test: Debug state default silent false warnings false.
    #[test]
    fn test_debug_state_default_silent_false_warnings_false() {
        // DebugState::default() has silent=false, warnings=false.
        let s = DebugState::default();
        assert!(!s.silent);
        assert!(!s.warnings);
        assert_eq!(s.debug, 0);
    }

    /// Test: Printinfo with various state warnings and debug mix.
    #[test]
    fn test_printinfo_with_various_state_warnings_and_debug_mix() {
        // warnings=true, debug=1 → both gates open; no panic with 2-part msg.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState { warnings: true, debug: 1, silent: true, ..Default::default() });
        printinfo(&["a", "b"]);
        *STATE.write().unwrap() = None;
    }

    /// Test: Show element with empty map defaults visible.
    #[test]
    fn test_show_element_with_empty_map_defaults_visible() {
        // Empty param → default is visible.
        let p: HashMap<String, ConfigValue> = HashMap::new();
        assert!(show_element(&p));
    }

    /// Test: Debug or group on fresh state with no debug group false.
    #[test]
    fn test_debug_or_group_on_fresh_state_with_no_debug_group_false() {
        // debug=0, group="" → debug_or_group returns false regardless of query.
        let _guard = STATE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_state(DebugState::default());
        assert!(!debug_or_group("any"));
        *STATE.write().unwrap() = None;
    }
}
