# OBL-D3-C1: legacy/tes4.rs is a dead todo!() stub — either wire or delete

**Issue**: #390 — https://github.com/matiaszanolli/ByroRedux/issues/390
**Labels**: bug, critical, legacy-compat

---

## Finding

`crates/plugin/src/legacy/tes4.rs:9-14` is a literal panic stub:

```rust
pub fn parse(_data: &[u8], _load_order: &LegacyLoadOrder) -> anyhow::Result<(PluginManifest, Vec<Record>)> {
    todo!("Oblivion ESM/ESP parser")
}
```

`legacy/mod.rs` declares the module (`pub mod tes4;`) but no caller in the workspace invokes `tes4::parse`. The working ESM pipeline goes through `esm::parse_esm` / `esm::cell::parse_esm_cells` and produces raw `EsmIndex` structs, which never land in `DataStore`. **Same pattern in `tes3.rs`, `tes5.rs`, `fo4.rs` — all `todo!()`.**

## Impact

- Any future code path taking the documented "legacy → stable Record" route panics at runtime.
- Mod conflict resolution, overrides, cross-plugin Form ID resolution do not work for any legacy game content today.
- The `Record` / `PluginManifest` plumbing exists and is tested, but nothing feeds it from legacy parsers.

## Fix (~40 lines)

Either:
1. **Delete** `legacy/{tes3,tes4,tes5,fo4}.rs` and the module declarations. The stubs mislead readers.
2. **Wire** `tes4::parse` as a thin adapter over `esm::parse_esm` that converts each record into `Record` form bundles keyed by `FormIdPair` via `LegacyLoadOrder::resolve`.

Option 2 is the intended end state; option 1 reduces surface area until the plugin system needs it.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Parallel decision for `tes3.rs`, `tes5.rs`, `fo4.rs` (same shape).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: If wiring, unit test converting a minimal parsed TES4 record into a `Record` with FormIdPair resolution.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 3 C1.
