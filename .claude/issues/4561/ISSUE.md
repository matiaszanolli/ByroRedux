# NIFAL-D5-2026-09-21-03: APP_CULLED / editor-marker gating missing on the NiParticleSystem leaf in both walkers

**Labels**: low, nifal, nif-parser, bug

**Severity**: LOW · **Dimension**: Particles (block selection feeding the boundary) · **Tier Violated**: no-fabrication (an emitter authored hidden is translated and spawned as visible) · **Game Affected**: all NIF-particle games (incidence unmeasured)
**Location**: `crates/nif/src/import/walk/mod.rs:642-668` (`walk_node_flat` particle arm); `crates/nif/src/import/walk/emitter.rs:791-820` (`walk_node_particle_emitters_flat` particle branch)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
Every node arm and geometry arm in both walkers gates on `av.flags & 0x01` (APP_CULLED) — shapes via the #3640 nuance (`!has_live_visibility_controller`, since a live `NiVisController` may un-cull later). Neither particle arm checks the block's own flags (or `is_editor_marker`): a `NiParticleSystem` authored APP_CULLED under a visible parent still spawns particles from frame zero.

### Evidence
Flag reads at `mod.rs:280/370/720/767` and shape arms `:461/:514/:556/:595`; the `downcast_ref::<NiParticleSystem>` arm at `:642` has no flag read. Same shape in `walk_node_particle_emitters_flat`.

### Impact
Hidden-at-author FX (state-machine-driven emitters that start culled) render from frame zero. Frequency unmeasured; the analogous #3640 census found 581 APP_CULLED shapes in 13 FNV files, so culled leaves are a real authoring pattern. A naive fix dropping culled emitters outright would delete emitters a vis controller later unhides — hence the #3640-mirroring condition.

### Related
#3640, #222

### Suggested Fix
Mirror the shape arms: skip the emitter push only when `ps.av.flags & 0x01 != 0 && !has_live_visibility_controller(scene, ps.av.net.controller_ref)` (plus the cheap `is_editor_marker(name)` check), in both walkers, with a fixture each.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
