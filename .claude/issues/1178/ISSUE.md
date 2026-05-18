# FO4-D4-001: SCOL StaticObject hardcodes has_script: false; parse_scol skips VMAD

**Labels**: bug, import-pipeline, low

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: ESM Architecture Records
**Severity**: LOW (forward risk for mod content; vanilla clean)

## Observation

`crates/plugin/src/esm/cell/support.rs:338-345`:

```rust
record_type: crate::record::RecordType::SCOL,
light_data: None,
addon_data: None,
// `parse_scol` doesn't currently capture VMAD
// presence — vanilla FO4 has no script-bearing
// SCOLs; revisit if mods add them.
has_script: false,
```

The parser at `crates/plugin/src/esm/records/scol.rs:121-219` does not inspect VMAD. The call-site comment acknowledges this as mod-only risk.

## Why bug

If a mod-authored SCOL carries a VMAD, the `has_script` flag on the spawned ECS entity is wrong and Papyrus event dispatch will skip it. Vanilla FO4 has no script-bearing SCOLs, so this is forward-risk only.

## Fix

Scan `sub.sub_type == b"VMAD"` once in `parse_scol`, plumb a `has_script: bool` field onto `ScolRecord`, propagate to the `StaticObject` insertion at `support.rs:344`.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm `parse_movs`, `parse_pkin`, `parse_txst` capture VMAD presence when applicable (MOVS already does per `records/movs.rs:85-131`)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic mod-style SCOL with a VMAD sub-record asserts `has_script == true` after parse

## Related

- #390 — legacy/{fo4,tes4,tes5}.rs stub removal that consolidated record parsers
