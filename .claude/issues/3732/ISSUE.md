# #3732: NIFAL-2026-08-30-D8-01: #3458's slot-2 colocation never reached the REFR-overlay sibling — its pick closure structurally cannot express a colocated role

**Labels**: bug, medium, game:skyrim, game:starfield, nifal, esm-plugin
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: 8 (Shader-flags / Effects) · **Tier violated**: single-boundary
**Game affected**: Skyrim, Starfield (the `TextureSlotLayout` arms `slot_to_colocated_role` covers)

Severity note from the report: the *live* mis-render on vanilla content is near-nil; the **structural inability to propagate** is what earns the MEDIUM rating.

## Location
- `byroredux/src/cell_loader/spawn/mesh_instance.rs` — the `pick` closure and the `lighting_mask` line it gates
- Contrast: `crates/nif/src/import/material/dedicated_shader.rs` (the import-side half that *does* consult both functions)

## Description
#3458 (fixed 2026-08-28, `d5a8c36c`) established that Skyrim's slot 2 is genuinely **two roles at once** on the tint family — the `*_sk.dds` is both the `Tint` map and the `LightingMask` the `SLSF2_Soft_Lighting` gate asserts exists — and introduced `slot_to_colocated_role` to return the second role. The NIF import loop consults both functions (first-wins).

The REFR texture-overlay path did not get the same treatment. `resolve_mesh_paths` routes every override through one closure:

```rust
let pick = |slot: u32, raw: Option<FixedString>, role: TextureRole| {
    raw.filter(|_| slot_to_role(slot_context, slot) == Some(role))
};
```

`pick` consults **only** `slot_to_role`. On the tint family `slot_to_role(ctx, 2)` returns `Tint`, so the `lighting_mask` line — `pick(2, o.glow, TextureRole::LightingMask)` — can never match, and the override is silently dropped for exactly the population #3458 was about. For non-tint meshes `slot_to_role` does return `LightingMask`, so that arm works; the hole is tint-family-only.

## Evidence
`slot_to_colocated_role` (`crates/nif/src/import/material/slot_role.rs`) is referenced at exactly one non-test site, `dedicated_shader.rs`. Re-verified 2026-08-30: `grep -rn "slot_to_colocated_role" byroredux/src` returns **nothing**.

## Impact
A REFR whose TXST overrides slot 2 on a Skyrim/Starfield tint-family mesh with `soft_lighting`/`rim_lighting` set updates `tint` but leaves `lighting_mask` bound to the **base mesh's** original slot-2 texture, while `MAT_FLAG_SOFT_LIGHTING` crosses regardless — a half-overridden pair.

**Vanilla reachability is very low and the report is explicit about that**: FaceGen heads reach the engine through `npc_spawn`, not through REFR placement, so the reachable set is REFR-placed statics that use shader type 4/5/6 *and* carry a TXST override *and* set a soft/rim gate. No evidence that population is non-empty in vanilla.

The durable defect is **structural, not statistical**: `pick`'s signature cannot express a colocated role at all, so **any** future entry added to `slot_to_colocated_role` will silently fail to reach the overlay path the same way, with no compile error and no test.

## Related
#3458 (the import-side half, fixed), #3187 (`apply_slot_swap`, the *third* slot table on this same overlay path — still open).

## Suggested Fix
Change `pick` to `slot_to_role(slot_context, slot) == Some(role) || slot_to_colocated_role(slot_context, slot) == Some(role)`. One line, and it makes the overlay path track the slot table's colocation model automatically.

## Completeness Checks
- [ ] **SIBLING**: #3187's `apply_slot_swap` is the third slot table on this same overlay path — check it in the same pass
- [ ] **CANONICAL-BOUNDARY**: the slot→role vocabulary stays in `slot_role.rs`; the overlay path consults it, never re-derives it. See `/audit-nifal`.
- [ ] **TESTS**: a regression test that a colocated role reaches the overlay path — the current gap has no failing test
