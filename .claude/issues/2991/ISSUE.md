# ESM-2026-08-16-D4-02: classify_fallout_inventory_kinds unconditionally resets Mod to Misc, discarding CVPA Junk

**Issue**: #2991
**Severity**: LOW
**Dimension**: 4 — Index Completeness
**Labels**: `low,import-pipeline,bug`
**Source report**: `docs/audits/AUDIT_ESM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ESM_2026-08-16.md` (Dimension 4 — Index Completeness).

**Record / Sub-record**: `MISC`/`CVPA`, `OMOD`/`LNAM`
**Location**: `crates/plugin/src/esm/records/index.rs`:378-397

## Description

`classify_fallout_inventory_kinds` resets `ItemKind::Mod → Misc` **unconditionally** before re-promoting, discarding whether the record's own `CVPA` had classified it `Junk`.

`CVPA` is read only in `parse_misc`, which is **not re-run**, so the Junk bit is unrecoverable once a later plugin's `OMOD` override clears `LNAM`.

## Evidence

```rust
// index.rs:378-397
for item in self.items.values_mut() {
    if matches!(item.kind, super::ItemKind::Mod) {
        item.kind = super::ItemKind::Misc;      // <- Junk provenance lost here
    }
}
for &loose_item in self.object_mod_loose_items.values() {
    if loose_item == 0 { continue; }
    if let Some(item) = self.items.get_mut(&loose_item) {
        if matches!(item.kind, super::ItemKind::Misc | super::ItemKind::Junk) {
            item.kind = super::ItemKind::Mod;
        }
    }
}
```

Re-verified 2026-08-17 — the unconditional reset is present and unchanged.

## Impact

Single-plugin loads and vanilla are unaffected — measured: **620 Junk / 1,283 Mod / 0 overlap-loss on `Fallout4.esm`**.

This is multi-plugin-only, hence LOW rather than a correctness bug today. It becomes live on any load order where an `OMOD` override clears `LNAM` for an item the base plugin's `CVPA` marked Junk.

## Suggested Fix

Preserve the pre-reset kind (or the `CVPA` Junk bit specifically) so the re-promotion pass can restore it, rather than collapsing `Mod` and `Junk` into `Misc` and losing the distinction.

## Related

- ESM-2026-08-16-D4-01 (#2989) — sibling `index.rs` finding from the same dimension

## Completeness Checks
- [ ] **SIBLING**: Any other classify/reset pass in `index.rs` checked for the same lossy-reset shape
- [ ] **MULTI-PLUGIN**: The regression test uses a two-plugin override, since single-plugin loads cannot reproduce it
- [ ] **NO-REPARSE**: The fix preserves provenance rather than re-running `parse_misc`
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2991 --json state` when live state is needed.*
