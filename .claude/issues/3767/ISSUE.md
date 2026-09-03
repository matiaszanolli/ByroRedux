# #3767 — CHAR-2026-08-30-D4-01: AfflictionTable::band_for requires bands to be sorted ascending, and nothing states, enforces or tests that

**Severity**: LOW · **Location**: `crates/core/src/character/affliction.rs`
**Source**: `docs/audits/AUDIT_CHARACTER_2026-08-30.md` (CHAR-2026-08-30-D4-01)

The issue cited `band_for`'s body as `self.bands.iter().rposition(|b| pool_value >= b.min_pool)`
— order-dependent, silently wrong on an unsorted `bands` vector.

**Investigation result**: STALE PREMISE, already fixed by an unrelated earlier commit
(`c604375f`, "Refactor material overflow handling and improve water shader functionality" — the
same commit that also independently fixed #3798's `wrapping_mul` finding). At HEAD, `band_for`
reads:

```rust
pub fn band_for(&self, pool_value: f32) -> Option<usize> {
    self.bands
        .iter()
        .enumerate()
        .filter(|(_, b)| pool_value >= b.min_pool)
        .max_by(|(_, a), (_, b)| a.min_pool.total_cmp(&b.min_pool))
        .map(|(index, _)| index)
}
```

Genuinely order-independent — selects the max `min_pool` among reached bands via `max_by`, not
positional scan order. The doc comment already states this explicitly: *"Band order is not
significant... Band order is intentionally ignored."* This is a stronger fix than the issue's
own suggested approach (enforce ascending sort via `debug_assert!`) — the algorithm doesn't
depend on sort order at all, so there's no invariant to enforce or violate.

The issue's own TESTS checklist item — "an unsorted `bands` vector must not silently return the
wrong band" — is also already satisfied: `band_for_ignores_band_order`
(`crates/core/src/character/affliction.rs`) reverses `stand_in_radiation_table()`'s bands and
asserts the correct band still resolves.

**SIBLING** (issue's own checklist item): grepped `crates/core/src/character/` for other
`position`/`rposition` calls — two exist (`PerkRanks::remove`, `FactionReputation::entry_mut`),
both exact-equality FormID lookups (`==`), not threshold (`>=`) scans, so scan order is
irrelevant to their correctness. No sibling gap found.

No code change needed; closed with citation.
