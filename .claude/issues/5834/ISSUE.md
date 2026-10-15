# TD3-2026-10-15-10: CLAUDE.md "Workspace Structure" still omits 16 of 29 crates and 3 of 4 tools, and describes `components.rs` as the marker-component home (regression of #5602, CLAUDE.md half)

Labels: low,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-10`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `CLAUDE.md` Workspace Structure block (crates list; `src/components.rs` row at line ~ 9 of the block)
- **Status**: Regression of #5602 (CLOSED; fix commit `65e6562fc` touched only ROADMAP.md although the title names CLAUDE.md)
- **Age**: crates added since the block was written; `6ef8976f2` (2026-10-14) split `components.rs` into `components/{render,terrain,environment,indices,input,audio,tuning,ai_anim,navmesh}.rs`
- **Effort**: small
- **Description**: the tree lists core, plugin, renderer, bsa, platform, nif, ui, scripting, menuxml, papyrus, cxx-bridge, debug-protocol, debug-server (13) plus `tools/byro-dbg`. Absent: audio, bgsm, boot-request, debug-ui, facegen, fsr3-sys, game-detect, hkx, mod-runtime, pex, physics, save, sdk, settings-io, sfmaterial, spt, and tools byro-detect, byro-launcher, texture-upscale. `src/components.rs  Marker components (Spinning, AlphaBlend, TwoSided, Decal) + app resources` is now a ~30-LOC hub. Other named docs (README, ROADMAP Crate Map) are complete.
- **Evidence**: `ls crates tools`; `sed -n '/^## Workspace Structure/,/^## Architecture/p' CLAUDE.md`.
- **Impact**: CLAUDE.md is loaded into every session; agents locate physics / save / audio code from it.
- **Related**: #5602, #5105, #4351.
- **Suggested Fix**: add a one-line-per-crate list for the 19 missing members (names + one clause), or replace the crate list with a pointer to ROADMAP's Crate Map; update the `components.rs` row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
