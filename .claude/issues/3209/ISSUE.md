# TD7-2026-08-20-01: WATERLINE_HYSTERESIS is declared twice across the two ends of WATAL

**Issue**: #3209 — https://github.com/matiaszanolli/ByroRedux/issues/3209
**Severity**: LOW
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD7-2026-08-20-01 (Dimension 7 — Magic Numbers & Hardcoded Constants).

**Severity**: LOW · **Effort**: trivial
**Location**: `byroredux/src/systems/water.rs:19` and `crates/physics/src/water.rs:221`. Canonical home: `crates/core/src/ecs/components/water.rs`.

## Description

Both ends of the WATAL layer declare `const WATERLINE_HYSTERESIS: f32 = 4.0;`. The physics copy documents the duplication and gives a reason:

> *"Mirrors the camera submersion system's `WATERLINE_HYSTERESIS` (`byroredux::systems::water`, #1450) — kept as a local constant because that one is private to the binary crate."*

**The reason describes the current arrangement, not a constraint.** The shared crate `byroredux-core` — which both `byroredux` and `byroredux-physics` already depend on — hosts precisely this class of constant, including one hoisted for exactly this purpose:

```rust
// crates/core/src/ecs/components/water.rs:414
pub const WEATHER_SCROLL_PER_BU_PER_S: f32 = 0.0015;
```

consumed by `crates/physics/src/water.rs:343` **and** `byroredux/src/render/water.rs:133`. `WaterFlow::SPEED_MIN` / `SPEED_MAX` / `SPEED_RAPIDS` and `WaterFlow::speed_for_kind` sit in the same file for the same reason. Nothing prevents `WATERLINE_HYSTERESIS` joining them; it simply was not moved.

## Evidence (verified at HEAD `bb0b92f2`)

```
$ grep -rn "const WATERLINE_HYSTERESIS" crates byroredux
byroredux/src/systems/water.rs:19:const WATERLINE_HYSTERESIS: f32 = 4.0;
crates/physics/src/water.rs:221:const WATERLINE_HYSTERESIS: f32 = 4.0;
```

Both are `4.0` at HEAD; **no divergence today**. The binary copy's own doc states the invariant that makes divergence a defect:

> *"the only constraint is that the vertical AABB acceptance below is extended by the **same** constant so the exit transition fires precisely at the band edge (#1450 / WAT-01)"*

— and the physics copy **is** that AABB acceptance band, in a different crate, with no mechanism tying the two together.

## Impact

Retuning one leaves the camera's `head_submerged` hysteresis and the physics body↔water containment band at **different widths**, which is the #1450 / WAT-01 flicker the constant exists to prevent, reappearing **at a crate boundary where no test spans both**. Latent, not live.

## Suggested Fix

Move it to `crates/core/src/ecs/components/water.rs` as `pub const WATERLINE_HYSTERESIS: f32 = 4.0;` next to `WEATHER_SCROLL_PER_BU_PER_S`, carrying the binary copy's fuller doc comment (the unit note and the #1450 same-constant invariant), and import it at both sites.

## Related

- **#2888** (OPEN) — *"the two ends of WATAL disagree on which overlapping water plane wins"*, the same class of cross-end split
- **#1450** / WAT-01 — the flicker this constant prevents
- The `weather_wave_adjustment` inline duplication filed alongside — the other cross-end WATAL duplication in this report

## Completeness Checks
- [ ] **SIBLING**: Both declarations replaced by the shared import; `grep -rn "const WATERLINE_HYSTERESIS" crates byroredux` returns nothing outside `crates/core`
- [ ] **DOC-CARRIED**: The #1450 same-constant invariant travels with the constant to its new home
- [ ] **TESTS**: A guard spans both ends (camera band edge and physics AABB acceptance) so a future retune of one cannot silently split them
