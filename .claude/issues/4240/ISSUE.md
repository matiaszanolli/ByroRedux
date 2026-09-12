# FO3-2026-09-11-D2-01: authored emitter azimuth (planar_angle) decoded, never forwarded — 319/422 FO3 emitters spawn a ring where the file authors a wedge

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4240
**Labels**: bug, medium, legacy-compat, game:fo3, nifal
**Source**: `/audit-fo3` — `docs/audits/AUDIT_FO3_2026-09-11.md`, finding FO3-2026-09-11-D2-01

**Severity**: MEDIUM
**Dimension**: NIF v20.2.0.7 Parser (FO3 Block Subset) — `/audit-fo3` Dimension 2
**Location**: decoded at `crates/nif/src/blocks/particle.rs:94-96`; finite-swept but not copied at `crates/nif/src/import/walk/emitter.rs:284-290` (`extract_emitter_params`) vs the spawn draw at `byroredux/src/systems/particle.rs:477-478`

**Description**: `nif.xml` defines `Planar Angle`/`Planar Angle Variation` as the emission cone's azimuth mean/spread. Decoded, but `extract_emitter_params` only reads them to reject non-finite values and drops both before constructing `ImportedEmitterParams`. The spawn loop hardcodes a uniform `phi = rng() * TAU` draw regardless of authored azimuth.

**Evidence**: Confirmed — `planar_angle`/`planar_angle_variation` are read at `blocks/particle.rs:95-96` and finite-checked at `walk/emitter.rs:289-290`, but do not appear in the `ImportedEmitterParams` constructor. `byroredux/src/systems/particle.rs:477` hardcodes `let phi = rng() * std::f32::consts::TAU;` with no reference to either field. Census over all 422 `NiPSysEmitter` blocks (162 meshes) in `Fallout - Meshes.bsa`: 386/422 (91.5%) author a sub-circle azimuth window; 319/422 (75.6%) additionally author non-trivial declination spread, i.e. azimuth is geometrically observable; 12 author a non-zero mean. Real examples: `meshes\mps\mpsspitattack.nif` (var=π), `meshes\effects\ambient\fxambdust01.nif` (var=0.377), Paradise Falls signage, underwater explosion FX.

**Impact**: Three-quarters of FO3's authored emitters emit omnidirectionally where the file specifies a directional fan — spit/spray attacks, steam vents, directional dust, sign sparks, and `meshes\mps\` weapon-impact FX (highest-frequency combat particle content). Distinct from, and additive to, CLOSED #1445 (which only fixed the finite-sweep half); the forwarding half was never previously filed — a 2026-06-14 FO3 report scored this LOW as "inert", a premise this census disproves.

**Related**: #1445 (closed, finite-sweep half only), #1402, `docs/engine/nifal.md`.

**Suggested Fix**: Add both fields to `ImportedEmitterParams`/`ParticleEmitter`, forward in `extract_emitter_params`, bias the spawn draw to `phi = planar_angle + (rng()-0.5) * planar_angle_variation`, default variation to `TAU` for unauthored emitters to preserve today's behavior exactly where nothing was authored.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Both fields must be forwarded at the NIFAL import → `ImportedEmitterParams` boundary, not re-derived at spawn time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test on `meshes\mps\mpsspitattack.nif` (var=π) pins the directional-fan spawn behavior.
