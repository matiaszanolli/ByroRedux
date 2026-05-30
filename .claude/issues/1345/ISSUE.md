# #1345 — D6-01: BSPSysSimpleColorModifier authored particle colors dropped

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d6-01). GitHub is authoritative for live state — query `gh issue view 1345 --json state`._

**Severity**: MEDIUM · **Dimension**: Animation & Skinning (Particles) · **Source**: AUDIT_FNV_2026-05-30 (D6-01)

**Location**: `crates/nif/src/blocks/particle.rs:493-499` (parser) ; `crates/nif/src/import/walk/mod.rs` (`extract_first_color_curve`, ~617-637)

**Description**: `BSPSysSimpleColorModifier` is the dominant FO3/FNV-era particle-color modifier (`#FO3_AND_LATER#`) and carries its 3 colors INLINE (`Colors[3]` Color4: start/mid/end) rather than referencing a separate `NiColorData` block the way Oblivion-era `NiPSysColorModifier` does. `parse_simple_color_modifier` (particle.rs:493) is byte-correct but discards every value (returns an opaque `NiPSysBlock`), and `extract_first_color_curve` only `downcast_ref::<NiPSysColorModifier>()`. So an FNV NIF whose color is authored solely via the simple-color modifier returns `None` and the emitter falls back to the name-heuristic preset color.

**Evidence**: particle.rs:493-499 retains nothing (`original_type: "BSPSysSimpleColorModifier"`, opaque block). The color-drive scan in walk/mod.rs matches only `NiPSysColorModifier`. #707/FX-2 wired only the legacy `NiPSysColorModifier`→`NiColorData` path (consumed as `color_curve`), which is `None` for the simple-color path.

**Impact**: FNV/FO3 particle effects authored with `BSPSysSimpleColorModifier` (the common case for that era — geysers, steam, many weapon/spell FX) render with generic preset colors instead of the authored ramp. Visible-but-not-fatal cosmetic mismatch; kinematics/size/rate unaffected (those flow through the separate emitter path).

**Suggested Fix**: Capture `Colors[0]`/`Colors[2]` (start/end) in a typed `BSPSysSimpleColorModifier` struct (mirror `NiPSysEmitter`'s pattern), and extend `extract_first_color_curve` to fall back to the simple-color modifier when no `NiPSysColorModifier`+`NiColorData` chain is present — emitting the same `ParticleColorCurve` so both spawn sites' existing `color_curve` override picks it up unchanged.

## Completeness Checks
- [ ] **SIBLING**: Check whether other `BSPSys*` modifiers (gravity, bound, etc.) are similarly parsed-but-discarded where their authored params should drive the system.
- [ ] **CANONICAL-BOUNDARY**: The color must be extracted at the NIF→`ImportedParticleEmitter` boundary (import-walk) and consumed at the spawn site, not re-derived per frame in `systems/particle.rs`.
- [ ] **TESTS**: Regression test — a synthetic FNV particle NIF with only a `BSPSysSimpleColorModifier` yields a non-`None` `ParticleColorCurve` matching its inline start/end colors.
