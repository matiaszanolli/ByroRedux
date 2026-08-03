# FO3-D5-01: bhkRigidBody (non-T) CInfo transform applied unconditionally — 9.5% of FO3 meshes get displaced colliders

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2316

**Severity**: HIGH
**Location**: `crates/nif/src/import/collision/mod.rs:269-282` (`extract_from_classic`); parser side `crates/nif/src/blocks/mod.rs:1147` + `crates/nif/src/blocks/collision/rigid_body.rs:295`
**Status**: NEW. **Directly contradicts the "informational (non-bug)" dismissal in `docs/audits/AUDIT_FO3_2026-07-16.md:189-193`**, whose premise ("non-T bodies always ship identity translation/rotation in vanilla content") this audit found to be empirically false.

### Description
nif.xml declares `bhkRigidBodyT` as a distinct type whose only semantic difference from plain `bhkRigidBody` is that the "T" suffix marks the body as active for translation/rotation — a non-T body's CInfo `Translation`/`Rotation` fields must be treated as identity. ByroRedux collapses both type names onto one struct at dispatch (`"bhkRigidBody" | "bhkRigidBodyT" => BhkRigidBody::parse`), destroying the T-ness at parse time. `extract_from_classic` then applies the CInfo transform to *every* body whenever it's non-identity, T or not.

Confirmed against current code: `blocks/mod.rs:1147` — `"bhkRigidBody" | "bhkRigidBodyT" => Ok(Box::new(BhkRigidBody::parse(stream)?))` — both type names dispatch to the same struct with no `is_t` flag retained. `import/collision/mod.rs:269-282` (`extract_from_classic`) computes `has_offset` from the body's translation/rotation and wraps the shape in `CollisionShape::Compound` whenever non-identity, with no check for T-suffix at all.

### Evidence
Real FO3 data (`Fallout - Meshes.bsa`, block-type name from the header table joined to the parsed block by index): `bhkRigidBodyT` = 2,054 blocks (1,998 nonzero translation); `bhkRigidBody` (non-T) = 6,903 blocks, **2,704 with nonzero translation, 1,649 with nonzero rotation**; **1,041 files (9.5% of the archive) carry a non-T body with a nonzero transform**. Offset magnitude actually applied (×7.0 havok scale, engine units): p50=160.7, p90=2820.9, max=20932.8. By motion type, 1,007 of the displaced bodies are `FIXED` — walkable static world geometry (e.g. `meshes\architecture\megaton\megatonrampturn45sml.nif`, `meshes\dungeons\utility\catwalks\utlcat3way01.nif`). Two independent reference implementations (NifSkope, nifly) gate this transform on the type name; this codebase does not. Same shape confirmed in the DLC archives (8,783 non-T vs 1,489 T bodies).

### Impact
~9.5% of FO3 meshes get a collider translated away from its render geometry by a median of 160 units. Symptoms: invisible walls, walk-through architecture, falling through catwalks/ramps, clutter resting mid-air. Not masked by the `spawn.rs` synthesized-trimesh fallback since `extract_collision` returns `Some`. Shared with Oblivion/FNV/Skyrim (identical code path) — the FO3 numbers above are what was measured this cycle.

### Suggested Fix
Retain T-ness at parse — add `pub is_t: bool` to `BhkRigidBody` set from the dispatch arm, gate the `has_offset` wrap in `extract_from_classic` on it. Add a regression test pinning "non-T body with non-identity translation ⇒ shape is NOT wrapped in a Compound".

### Related
`docs/audits/AUDIT_FO3_2026-07-16.md:189-193` (the stale dismissal this finding corrects); #1230 (havok scale); #1832/#1874 (zero-mass reclassification, adjacent code)

## Completeness Checks
- [ ] **SIBLING**: Shared code path with Oblivion/FNV/Skyrim — same fix + regression test applies to all four; DLC archives already confirm the same ratio (8,783 non-T vs 1,489 T)
- [ ] **TESTS**: A regression test pins "non-T body with non-identity translation ⇒ shape is NOT wrapped in a Compound" (per the suggested fix)
