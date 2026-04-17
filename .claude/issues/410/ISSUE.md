# FO4-D6-A1: crates/plugin/src/legacy/fo4.rs is a todo!() stub with zero callers

**Issue**: #410 — https://github.com/matiaszanolli/ByroRedux/issues/410
**Labels**: bug, high, legacy-compat

---

## Finding

`crates/plugin/src/legacy/fo4.rs:13` is still `todo!("Fallout 4 ESM/ESP parser")`. Zero callers in the workspace.

Same pattern as #390 (tes4.rs), #368 (tes5.rs), and the tes3.rs stub — all four legacy bridge functions panic on call.

## Impact

- Any `LegacyLoadOrder` call on `Fallout4.esm` panics at runtime.
- DataStore / plugin-resolver have no typed entry point for FO4 content. Mod-conflict resolution, overrides, cross-plugin Form ID resolution do not work for FO4 content.
- The real parser path (`esm::parse_esm`) already handles FO4 via `EsmVariant::Modern`; the `legacy::fo4::parse` function just needs to be either wired as a shim or deleted.

## Fix

Same decision as Oblivion #390:

**Option A (wire)**: ~40-line shim that routes through `plugin::esm::parse_esm` and translates `EsmIndex` → `(PluginManifest, Vec<Record>)` via `LegacyLoadOrder::resolve`.

```rust
pub fn parse(data: &[u8], load_order: &LegacyLoadOrder) -> anyhow::Result<(PluginManifest, Vec<Record>)> {
    let index = crate::esm::parse_esm(data, EsmVariant::Modern)?;
    let manifest = PluginManifest::from_esm_index(&index);
    let records = index.records.into_iter()
        .map(|r| Record::from_esm(r, load_order))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((manifest, records))
}
```

**Option B (delete)**: Remove `legacy/{tes3,tes4,tes5,fo4}.rs` entirely. Readers confused by stubs; zero callers so no breakage.

Option A is the documented end state per #390's proposed plan; do all four legacy stubs uniformly in one PR.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Parallel decision for `tes3.rs`, `tes4.rs` (#390), `tes5.rs` (#368). Should land uniformly.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: If wiring, unit test converting a minimal parsed FO4 record → `Record` with FormIdPair resolution via LegacyLoadOrder.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 4 H4 + Dim 6 Stage A1.
