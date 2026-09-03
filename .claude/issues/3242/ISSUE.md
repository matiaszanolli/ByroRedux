# #3242 — Incremental: MSWP per-shape swap loop breaks later-wins for duplicate-source entries

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_INCREMENTAL_2026-08-23.md` (F1)
**Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs`

## Fix

Verified the premise: the per-shape MSWP swap loop (added by #973)
compared each `entry.source` against `swapped` — the running output,
reassigned on every match — instead of against `current`, the shape's
fixed original material path. Applied the issue's own suggested fix
exactly, mirroring the sibling reference implementation
(`refr.rs::build_refr_texture_overlay`, unchanged, already correct):
compare against `current` (never mutated), assign only to `swapped` as
the output.

This fixes both failure modes the issue describes:
1. **Duplicate source, later-wins broken** — two entries sharing the
   same `source` used to only ever fire the first match (comparing
   against `swapped`, already changed by the first hit, so the second
   entry's `source == original` check failed). Now the last matching
   entry wins, matching the documented MSWP later-wins semantics.
2. **Incidental chaining** — an entry whose `target` happened to equal a
   later entry's `source` used to silently chain (A→B→C), a behavior
   nothing in the format or surrounding comments describes or intends.
   Now only entries whose `source` matches the shape's actual authored
   material can ever fire.

## SIBLING (issue's own checklist item — "loop pattern matches
`refr.rs:388-401` exactly")

Searched for every MSWP-swap-application site in the codebase — exactly
two exist: `refr.rs`'s `build_refr_texture_overlay` (the original,
already-correct REFR-level implementation) and this per-shape loop. No
other site needed the same fix; the two now match exactly (compare
against the fixed original, assign to the output only).

## TESTS (issue's own checklist item — "a test with two `material_swaps`
entries sharing the same `source`, asserting the *last* one wins")

- `mswp_duplicate_source_entries_resolve_to_the_last_one` — the issue's
  exact scenario: two entries with the same `source`, asserts the
  shape's material resolves to the LAST entry's `target`.
- `mswp_incidental_target_source_collision_does_not_chain` — the
  companion failure mode: an entry's `target` equals a later entry's
  `source`; asserts the chain never fires because the second entry's
  `source` never matches the shape's own original authored material.

**Reintroduce-and-revert verification**: temporarily restored the
`swapped`-vs-itself comparison — confirmed both new tests failed with
exactly the wrong values the issue describes (first-wins instead of
last-wins; an unintended chained target instead of the correct
single-hop swap). Restored the fix and reran — all 4 `mswp_*` tests in
`cell_loader::spawn::mesh_instance::tests` pass again.

## Verification

- `cargo check -p byroredux --tests`: clean, zero warnings.
- `cargo test -q -p byroredux --bin byroredux mswp_`: 4 passing, 0
  failing (+2 new).
- `cargo test -q --no-fail-fast` (full workspace): **7175 passing, 0
  failing**.
