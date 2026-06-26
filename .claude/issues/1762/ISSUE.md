# TD8-005: RawDependency.name parsed from TOML then dropped (masked-dead field)

_Filed 2026-06-26 as #1762 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1762` for live state)._

**Severity**: LOW · **Dimension**: 8 — Dead Code
**Location**: `crates/plugin/src/manifest.rs:70-75`
**Status**: NEW · **Audit**: TD8-005

## Description
`RawDependency { uuid, #[allow(dead_code)] name }`. `Manifest::from_toml` maps dependencies via `.map(|d| PluginId::from_uuid(d.uuid))` (manifest.rs:48) — `name` is never read into the public `Manifest`. There is no `#[serde(deny_unknown_fields)]` on the struct, so serde silently ignores unknown TOML keys; the `name` field is not required to parse a `[[dependencies]]` block that includes `name = "…"`. Classic "silence the warning instead of deleting."

## Evidence
`grep '\.name' manifest.rs` → only `raw.plugin.name` (the plugin's own name, line 42) is consumed; `RawDependency.name` is never referenced outside its declaration. `grep deny_unknown_fields manifest.rs` → none.

## Suggested Fix
Delete the `name` field (serde ignores the TOML key with no `deny_unknown_fields`). OR, if dependency display-names are a near-term feature, propagate into a public `PluginDependency { id, name }` on `Manifest`. Default: delete.

## Completeness Checks
- [ ] **TESTS**: manifest TOML parse tests still pass (a `[[dependencies]]` block with a stray `name =` key still round-trips)
