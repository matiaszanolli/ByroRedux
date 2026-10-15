# TD1-2026-10-15-02: `crates/scripting/src/translate/effects.rs` tripled to 5295 production LOC (1933 → 5295 in three days)

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-02`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/translate/effects.rs` (10117 lines total, tests start at L5296)
- **Status**: NEW (the file was a watch-list row at 1933 on 10-12; no issue)
- **Age**: +3362 prod lines across ~82 commits since 5d31fd637, e.g. `f597efa83`, `f2d559a79`, `5dc24a272`, `e2a234c6e`, `a0045b0cf`
- **Effort**: large
- **Description**: the file is five layers in sequence:
  1. the effect IR: `Effect` enum (L1032–1577, 545 lines, ~100 variants), `FragmentGuard` (L118–267), `ValueExpr`, `ActorRef`/`ObjectRef`/`QuestRef`;
  2. guard + value classification (L331–996): `classify_value`, `classify_condition`, `guard_reading` (**234 lines**), `guard_from`;
  3. statement lowering (L1772–2800): `lower_fragment*`, `lower_statements`, `lower_if`, quest-script inlining (`inline_quest_script_call`, 151 lines), variable writes, `prune_unread_locals`;
  4. the primitive table and its **~95 `prim_*` classifiers (L2916–4980, ~2060 lines)** keyed off `EFFECT_PRIMITIVES` (L2963); and the VM-facing bridge (`VmResume`/`VmFrame`/`VmStep`/`VmStored`/`NativeBindings`, `classify_native_*`, L2812–2915);
  5. receiver / argument resolvers (L4980–5295).
  Layer 4 is a flat list of independent `fn prim_x(e: &Expr, scope: &Scope) -> Option<Effect>` grouped only by comment: quest/stage/objective, inventory/caps, faction/perk/AV, map/message, scene/activation/lock, player controls, combat/AI, animation/vehicle.
- **Evidence**: `prod_loc` 5295; `grep -n '^fn prim_' | wc -l` ≈ 95; line spans above from `grep -nE '^(pub )?(fn|enum|struct|impl)'`.
- **Impact**: a new native means one `prim_*` + one `Effect` variant + one `EFFECT_PRIMITIVES` row in a 5000-line file, then a matching applier in `fragment/effects/<family>.rs` and a VM arm (finding 01). The applier side was already split by family (#5468); the lowering side was not, so the same native lives in two differently-organised trees.
- **Related**: finding 01, 03; #5468 (appliers by family); the `every_effect_primitive_bounds_its_argument_count` test (L9679) scans this file's own text.
- **Suggested Fix**: `translate/effects/{model.rs (Effect, FragmentGuard, ValueExpr, refs), guards.rs, lower.rs, inline.rs, vm_bridge.rs, receivers.rs}` and `translate/effects/prims/{quest,inventory,actor_state,object,scene_lock,player_control,combat_ai,animation_vehicle}.rs`, one file per family named exactly like the matching `fragment/effects/` applier so a native's two halves are easy to pair. `EFFECT_PRIMITIVES` stays one table in `prims/mod.rs` (it is the ordering contract). Hazard: the `include_str!("effects.rs")` scan at L9679 splits on `"\nfn prim_"` and must read `concat!` of all `prims/*.rs` instead; `translate/mod.rs` and the `scripting/examples/` harnesses import from `translate::effects::*`, so keep a `pub use` re-export hub there.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
