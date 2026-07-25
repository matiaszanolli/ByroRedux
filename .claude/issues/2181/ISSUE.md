# SAVE-D2-NEW-07: #1714 guard's serde(default) detection is a line-prefix match — misses non-first-key ordering

- **GitHub Issue**: #2181
- **Source Audit**: `docs/audits/AUDIT_SAVE_2026-07-25.md`
- **Severity**: LOW
- **Dimension**: Registry & (De)serialization Fidelity
- **Location**: `byroredux/src/save_io.rs:1649-1656` (`serde_default_on_saved_struct_requires_format_major_bump`)
- **Status at filing**: NEW
- **Data-Loss Class**: none (latent robustness gap; no live instance triggers it)
- **Labels applied**: `low`, `tech-debt`, `bug`

## Description
The `#1714` regression guard scans every save-participating source file (`SAVE_TYPE_SOURCES`) for a line whose trimmed text `starts_with("#[serde(default")`, flagging any addition of a bare `#[serde(default)]` (or `#[serde(default = "...")]`) attribute on a saved struct's field. This correctly catches `#[serde(default)]` and `#[serde(default, ...)]` (both start with the matched prefix), but would **miss** the semantically identical `#[serde(skip_serializing_if = "...", default)]` — a legal, idiomatic serde ordering where `default` appears after another key in the same attribute list.

Verified this exact ordering does not currently exist anywhere in the 21 files `SAVE_TYPE_SOURCES` scans (grepped every file for any multi-key `serde(...)` attribute — none found), so there is no live gap today; this is purely a static-analysis blind spot in the guard itself.

## Evidence
```rust
if line.trim_start().starts_with("#[serde(default") {
    offenders.push(format!("{rel}:{}", i + 1));
}
```
A future field like `#[serde(skip_serializing_if = "Vec::is_empty", default)]` on any registered type would not trip this check, silently reintroducing the exact SAVE-D2-01 hazard (an old save missing the new field loads with it silently default-filled) with the regression guard reporting green.

## Impact
None today. Becomes live only if a maintainer adds `default` as a non-first key inside a multi-key `#[serde(...)]` attribute on any of the ~15 save-participating types — an easy mistake to make since it's valid, common serde style, and nothing in the codebase currently steers away from it.

## Related
Sibling gap-class to the guard's own documented residual (the "new-`Option`" half it already admits it can't catch statically) — this is a narrower miss on the half it claims to catch fully.

## Suggested Fix
Broaden the match to `line.contains("#[serde(") && line.contains("default")` (accepting some false positives, e.g. a field literally named `default`, which is rare and cheap to allowlist), or parse the attribute with `syn` for an exact check. Either is a small diff to a test-only file.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. a fixture line using `#[serde(skip_serializing_if = "...", default)]` that the broadened guard must catch)
