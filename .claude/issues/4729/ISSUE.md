# EXT-D3-2026-09-21-01: WindField.direction has no defined frame — grass leans against its own gust waves and against the trees

**Issue**: #4729
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: MEDIUM (visual; a canonical-meaning divergence across five consumers)
**Dimension**: Ground-cover pipeline (canonical `WindField`), cross-consumer
**Tier Violated**: no-leak (one canonical value, several incompatible interpretations downstream)
**Game Affected**: all games with ground cover; FO3/FNV/Oblivion additionally for SpeedTree sway
**Location**:
- `crates/core/src/ecs/components/groundcover.rs:377-387` (definition, no frame stated)
- `crates/renderer/shaders/groundcover_blade.vert:241-242` (advection), `:256-257` (lean)
- `byroredux/src/systems/billboard.rs:230,240` (SpeedTree sway axis)
- `crates/physics/src/water.rs:246` (body drag)
- `byroredux/src/render/water.rs:273-280` (water, see #4728)

## Description
`WindField.direction` has no documented axis/sense. Four consumers read it directly as engine (x, z) — grass gust advection, SpeedTree sway phase, physics drag, water scroll — while only the grass lean negates y. No single reading satisfies all five: under the majority "toward" reading, the grass lean is mirrored and the SpeedTree crown and water ripples point upwind.

## Evidence
Direction-probe table (glam 0.29.3, `/tmp/audit/exterior/probe_wind`) for direction `[1,0]` and `[0,1]`: tree crown and water ripples read `-x/-z`; grass lean reads `+x/-z`; grass gusts and body drag read `+x/+z`. No test pins a lean sign on any consumer.

## Impact
Every windy exterior with ground cover: blades lean opposite their own gust wave roll on N-S wind; FO3/FNV/Oblivion SpeedTree billboards lean opposite the grass beneath them on E-W wind. Visual only.

## Suggested Fix
Define the frame once on `WindField` (engine xz, "blows toward"), then fix the consumers that break it: remove the grass lean's `-w.y`, reverse the SpeedTree axis sign, and fix the water sign under #4728. Add one pin per consumer.

## Related
#4728 (water scroll sign, shares this question), #3191 (closed, different axis bug), #4186

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D3-2026-09-21-01)
