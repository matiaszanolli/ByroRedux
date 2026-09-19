# FO3-D2-2026-09-19-01: FO3-D2-2026-09-19-01: #4261's per-system controller scoping drops most authored emitter birth rates — #3754's per-game rate gate is RED on main

- **Labels**: high,bug,nif-parser,nif,game:fo3,legacy-compat
- **Filed from**: docs/audits/AUDIT_FO3_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4467

---

**Dimension**: 2 — NIF Parser FO3 Block Subset (typed-emitter slice; NIF-side controller-chain lookup)
**Filed from**: `docs/audits/AUDIT_FO3_2026-09-19.md` (/audit-fo3, HEAD `340799d66`)

**Description**

`b3237e65a` (Fix #4261, 2026-09-12) re-scoped `extract_emitter_rate` from a whole-scene first-match for `NiPSysEmitterCtlr` to a walk of the `NiParticleSystem`'s own `controller_ref` chain (`anim::walk_controller_chain`). The walk only advances through controller types `time_controller_base_of` recognizes. On real Bethesda content the emitter ctlr usually sits at **hop ≥ 2** of the system's own chain, behind sibling `NiPSys*` controllers (`NiPSysModifierActiveCtlr`, `BSPSysMultiTargetEmitterCtlr`, `NiPSysEmitterSpeedCtlr`, …) that the parser collapses into the generic `NiPSysBlock` marker (`crates/nif/src/blocks/particle.rs:164-176` — retains only `original_type` + budget, discards `NiTimeControllerBase.next_controller_ref`). At such a head the walk's advance yields NULL and stops at hop 1; the emitter ctlr two links down is unreachable, `extract_emitter_rate` returns `None`, and the emitter falls back to the name-heuristic preset rate.

**Location**: `crates/nif/src/import/walk/emitter.rs:390-405` (`find_own_emitter_ctlr_interpolator`), `:414`, `:600-606`. Failing gate: `crates/nif/tests/parse_real_nifs.rs:926` (`real_archive_torch_meshes_surface_particle_emitters`, `--ignored` lane).

**Evidence** (all real-data, 2026-09-19, FO3 GOTY install):

- Gate at HEAD `340799d66`: FNV 98/194 (50.5%), **FO3 102/144 (70.8%)**, OB 210/272 (77.2%), **SSE below the 50% floor — test FAILS** (dimension agent `--nocapture`: 39/124; orchestrator's independent re-run panic: `Skyrim SE: only 63/153 emitters decoded a finite positive birth rate (floor 50%)`).
- Gate at `b3237e65a~1` = `21dcbe58d` (built out-of-tree via `git archive`): FNV 343/343, FO3 149/158 (94.3%), OB 272/272, SSE 182/182 — **passes**. Bisect: only `b3237e65a` (#4261) and `9e372f452` (#4240, forwarding-only) touched this path in between.
- Attachment census (Fallout - Meshes.bsa): 361 `NiPSysEmitterCtlr` blocks, **100% have `base.target_ref` → a `NiParticleSystem`**; 422 systems, only **230/422 have the ctlr at hop 1**. The 192 missed systems' chain heads: `NiPSysModifierActiveCtlr` 114, `BSPSysMultiTargetEmitterCtlr` 45, `NiPSysEmitterSpeedCtlr` 20, others ≤ 6. Same shape on SSE (139/531 systems reachable).

**Impact**

FO3 smoke/fire/dust emitters render at heuristic rather than authored densities (FO3 −23.5 coverage points; FNV −49.5; SSE −68.5 — the defect is cross-game, measured via the FO3 audit). Separately, the red gate poisons the `--ignored` real-data lane: every future run fails on SSE until fixed, masking any NEW regression the floor would catch. The #4261 session ran only the default test lane — the second emitter-chain regression that the real-data lane alone catches.

**Related**: #3754 (rate-curve mean + per-game floor — fix intact, gate doing its job), #4261 (introducing commit), #3327 (`time_controller_base_of` stop-dead class), #1402, #3286.

**Suggested Fix** — two complementary, per-instance-safe options:
1. Correct-by-construction: retain `NiTimeControllerBase` on `NiPSysBlock` (or give the sibling `*Ctlr` types that head chains real structs) and add the arms to `time_controller_base_of`, so the existing walk traverses hop ≥ 2 (the #3327 pattern).
2. Fallback attribution: when the own-chain walk misses, resolve the scene's `NiPSysEmitterCtlr` blocks by `base.target_ref == this system's index` (census: 100% target the system — per-instance-exact, preserving #4261's goal, restoring pre-#4261 coverage).

Either way: the #4261 fix-class must run the `--ignored` real-data lane before landing.

## Completeness Checks
- [ ] **SIBLING**: The same hop-≥2 stop-dead shape audited for the other sibling controller types that head chains (`NiVisController`, `NiPSysGravityStrengthCtlr`)
- [ ] **CANONICAL-BOUNDARY**: The fix stays in the NIF walk (`crates/nif/src/import/walk/emitter.rs`) — per-game logic must not appear; `extract_emitter_rate` remains a data extractor feeding `apply_emitter_params`
- [ ] **TESTS**: The `--ignored` real-data gate passes on all four games again, and a default-lane unit test pins the hop-≥2 chain shape (a `NiPSysBlock` sibling head with the emitter ctlr behind it)
