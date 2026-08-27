# FNV-2026-08-26-D6-06

**Issue**: #3345
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 6 — Animation, Skinning & Particles
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/anim_convert.rs:490-501` (`convert_nif_clip`)

**Premise verified**: `crates/nif/src/anim/types.rs:206` declares `phase`,
`sequence.rs:188` populates it from `seq.phase`, `entry.rs:345` populates it
from the embedded controller envelope — and `convert_nif_clip`'s
`AnimationClip { … }` literal copies `name/duration/cycle_type/frequency/
weight/accum_root_name/channels/…` but not `phase`. `byroredux_core`'s
`AnimationClip` has no `phase` field at all, and a repo-wide grep finds no
animation-phase consumer. Gamebryo's `NiTimeController` applies phase as a
scaled-time offset (`fFrequency * t + fPhase`), so the field is load-bearing in
principle.

**Evidence** — measured across **all** vanilla FNV + 5 DLC mesh archives
(20 677 NIFs, 4 989 KFs):
```
Fallout - Meshes:      embedded clips=681 nonzero_phase=0 freq!=1=0
                       kf clips=4296     nonzero_phase=0 freq!=1=0
DeadMoney/HonestHearts/OldWorldBlues/LonesomeRoad/GunRunnersArsenal — Main:
                       nonzero_phase=0 freq!=1=0 in every archive
```

**Impact**: **none on vanilla FNV** — no shipped FNV clip has a non-zero phase.
Reported as a live dead field because #3097's fix stops one boundary short and
will silently no-op for the Skyrim/FO4 content where phase does appear; it is
explicitly *not* an FNV rendering defect.

**Fix sketch**: add `phase: f32` to `byroredux_core::animation::AnimationClip`,
copy it in `convert_nif_clip`, and seed `AnimationPlayer.local_time` /
`AnimationLayer` start time with `clip.phase` on attach. (Two of the three
call-sites already exist; the field is a one-line add each.)

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
