# #758: SF-D3-05: BGEM vs BGSM dispatch in `merge_bgsm_into_mesh` keys on extension only, not file magic

URL: https://github.com/matiaszanolli/ByroRedux/issues/758
Labels: bug, import-pipeline, medium

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 3, SF-D3-05)
**Severity**: MEDIUM (low risk in vanilla; mod-content footgun)
**Status**: NEW

## Description

`byroredux/src/asset_provider.rs:435` (`merge_bgsm_into_mesh`) dispatches purely on `path.ends_with(".bgsm")` / `.bgem`. The actual file magic isn't checked here (it's checked inside `parse()` in `crates/bgsm/src/lib.rs:111-125`). If a mod ships a `.bgsm`-named file with BGEM magic (or vice versa) the wrong override semantics apply silently.

## Impact

Bethesda's tooling enforces the suffix in vanilla, so the risk is in user mods. Wrong dispatch means BGEM single-file semantics get applied to a BGSM template chain (or vice versa) — silently, with no warn.

## Suggested Fix

Add a 4-byte magic sanity check before dispatching. The bgsm crate already reads magic at parse time; expose a cheap pre-flight fn:

```rust
// crates/bgsm/src/lib.rs
pub fn detect_kind(bytes: &[u8]) -> Option<MaterialKind> {
    match &bytes.get(..4)? {
        b"BGSM" => Some(MaterialKind::Bgsm),
        b"BGEM" => Some(MaterialKind::Bgem),
        _ => None,
    }
}
```

Call from `asset_provider.rs:435` before extension dispatch; warn if extension and magic disagree.

## Completeness Checks

- [ ] **TESTS**: Test with a forged `.bgsm`-named buffer carrying BGEM magic; assert warning fires AND correct dispatch occurs.
- [ ] **SIBLING**: Verify `MaterialProvider` chain doesn't have a separate dispatch site keyed on extension.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.
