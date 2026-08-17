# SF-D5-01: Starfield ARMO MODL is a fixed-width 4-byte payload, mislabelled corrupt

**Issue**: #3056
**Severity**: MEDIUM
**Labels**: `medium,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_STARFIELD_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-16.md` (Dimension 5 — ESM + Cell Bring-up).

**Location**: `crates/plugin/src/esm/cell/support.rs`:61-74

## Description

Starfield ARMO `MODL` is a **fixed-width 4-byte payload** (a FormID reference), not a string path. The `#1620` arm treats a `MODL` holding control bytes as *"corrupt … treating as model-less"* and warns — producing **1,480 WARNs per parse across 848 forms** for data that is correctly formed for its game.

## Evidence

```rust
// crates/plugin/src/esm/cell/support.rs:61-74 (re-verified 2026-08-17)
b"MODL" => match read_mesh_path(&sub.data) {
    Ok(p) => model_path = p,
    // #1620 — a MODL holding control bytes is a non-string value
    // (FormID-shaped u32) mis-read as a path. Warn (the old path
    // was silent) and leave `model_path` empty …
    Err(bad) => log::warn!(
        "#1620 — {} {:08X}: corrupt MODL mesh path (control bytes), \
         treating as model-less: {:?}", …
```

The comment already identifies the payload correctly — *"a non-string value (FormID-shaped u32)"* — but classifies it as corruption rather than as Starfield's actual ARMO layout.

## Impact

848 legitimate Starfield ARMO forms are logged as corrupt on every parse, generating 1,480 WARNs that bury real diagnostics. More importantly, `model_path` is left empty, so those armour forms are treated as model-less rather than resolving their referenced mesh.

The mislabelling also misleads: a reader grepping for "corrupt" finds 1,480 hits that are not corruption.

## Suggested Fix

Add a Starfield arm that reads `MODL` as a 4-byte FormID and resolves it, rather than attempting a string decode. Keep the `#1620` warn for games where `MODL` genuinely is a path and the bytes genuinely are malformed.

## Related

- #1620 (the warn this narrows)
- #2996 / #2995 (the same "shared arm decodes the wrong per-game layout" class in FO4 items)

## Completeness Checks
- [ ] **PER-GAME**: A Starfield arm reads the FormID form; other games keep the string path
- [ ] **WARN-MEANS-CORRUPT**: After the fix, a "corrupt MODL" warn indicates real corruption
- [ ] **RESOLVED**: The 848 forms resolve their referenced mesh rather than going model-less
- [ ] **SIBLING**: Other fixed-width-vs-string sub-records on Starfield checked for the same assumption
- [ ] **TESTS**: A regression test parses a Starfield ARMO and asserts a resolved model reference

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3056 --json state` when live state is needed.*
