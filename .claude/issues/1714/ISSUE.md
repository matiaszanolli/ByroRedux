# SAVE-D2-01: schema fingerprint is coarse by design — guard against a save-participating type masking an intra-type change

Labels: bug medium tech-debt 

- **Severity**: MEDIUM
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: none today (latent across versions)
- **Location**: `crates/save/src/registry.rs:234-249` (`schema_fingerprint`); decode minor-advisory note `crates/save/src/snapshot.rs:113-114`

## Description
`schema_fingerprint` is FNV-1a over kind-tagged, ordered column **keys** only — it catches add/remove/rename of a *type*, not a *field* change within a type (correctly documented). The intended backstop for intra-type change is `serde_json::from_value` **failing** at load. That backstop only fires if the new field is required. Combined with `decode`'s advisory `minor` (a newer minor still loads, serde default-fills), a future field added with `#[serde(default)]` or as `Option` would load an OLD save silently default-filled — masking the change rather than rejecting it.

## Evidence
Current save-participating structs grepped: none carries `#[serde(default)]` today, so the trap is not yet sprung. `decode` does not reject on minor skew. FNV constants verified canonical (offset `0xcbf29ce484222325`, prime `0x100000001b3`); the hash depends only on names+order, no `TypeId`/address — confirmed stable across runs/builds.

## Impact
No current data loss; a forward-compat hazard. The moment a `#[serde(default)]`/`Option` is added to a saved struct without a major bump, old saves load silently downgraded.

## Suggested Fix
Add a guard test (or doc rule) forbidding `#[serde(default)]`/new `Option` on save-participating structs without a `FORMAT_MAJOR` bump, until a versioned migrator chain exists. Optionally extend the fingerprint to hash a per-type schema version.

## Completeness Checks
- [ ] **TESTS**: A guard test fails if a save-participating struct gains `#[serde(default)]`/new `Option` without a format-major bump
