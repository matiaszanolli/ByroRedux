# #4045 — REN-2026-09-06-D7-01: two premises in the Dimension 7 checklist name real symbols with the wrong meanings

**Labels**: low, nifal, renderer, tech-debt, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D7-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 7,
  bullets 1 and 2), against `crates/renderer/src/vulkan/scene_buffer/upload.rs`
  (`upload_materials`) and `crates/renderer/src/vulkan/material.rs`
  (`GpuMaterial::as_bytes`, `hash_gpu_material_fields`)
- **Status**: NEW
- **Description**: Both are checkable-and-wrong, and both name a live symbol —
  which is what makes them worse than vague prose: an auditor can look the
  symbol up, find it, and conclude the bullet is verified.

  1. *"Per-frame SSBO sized to `min(intern_count, MAX_MATERIALS)`."*
     `MaterialTable::interned_count()` is a real accessor and it is the
     **wrong quantity** — its own doc reads "Total `intern()` calls so far
     this frame (hits + misses)", i.e. the *denominator of the dedup ratio*,
     which on a real cell is one to two orders of magnitude larger than the
     unique count. The SSBO is sized by the unique count.
  2. *"Hash/Eq treat `GpuMaterial` as raw bytes."* `GpuMaterial` has **no
     `Hash` impl** — derived or manual — and the dedup key has not been the
     byte string since #781. Only `PartialEq`/`Eq` are byte-level.
- **Evidence**: (1) `upload_materials` computes
  `let count = materials.len().min(MAX_MATERIALS);` behind a release
  `assert!(materials.len() <= MAX_MATERIALS, …)`, and dirty-gates the copy on
  `hash_material_slice(&materials[..count])`. `interned_count` appears
  nowhere in `scene_buffer/`. (2) `GpuMaterial::as_bytes`'s own doc states it:
  "`GpuMaterial` has no `Hash` impl; dedup is keyed on the field-walking
  `hash_gpu_material_fields` instead (#781 moved the index key off the struct
  itself)." `rg 'impl.*Hash for GpuMaterial|derive\(.*Hash'` over
  `material.rs` → no match.

  For the record, since the bullet's *intent* is the real invariant and it was
  checked properly: a field-name diff shows 108 declared `GpuMaterial` fields
  and 108 walked by `hash_gpu_material_fields`, symmetric difference empty;
  `DrawCommand::material_hash` reaches the same 108 through 97 explicit
  `write_*` calls plus a `supplemental_texture_indices[..12]` loop, with slots
  12–15 (`GLASS_ROUGHNESS_SCRATCH`, `GLASS_DIRT_OVERLAY`, `LIGHTING_MASK`,
  `BACK_LIGHTING`) written individually where their `GpuMaterial` fields sit;
  and `cargo test -p byroredux-renderer --lib
  material_hash_matches_gpu_material_field_hash` → **1 passed**. The
  invariant holds; only its description does not.
- **Impact**: (2) is the more consequential. It sends an auditor of the dedup
  key to look for a `Hash` impl and a padding-zeroing invariant that no longer
  exist, instead of at `hash_gpu_material_fields` — the hand-maintained
  108-field walk that is the *actual* single point of failure, and the one
  place a newly added `GpuMaterial` field can be silently omitted (the struct
  literal in `to_gpu_material` is exhaustive; the hash walk is not). The
  bullet's parenthetical "(depends on the Dim-3 scalar-fields + zeroed-pad
  invariant)" compounds it: `GpuMaterial` has zero pad fields today — all 108
  members are live named scalars (108 × 4 == 432 == `size_of`). (1) is
  milder but points an auditor at the wrong accessor when checking the one
  cap whose overflow path is live and counted.
- **Related**: #3846 (open — the sibling stale `GpuMaterial` size claim in
  `include/bindings.glsl`), #797 / #807 (the over-cap route), #781 (the move
  off the struct hash), #1368, and `AUDIT_SAFETY_2026-08-30.md`'s finding on
  the same "byte-level Hash / zeroed pads" claim in three `unsafe`-adjacent
  doc comments — this is the audit-skill copy of that same stale statement.
- **Suggested Fix**: Reword bullet 1 to `min(table.len(), MAX_MATERIALS)` (or
  simply "the unique-material count"), and bullet 2 to: "`Eq` is byte-level
  (`GpuMaterial::as_bytes`); the dedup **key** is the field-walking
  `hash_gpu_material_fields`, and `DrawCommand::material_hash` must stay in
  lockstep with it — pinned by
  `material_hash_matches_gpu_material_field_hash`." Naming the pin is the part
  that makes the bullet self-checking.

---

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
