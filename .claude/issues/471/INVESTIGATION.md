# #471 Investigation

## Root cause

`EnableParent::default_disabled()` returned `true` for every non-inverted XESP
with a non-zero parent. XESP semantics — per GECK + UESP — say the child is
visible iff the parent is enabled (non-inverted) or disabled (inverted). The
predicate decided gating *without* consulting the parent's actual state, and
picked the minority case (parent disabled) as the default.

Most XESP chains on vanilla FNV / FO3 / Oblivion point at persistent,
always-enabled parents: quest-flagged clutter, shop inventory, patrol markers.
Those parents have REFR flag 0x0800 *clear*, so children should render.

## Interim fix

Flipped the predicate: `self.form_id != 0 && self.inverted`. Under the
parents-assumed-enabled heuristic:

- Non-inverted XESP → parent enabled → child visible → keep
- Inverted XESP → parent enabled → child hidden → skip

Updated callsite comment in `byroredux/src/cell_loader.rs:789-801` and the
`EnableParent::default_disabled` doc comment to spell out the interim
semantic and the two-pass long-term fix.

## Tests

Two existing tests pinned the old sense — they named assertion messages
after "default-disabled" and expected `true` for the non-inverted case.
Renamed and flipped to match the new semantic; third null-parent test
passes unchanged (returns `false` in both predicates). 135/135 plugin tests.

## Scope

3 files — `crates/plugin/src/esm/cell.rs` (predicate + 2 tests +
cross-referenced doc strings), `byroredux/src/cell_loader.rs` (callsite
comment), plus the INVESTIGATION.md. Within the 5-file pipeline limit.
