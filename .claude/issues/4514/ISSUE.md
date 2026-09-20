# REN-D10-2026-09-20-01: spawn_mesh_instance's ESM-light fallback bypasses canonical_light_falloff_exponent — the third LIGH spawn path missed by 6b4e6252c

- **ID**: REN-D10-2026-09-20-01
- **Labels**: medium,renderer,bug,game:oblivion,game:fo3,game:fnv
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: MEDIUM · **Dimension**: Light Animation
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D10-2026-09-20-01)

**Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1426` (raw `ld.falloff_exponent`); canonicalized siblings at `cell_loader/references/synth_child.rs:394,500`

**Description**
6b4e6252c resolved the LIGH falloff-exponent sentinel per layout generation on two of the three spawn paths. The ESM-light fallback in spawn_mesh_instance (gated `spawned_nif_lights == 0` — the dominant meshed-lamp path, Prospector-Saloon class) still passes the raw value. Pre-Skyrim 32-byte LIGH always carries the 0.0 sentinel, so Oblivion/FO3/FNV meshed lamps resolve k=1.0 via `Emitter::from_legacy_world_units`'s non-ESM last-resort net instead of k=2.0.

**Evidence**
Three call sites of the falloff lane; only two go through `canonical_light_falloff_exponent`.

**Impact**
Wrong falloff curve (too-flat) on most classic-game meshed lamps — visual only, per-game blast radius across Oblivion/FO3/FNV.

**Suggested Fix**
Thread a falloff lane through `PlacementCtx` like the three already-canonicalized light fields; add one guard pinning all three sites.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
