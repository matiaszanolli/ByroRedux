# #2193: OBL-2026-07-25-01: is_grounded stays false at Oblivion interior spawn (ICMarketDistrictTheGildedCarafe) — jump unconditionally broken

**Source audit**: `docs/audits/AUDIT_OBLIVION_2026-07-25.md`, finding `OBL-2026-07-25-01`
**Labels**: `high`, `nif-parser`, `legacy-compat`, `bug`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2193

**Severity**: HIGH
**Dimension**: Exterior Blocker Chain & Game-Specific Quirks (Oblivion compat audit)
**Location**: `crates/nif/src/import/collision/shape.rs:354` (`resolve_tri_strips_data_refs`, the suspected root per the closing investigation), consumed at `byroredux/src/systems/character.rs:195,219,335` (`c.is_grounded` gates `jump_fired` and `desired_vertical`), `crates/physics/src/components.rs:107`

## Background

This is the follow-up that should have been filed when #2013 closed.

`#2013` ("TES-family player rig never grounds at cell-load spawn — infinite freefall") was fixed in `e2f75456` (2026-07-18) by adding a capsule-shaped ground probe (`PhysicsWorld::cast_capsule_down`) to the door-spawn nudge, resolving infinite freefall on both Skyrim SE and Oblivion. The issue was then closed.

But the closing comment on #2013 documented a **second, distinct** bug found while verifying that fix live against real Oblivion data (`ICMarketDistrictTheGildedCarafe`, real Vulkan device + Steam install, headless bench with `RUST_LOG=info`):

> Partially resolved / follow-up needed: Oblivion's spawn is now stable from frame 0 (previously fell continuously past the floor — the reported "infinite freefall" symptom is gone), but `is_grounded` itself still reads `false` there. A one-off diagnostic probe found the resting contact's surface normal points down (dot-up ≈ -0.99) instead of up — the collision triangle at that exact spot appears to be inverted, independent of spawn accuracy. I didn't isolate the root cause (likely somewhere in the NiTriStrips-based Oblivion collision import path, `resolve_tri_strips_data_refs` in crates/nif/src/import/collision/shape.rs) and didn't want to guess at a mesh-winding fix that could regress correctly-oriented floors elsewhere. Recommend filing this as a distinct follow-up issue — it's a different bug (inverted collision normal) than the spawn-positioning bug this issue tracked, and closing this one since its reported symptom (infinite freefall on both TES-family cells) is resolved.

That recommendation was never carried out. The bug has sat untracked as a paragraph in `ROADMAP.md`'s Known Issues (line ~701) ever since. This finding (`OBL-2026-07-25-01`, from the 2026-07-25 Oblivion compat audit) files the issue #2013's own closing comment asked for.

## Description

At `ICMarketDistrictTheGildedCarafe`, the player character no longer falls through the floor (the #2013 fix holds), but `is_grounded` never flips `true` while resting on solid architecture. A diagnostic probe run during the #2013 verification attributed this to the resting contact's surface normal reading inverted (`dot(normal, +Y) ≈ -0.99` instead of ≈ +1), consistent with a wrong-winding collision triangle — suspected to originate in the NiTriStrips-based Oblivion collision import path (`resolve_tri_strips_data_refs` / `merge_tri_strips_shape`, `crates/nif/src/import/collision/shape.rs:340-`), not yet isolated to a specific mesh or line.

`is_grounded` is not cosmetic state — it gates real player control:

```rust
// byroredux/src/systems/character.rs:195
let jump_fired = want_jump_now && controller.is_grounded && !controller.wants_jump;
```
```rust
// byroredux/src/systems/character.rs:219
let desired_vertical = if controller.is_grounded && !jump_fired {
    -controller.step_height
} else {
    vertical_velocity * dt
};
```

With `is_grounded` stuck `false`, `jump_fired` can never be `true` (jump is unconditionally unavailable), and the vertical-velocity resolution path takes the non-grounded (full gravity-integration) branch every frame despite the character resting on solid ground — the opposite of the Bethesda-engine convention (gravity suppressed while grounded, comment at `character.rs:210-218`) that the fix for #2013 itself relies on.

## Evidence

- `gh issue view 2013 --comments` — full investigation trail, final comment (quoted above) documents the inverted-normal finding and explicitly recommends a separate issue.
- `crates/physics/src/components.rs:107` — `is_grounded: bool`, doc comment: "Set by `KCC.move_shape`'s `EffectiveCharacterMovement.grounded` every frame."
- `gh issue list --repo matiaszanolli/ByroRedux --state all --search "grounded"` (run fresh this session) returns only `#2013` (closed, this bug's source) and `#1832` (closed, unrelated earlier zero-mass-Dynamic reclassification) — no distinct tracking issue exists for the inverted-normal residual.
- The shared FO3/FNV collision import path (also going through `extract_from_classic`, per #1832's history) grounds correctly on equivalent cells — this narrows the defect to Oblivion-specific geometry/import handling, not the shared grounding/KCC logic.

## Impact

On any Oblivion interior spawn reproducing the same floor geometry class as `ICMarketDistrictTheGildedCarafe` (verified live with a real Vulkan device + Oblivion game data), the player character cannot jump and permanently takes the non-grounded vertical-integration branch despite resting on solid ground — a correctness gap in core player control, not merely cosmetic, and specific to Oblivion content. Graded HIGH: fails under realistic conditions (real, played interior cell), no workaround, and is not a rare/gated failure mode within the affected cell(s).

## Related

- `#2013` (closed) — spawn-positioning "infinite freefall" symptom fixed by `e2f75456`; its closing comment is the direct source of this finding and explicitly asked for this follow-up to be filed.
- `#1832` (closed) — zero-mass-Dynamic→Static reclassification, a prerequisite fix sharing the same `extract_from_classic` path.

## Suggested Fix

Isolate the inverted-normal hypothesis in the Oblivion NiTriStrips collision-import path (`resolve_tri_strips_data_refs` / `merge_tri_strips_shape`, `crates/nif/src/import/collision/shape.rs:340-`) using a live Vulkan device + real Oblivion game data (`ICMarketDistrictTheGildedCarafe`, same tooling that closed #2013): confirm which mesh/triangle contributes the resting contact, check winding order against the shared FO3/FNV path (which grounds correctly) to find the Oblivion-specific divergence, and fix the winding/normal derivation without regressing correctly-oriented floors elsewhere.

## Completeness Checks
- [ ] **SIBLING**: Check winding-order handling in the shared FO3/FNV NiTriStrips collision path (which grounds correctly) to isolate what's Oblivion-specific vs. shared, so the fix doesn't just move the bug
- [ ] **TESTS**: A regression test pins the correct collision-triangle winding/normal for this NiTriStrips-derived shape (unit test on the parsed mesh data, since the symptom itself needs a live Vulkan device + real game data to observe)
