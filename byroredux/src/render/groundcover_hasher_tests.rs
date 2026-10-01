//! #5119 (REG-2026-09-29-03) — pin the hasher on `render/groundcover.rs`,
//! the per-frame ground-cover path whose #4607 fix (std → FxHash end to
//! end) shipped with no source-scan guard: unlike `skin_offsets`,
//! `light_history`, `SkinSlotPool` and `rigid_motion_history`, nothing kept
//! a reintroduced std map from compiling back in. A source assertion, like
//! its #2923/#2985 siblings — a map's hasher is not observable from a value
//! at runtime, and the declaration text is exactly what regresses.
//!
//! The one std `HashSet` in the file lives in its trailing test module,
//! which the production cut excludes; a future production site that needs
//! a map must take `FxHashMap`/`FxHashSet` (`rustc_hash`), the same
//! hot-path rule `_audit-common.md` records for the other four collections.

/// `groundcover.rs` production text: everything before its first
/// `#[cfg(test)] mod` test module. The `#[cfg(test)] fn` further up (the
/// test-only `keep_nearest_chunks` helper) is deliberately NOT a cut —
/// only the trailing test module is excluded, because that is where the
/// file's own test literals (including its std `HashSet`) live.
fn production_text() -> &'static str {
    const SOURCE: &str = include_str!("groundcover.rs");
    SOURCE
        .split_once("\n#[cfg(test)]\nmod ")
        .expect("groundcover.rs lost its test module — the production cut has nothing to anchor on")
        .0
}

/// Lines whose *code* (with `//`-prefixed comment text stripped, so a doc
/// mention of the std types cannot read as a declaration — `///` and `//!`
/// lines strip to nothing) mentions `HashMap` / `HashSet` at all.
fn map_lines(src: &str) -> Vec<(usize, &str)> {
    src.lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let code = line.split("//").next().unwrap_or("");
            let mentions = ["HashMap", "HashSet"]
                .iter()
                .any(|needle| code.contains(needle));
            mentions.then_some((idx + 1, code))
        })
        .collect()
}

/// True when `needle` occurs in `code` at least once with no `Fx` prefix —
/// i.e. not spelled `FxHashMap` / `FxHashSet` (which also covers the
/// fully-qualified `rustc_hash::FxHashMap` spelling). `use
/// std::collections::HashMap`, `HashMap::new()` and a bare `HashMap<K, V>`
/// declaration all match.
fn occurs_unprefixed(code: &str, needle: &str) -> bool {
    code.match_indices(needle)
        .any(|(at, _)| !code[..at].ends_with("Fx"))
}

/// The violations: production lines that name a map/set type without the
/// `Fx` prefix. Checked on the comment-stripped code so a trailing
/// `// not std HashMap` remark cannot false-positive.
fn unprefixed_map_sites(src: &str) -> Vec<(usize, &str)> {
    map_lines(src)
        .into_iter()
        .filter(|(_, code)| {
            ["HashMap", "HashSet"]
                .iter()
                .any(|needle| occurs_unprefixed(code, needle))
        })
        .collect()
}

#[test]
fn groundcover_hot_path_maps_are_not_siphash() {
    let production = production_text();

    // The file maps by Fx today (the import plus three declarations); if
    // the extractor stops seeing them, the assertion below proves nothing.
    assert!(
        !map_lines(production).is_empty(),
        "groundcover.rs no longer declares any map/set — did the chunk \
         selection move files? Update this guard's include path (#5119)"
    );

    let violations = unprefixed_map_sites(production);
    assert!(
        violations.is_empty(),
        "groundcover.rs's per-frame path must hash with FxHashMap/FxHashSet, \
         not a SipHash std map (#2923 hot-path rule, #4607 fix — reintroduced \
         at):\n{}",
        violations
            .iter()
            .map(|(line, text)| format!("  {line}: {text}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// The extraction accepts both Fx spellings and rejects a std swap — pins
/// the detector itself so a filter that silently stopped matching cannot
/// read as a clean pass (the shape the #1368/#2174/#4607 regressions took).
#[test]
fn the_groundcover_extraction_accepts_both_fx_spellings_and_rejects_a_std_swap() {
    let fx_lines = [
        "use rustc_hash::{FxHashMap, FxHashSet};",
        "    desired: FxHashMap<ChunkKey, ChunkCandidate>,",
        "    resident: rustc_hash::FxHashSet<ChunkKey>,",
    ];
    assert!(
        fx_lines
            .iter()
            .all(|line| unprefixed_map_sites(line).is_empty()),
        "the Fx spellings must not be flagged"
    );

    let swapped = [
        "use std::collections::HashMap;",
        "    desired: HashMap<ChunkKey, ChunkCandidate>,",
        "    resident: std::collections::HashSet<ChunkKey>,",
        "    let seen = HashSet::new();",
    ];
    for line in swapped {
        assert!(
            !unprefixed_map_sites(line).is_empty(),
            "the std spelling {line:?} must be flagged — the detector stopped matching"
        );
    }

    // Doc-comment mentions are not declarations.
    let documented = "/// compare with std HashMap lookup\nfn f() {}";
    assert!(
        unprefixed_map_sites(documented).is_empty(),
        "a doc-comment mention of the std types must not be flagged"
    );
}
