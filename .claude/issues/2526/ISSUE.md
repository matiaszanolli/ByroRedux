# NIF-D1-NEW-01: legacy_particle.rs reads several version-gated fields unconditionally, corrupting stream position for any genuine pre-Gamebryo instance

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2526
**Finding ID**: NIF-D1-NEW-01

**Severity**: MEDIUM
**Dimension**: Stream Position
**Game Affected**: `NifVariant::Morrowind` band only (NIF ≤ v10.0.1.0) — genuine NetImmerse-era content. **Not reachable** by any of the 7 currently-shipped/tested games — Oblivion is v20.0.0.5, already past every ceiling discussed here.
**Location**: `crates/nif/src/blocks/legacy_particle.rs:535` (`NiLegacyParticles::parse`, `has_shader`); `:608,645,652` (`NiLegacyParticlesData::parse`, `has_radii`/`has_rotation_angles`/`has_rotation_axes`); `:155` (`NiParticleBomb::parse`, `symmetry_type`)
**Status**: NEW

## Description
Per `docs/legacy/nif.xml`, the block types these parsers handle (`NiAutoNormalParticles`/`NiRotatingParticles`/their Data siblings) are themselves declared `until="V10_0_1_0"` at the `<niobject>` level. Several fields inside these parsers carry their own `since=` gate that sits *above* that object-level ceiling (or above true Morrowind's v4.0.0.2) and are read unconditionally instead of version-gated: `NiGeometry.Has Shader` (`since="10.0.1.0"`, read with a bare `read_byte_bool()` and no version check, where the sibling `NiTriShape::parse` gates the identical field correctly — proving the correct pattern is known and used elsewhere in the crate); `NiParticlesData.Has Radii` (`since="10.1.0.0"`, strictly *above* the block's own `until` ceiling — can never legitimately be present); `Has Rotation Angles`/`Has Rotation Axes` (`since="20.0.0.4"`, 10 major-version steps past the ceiling); `NiParticleBomb.Symmetry Type` (`since="4.1.0.12"`, above true Morrowind's v4.0.0.2).

## Evidence
```rust
// crates/nif/src/blocks/legacy_particle.rs:535
let has_shader = stream.read_byte_bool()?;   // nif.xml: since="10.0.1.0", no gate here
```
vs. the correct pattern already used in the sibling parser (`crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:140-152`):
```rust
} else if stream.version() >= NifVersion::V10_0_1_0
    && stream.version() <= NifVersion::V20_1_0_3
{
    let has_shader = stream.read_bool()?;
    ...
}
```

## Impact
For any genuine pre-Gamebryo NetImmerse `.nif` authoring these types, the unconditional reads consume phantom bytes not present on disk, with zero recovery (these files predate `block_sizes` and the Oblivion `no_block_sizes` stream-drift-detector's per-type median-size fallback only helps after at least one clean prior parse of the same type in the same file). **Practical blast radius today is zero** — Morrowind is not on ByroRedux's compatibility roadmap (Oblivion→Skyrim→FO3→FNV→FO4→FO76→Starfield) and corpus baselines confirm no supported game's vanilla archive ever emits these type names — but the module's own doc comment claims a scope ("older NetImmerse-era / mod / non-Meshes.bsa content") this code cannot actually fulfil correctly.

## Related
Same bug *class* as the resolved `#1301`/`#1310`/`#1843` phantom-bool family, but in the Oblivion-*unreachable* legacy particle stack, which was apparently never audited against nif.xml's object-level version ceilings. Not tracked under any open issue.

## Suggested Fix
Gate each field on the stream version exactly as `NiTriShape::parse` does (`>= V10_0_1_0 && <= V20_1_0_3` for `has_shader`); drop `has_radii`/`has_rotation_angles`/`has_rotation_axes` entirely as structurally unreachable given the object's own `until` ceiling; gate `symmetry_type` on `>= V4_1_0_12`. Given zero corpus coverage either way, a synthetic byte-stream regression test at exactly v4.0.0.2 would catch this class of bug cheaply. Alternative: if pre-Morrowind support isn't an actual goal, narrow the module doc's claimed scope instead of fixing the parse.

## Completeness Checks
- [ ] **TESTS**: A synthetic byte-stream regression test at exactly v4.0.0.2 pins the correct gating (zero real corpus coverage exists for this band)
- [ ] **SIBLING**: All four fields (`has_shader`, `has_radii`, `has_rotation_angles`/`has_rotation_axes`, `symmetry_type`) gated consistently
