# #3791 — SAVE-D4-2026-08-30-03: validate_animation's clip-handle and root-entity checks stop at AnimationPlayer

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: medium, save-load, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-30.md` (Dimension 4 — Validation Gates), HEAD `64f64480`
**Finding ID**: `SAVE-D4-2026-08-30-03`

- **Severity**: MEDIUM
- **Status**: NEW
- **Data-Loss Class**: latent corruption-on-load (defense-in-depth gap)

## Location

- `crates/save/src/validate.rs:334-365` — `validate_animation`
- `crates/core/src/animation/stack.rs:17-33` + `:108-112` — `AnimationLayer` / `AnimationStack`
- `crates/core/src/ecs/components/sandbox.rs:54-104` — `Seated` / `SeatedAnimationRestore`
- `byroredux/src/save_io.rs:104-112` — the exclusion rationale that names only one hazard

The `Seated` half was created by `d2d5e067` (#3333), which added `Seated.animation_restore` as the required v9 field; the `AnimationStack` half has been latent since the column was registered but has never been enumerated by a save audit.

## Description

`validate_animation` checks exactly two things, and only on `AnimationPlayer`:

```rust
for (entity, player) in q.iter() {
    if let Some(reg) = registry.as_ref() {
        if reg.get(player.clip_handle).is_none() { … AnimationClip … }
    }
    if let Some(root) = player.root_entity {
        if root >= next_entity { … DanglingEntity … }
    }
}
```

Two other **registered, saved** columns carry the identical pair of reference classes and are inspected by none of the nine gates:

**1. `AnimationStack`** — `root_entity: Option<EntityId>` plus `layers: Vec<AnimationLayer>`, each layer holding a `clip_handle: u32` (`stack.rs:18`). Both hazards, zero checks. It is forward-latent: the only `AnimationStack::new()` anywhere in the tree sits inside `#[cfg(all(test, feature = "inspect"))]` (`stack.rs:272`), so no production path populates it. That makes it cheap to fix and cheap to ignore — but it is registered, so a future producer inherits an unguarded column.

**2. `Seated.animation_restore.clip_handle`** — this one **is** production-populated. `sandbox_seat_system` captures the pre-park `AnimationPlayer` state into it, and `clear_ambient_behavior` writes it straight back onto the live player (`npc_spawn/ai_package.rs:455-465`). It is an `AnimationClipRegistry` index — precisely the session-local handle class the allowlist rejects `AnimationClipRegistry` itself for ("numeric handles are session-local") — riding to disk inside a saved column with no gate on the way out.

### Second-order consequence

`MUTABLE_DELTA_COLUMNS`' exclusion rationale for `Seated` (`save_io.rs:104-112`) names only the `EntityId` hazard:

> `FollowState`/`EscortState`/`Seated` are deliberately NOT here — they carry `EntityId` fields (`target_entity`/`furniture`) …

`Seated.furniture` is exactly the kind of `EntityId` a FormId-keyed remap could legitimately be extended to resolve, since furniture is a placed REFR with a stable pair. A maintainer who does that work will read this comment, see the one hazard they just fixed, and **have no way to learn that v9 quietly added a second, independent session-local-handle hazard to the same struct**.

## Evidence

`validate.rs:336-365` quoted complete above — `AnimationStack` and `Seated` appear nowhere in it, and `grep -n "AnimationStack" crates/save/src/validate.rs` returns nothing. `stack.rs:108-112` and `sandbox.rs:81-87` carry the field shapes.

Production-only scan for `AnimationStack::new()` / `insert(.*AnimationStack)` across `crates/core/src`, `byroredux/src`, `crates/scripting/src` returns one hit, inside `#[cfg(all(test, feature = "inspect"))]`.

Re-verified at HEAD.

## Impact

No live loss today — neither column is overlaid, and `restore_world` (the only consumer that would replay a stale handle into a world) has no production callers.

The defect is that **the gate's coverage is asymmetric in a way nothing records**: the identically-shaped `AnimationPlayer` is checked, its two siblings are not, and one of them was extended with a new handle field two commits ago without the gate moving. That is the drift the pre-save pass exists to catch.

## Suggested Fix

Extend `validate_animation` to walk `AnimationStack` (its `root_entity` through the existing `validate_entity_reference` helper, each layer's `clip_handle` through the same registry probe) and to check `Seated.animation_restore.clip_handle` — roughly fifteen lines reusing machinery already in the file.

Separately, add the session-local-handle hazard to `Seated`'s exclusion rationale at `save_io.rs:104-112` so the comment lists **both** reasons the column stays off the overlay, not just the one that was true in 2026-08.

## Related

- #3333 (added `Seated.animation_restore` and the v9 bump)
- #1696 (the exclusion of `AnimationPlayer`/`AnimationStack` from the overlay, which names the `root_entity`/`clip_handle` hazard the gate then only half-checks)
- #1700 (the commit that widened `validate_world` past hierarchy+equipment)
- #3649 (`validate_animation`'s `AnimationPlayer -> AnimationClipRegistry` lock order — same function, different defect; check both together)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every registered saved column carrying an `EntityId` or a registry handle, against the nine gates
- [ ] **LOCK_ORDER**: `validate_animation` already takes `AnimationPlayer -> AnimationClipRegistry` (see #3649); adding two more column queries must not widen or invert that order
- [ ] **TESTS**: A regression test pins this specific fix — a snapshot with a dangling `AnimationStack.root_entity` and one with a stale `Seated.animation_restore.clip_handle` must both fail the gate
