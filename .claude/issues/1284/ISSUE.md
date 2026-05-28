## Symptom

Loading FNV `FreesideAtomicWrangler` (Atomic Wrangler casino) exposes a real-content overflow of the `SkinSlotPool`:

```
[WARN  byroredux_core::ecs::resources]
  SkinSlotPool exhausted at capacity 226 (slot 0 reserved).
  Excess skinned entities silently fall back to bind pose.
  Bump MAX_TOTAL_BONES or implement variable-stride packing.

[WARN  byroredux_renderer::vulkan::acceleration::tlas]
  TLAS: 2631 instances from 2922 draw commands
  (34 lack BLAS — skinned=34, rigid=0, ssbo_evicted=0 — no RT shadows for those meshes)
```

That's 226 pool slots in use + 34 overflow = **260 distinct skinned entities** at the peak (Garret twins, dealers, security, escorts, patrons, prostitutes — the densest NPC interior in FNV).

The 34 spilled NPCs:
- Render in **bind pose** (Vitruvian-man T-pose) instead of their actual animated pose.
- Drop out of the **RT shadow** path (no BLAS entry).
- Raster is unaffected — they're still visible as posed meshes, just statically.

Reproducible on 2026-05-28 release build (commit bf386879).

## Root cause

`crates/core/src/ecs/resources.rs:619-625` and `byroredux/src/main.rs:867-879`:

```rust
// Pool capacity = (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1
//                = (32768 / 144) - 1
//                = 226
SkinSlotPool::new(((MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1) as u32)
```

Bones-per-slot is a **fixed stride**: every entity reserves 144 bone-mat4 slots in the persistent `bind_inverses` SSBO whether it uses 80 (typical biped) or 144 (NIF skeleton ceiling). Typical FNV humanoid skeleton is ~85 bones, so ~40 % of every pool slot is currently slack.

History: `MAX_TOTAL_BONES` was already bumped from 4096 → 32768 once (pre-M41.0 it caused "FNV Prospector rendered the first ~4 actors then dropped the rest"; see comment block in `scene_buffer/constants.rs:18-30`). Same shape of bug — Atomic Wrangler is the next pressure test point.

## Fix options (ordered cheap-to-correct)

### Option A — Bump `MAX_TOTAL_BONES`
Smallest patch. `32768 → 49152` raises pool cap from 226 → 340. SSBO grows `2 MB → 3 MB` — trivial on the 6 GB-VRAM minimum target.

Pros: one-line constant change.
Cons: doesn't address the underlying 40 %-slack waste; the next densest cell (Skyrim's Bee & Barb? Crimson Caravan? FO4 Diamond City Market?) pushes the cap again.

### Option B — Variable-stride packing (M29.5 — the proper fix per the existing TODO)
Pack `bind_inverses` by actual bone count, not by `MAX_BONES_PER_MESH`. Per-entity offset table replaces the slot ID. ~80-bone humanoids would pack 32768/80 = 409 entities in the current SSBO budget — Atomic Wrangler fits with 30 % headroom.

Pros: structural fix; raises cap by ~80 % without growing VRAM.
Cons: invasive — touches `SkinSlotPool`, `bind_inverses` upload, every shader sampling the palette, the per-frame skin-compute dispatch. The existing M29.5 milestone budget.

### Option C — LRU eviction of off-screen skinned entities
Skinned entities outside the camera frustum (or beyond some distance threshold) release their slot to the free list. Re-allocates on re-entry.

Pros: bounds the live-cap to the visible-NPC count, not the cell-NPC count.
Cons: one-frame bind-pose pop on re-entry; complicates pose-hash dirty tracking (`#1195 / PERF-DIM7-01`).

## Recommendation

**Land Option A as the immediate hotfix** (constant bump, ships with `audit-runtime` baseline regen), then schedule Option B as the proper M29.5 closeout. The 34-NPC visible-symptom is observable on screen as a row of T-posed gamblers in the back of the casino — not a hypothetical concern.

## Acceptance criteria

- [ ] Loading FNV `FreesideAtomicWrangler` produces zero `SkinSlotPool exhausted` warnings.
- [ ] `tlas` warning shows `skinned=0` lack-BLAS (down from 34).
- [ ] `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` regenerated with the new draw-call / instance counts.
- [ ] Existing `bone_palette_overflow_tests.rs` updated to the new ceiling (currently parameterised on `MAX_TOTAL_BONES`, should follow the constant automatically).

## Refs

- Surfaced under #1283's `audit-runtime` skill (first real-content baseline capture).
- Related: M29.5 (GPU skinning — variable-stride packing roadmap item per project notes).
- Constant docs: `crates/renderer/src/vulkan/scene_buffer/constants.rs:18-54`.
- Construction site: `byroredux/src/main.rs:867-879`.
- Pool definition: `crates/core/src/ecs/resources.rs:619-722`.
