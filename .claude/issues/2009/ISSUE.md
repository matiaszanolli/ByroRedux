# 2009: MAT-D1-01: classify_pbr_keyword's unbounded substring match misfires the glass arm on common English words

https://github.com/matiaszanolli/ByroRedux/issues/2009

Labels: high, renderer, bug

**Severity**: HIGH · **Dimension**: Material (NIFAL canonical translation)
**Tier Violated**: no-fabrication
**Location**: `crates/core/src/ecs/components/material.rs:519-524` (the glass arm), `crates/core/src/ecs/components/material.rs:719-728` (`contains_any_ci`)
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIFAL_2026-07-16.md (MAT-D1-01)

## Description
The alpha-unaware glass arm matches `&["glass", "crystal", "ice", "gem"]` via `contains_any_ci`, a raw ASCII case-insensitive **substring** match with zero word-boundary logic. `"ice"` is a 3-letter substring embedded in many ordinary English words in Bethesda texture paths: `office`, `notice`, `device`, `justice`, `invoice`, `spice`, `voice`, `twice`, `advice`, `entice`, `sacrifice`, `practice`, `police`, `juice`, `dice`, `slice`. Any of these in a diffuse texture path routes the surface through the glass arm.

Same root-cause class as CLOSED #1819 (SpeedTree `"genericelderberry"` → glass), which patched only the reported SpeedTree symptom via a bypass, never root-caused `contains_any_ci` itself. Commit `e1b0294d` (#1925) claimed to sweep for other order-sensitive substring arms and missed this.

## Evidence
```rust
// material.rs:519
if contains_any_ci(path, &["glass", "crystal", "ice", "gem"]) {
    return PbrMaterial { roughness: 0.1, metalness: 0.0 };
}
// material.rs:719 — pure substring window, no boundary check
fn contains_any_ci(haystack: &str, keywords: &[&str]) -> bool {
    let hs = haystack.as_bytes();
    keywords.iter().any(|kw| {
        let kb = kw.as_bytes();
        if kb.is_empty() || kb.len() > hs.len() { return false; }
        hs.windows(kb.len()).any(|w| w.eq_ignore_ascii_case(kb))
    })
}
```

## Impact
Any FO3/FNV/Oblivion (or non-BGSM Skyrim/FO4) surface whose diffuse texture path contains one of the colliding words renders with `roughness = 0.1`, below the RT reflection gate (`roughness < 0.6`) — spuriously mirror-reflective ("wet floor"/chrome) with no in-game workaround, since NIFAL's no-render-time-fallback discipline means nothing downstream can catch a wrong import-time classification.

## Related
#1819 (CLOSED, same root cause, never root-caused).

## Suggested Fix
Add a word-boundary check to `contains_any_ci` (match only counts when the surrounding byte is not ASCII-alphanumeric / is a path separator or string boundary). Add a regression test asserting `office*.dds`/`notice*.dds`/`device*.dds` do not reach the glass arm.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (every other short/common keyword across all `classify_pbr_keyword` arms — `"fur"`, `"gem"`, others)
- [ ] CANONICAL-BOUNDARY: `Material::resolve_pbr` is a named canonical-boundary function — verify the fix stays entirely within parse-time classification
- [ ] TESTS: A regression test pins this specific fix
