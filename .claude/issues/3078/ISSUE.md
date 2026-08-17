# SPT-D1-2026-08-16-01: a fatal parse_spt error discards a fully recoverable placeholder

**Issue**: #3078
**Severity**: MEDIUM
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md` (Dimension 1 — import dispatch).

**Location**: `byroredux/src/cell_loader/references/import.rs`:301, :329-332

## Description

A fatal `parse_spt` error **discards a fully recoverable placeholder** — the tree disappears instead of degrading.

## Evidence

```rust
// byroredux/src/cell_loader/references/import.rs (re-verified 2026-08-17)
let scene = match byroredux_spt::parse_spt(spt_data) {
    Ok(s) => { … }
    Err(e) => {
        …
        return None;
    }
};
```

The `Err` arm returns `None`, dropping the REFR entirely. But the placeholder importer downstream is **variant-agnostic** — the in-code comment at :302-306 says so explicitly (*"the placeholder importer below is variant-agnostic"*) — so it does not need a successfully parsed scene to produce a billboard.

## Impact

Any `.spt` whose TLV walk fails loses its tree entirely rather than falling back to the placeholder billboard that exists precisely for unparsed SpeedTree content.

This is the same shape as #3036 (`BSXFlags` bit 5 dropping whole NIFs): a recoverable condition treated as fatal, silently removing world content.

## Suggested Fix

On `Err`, log and fall through to the placeholder billboard rather than returning `None`. The placeholder path needs only the REFR's placement, which is already in hand.

## Related

- #3076 (SPT-D3-01), #3077 (SPT-D2-01) — the other `.spt` placeholder defects
- #3036 (FNV-D1-01) — the same recoverable-treated-as-fatal shape in NIF import

## Completeness Checks
- [ ] **DEGRADE**: A parse failure costs the geometry, never the whole REFR
- [ ] **SIBLING**: The other `.spt` dispatch site (`synth_child.rs`) checked for the same fatal arm
- [ ] **NOT-SILENT**: The fallback logs once so the parse failure stays visible
- [ ] **TESTS**: A regression test feeds a malformed `.spt` and asserts a placeholder still spawns

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3078 --json state` when live state is needed.*
