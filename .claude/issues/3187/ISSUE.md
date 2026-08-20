# NIFAL-D8-2026-08-20-02: RefrTextureOverlay::apply_slot_swap is a third slot table, game-agnostic, and its FO4 slot-5 arm reads a lane the FO4 TXST parser never populates

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3187
Finding: NIFAL-D8-2026-08-20-02
Labels: low,nif-parser,renderer,bug
Source: docs/audits/AUDIT_NIFAL_2026-08-20.md

Filed from `docs/audits/AUDIT_NIFAL_2026-08-20.md` (Dimension 8 — shader-flags / texture-role vocabulary). NIFAL canonical-translation finding — see `/audit-nifal`.

**Severity**: LOW
**Tier violated**: `single-boundary` (a per-game slot vocabulary re-implemented outside `slot_to_role`)
**Game Affected**: FO4, FO76, Starfield

**Location**: `byroredux/src/cell_loader/refr.rs:158-183`

## Description

`RefrTextureOverlay::apply_slot_swap` maps a raw `XTXR` NIF-slot index onto a named `esm::cell::TextureSet` field with a flat, shader-type- and **game-agnostic** match. Its doc justifies the flatness with:

> *"The source TXST has already been translated from its different TXnn ordering into named roles, so this match is intentionally NIF-role order rather than raw ESM index order."*

That premise is only **half** true: the TXST -> named-role translation is itself game-dependent. `crates/plugin/src/esm/cell/support.rs:462-471` routes `TX02` to `set.wrinkle` for `Fallout4 | Fallout76 | Starfield` and to `set.env_mask` otherwise — so on those three games `set.env_mask` is **never populated**, while `apply_slot_swap(slot_index = 5)` reads exactly `ts.env_mask`.

Meanwhile `slot_to_role((Fallout4, 5))` on the tint family resolves to `TextureRole::Wrinkle` (`crates/nif/src/import/material/slot_role.rs:301-308`) — the role that lane should have reached.

## Evidence

```rust
// crates/plugin/src/esm/cell/support.rs:462-471
b"TX02" => { if matches!(game, Fallout4 | Fallout76 | Starfield) { set.wrinkle = path; }
             else { set.env_mask = path; } }

// byroredux/src/cell_loader/refr.rs:164 + :179   (no `game` in scope at all)
5 => ts.env_mask.as_deref(),        // <- always None on FO4/FO76/Starfield
5 => &mut self.env_mask,
```

The **non-`XTXR`** path is unaffected: `merge_from_texture_set` (`byroredux/src/cell_loader/refr.rs:130`) fills `self.wrinkle` from `ts.wrinkle` directly, and `byroredux/src/cell_loader/spawn/mesh_instance.rs:172` forwards it unconditionally. Only the explicit slot-index swap form loses the binding.

## Impact

An FO4/FO76/Starfield REFR that overrides NIF slot 5 via `XTXR` is a **silent no-op** instead of a wrinkle-map swap. Narrow — `XTXR` slot-5 swaps on head meshes are the only population — and it **fails closed** (nothing wrong is bound), which is why this is LOW rather than MEDIUM.

The maintenance cost is the real one: this is a **fourth** place the slot vocabulary is written down, after `slot_to_role`, the FO4 `TX02` branch, and the `mesh_instance.rs` `pick(...)` list.

## Suggested Fix

Give `apply_slot_swap` the game/layout it is missing and route slot 5 to `self.wrinkle` when the layout is FO4-family. Better: add `pick(5, o.wrinkle, TextureRole::Wrinkle)` alongside the existing `EnvironmentMask` pick in `mesh_instance.rs` and have `apply_slot_swap` write slot 5 into both lanes, letting `slot_to_role` remain the **sole arbiter** of the slot vocabulary.

## Related

- #2695 — the two-table defect.
- The `texture_slot_layout` discriminator finding from this same sweep — same "per-game routing decided outside the shared table" root cause.
- #2999 — introduced the FO4 slot-5 -> `Wrinkle` arm without a matching overlay-side path.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the fix reduces the number of places the slot vocabulary is written down; `slot_to_role` stays the sole arbiter rather than gaining a fifth parallel copy. See `/audit-nifal`.
- [ ] **SIBLING**: the other seven slot indices audited for the same game-dependent TXST-lane mismatch, not only slot 5
- [ ] **TESTS**: an FO4 REFR with an `XTXR` slot-5 swap asserts the wrinkle lane is bound
