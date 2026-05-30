**Severity:** LOW · **Dimension:** Import Pipeline (manifests here; root cause is a parse-time drop) · **Game Affected:** All (NiParticleSystem is universal; most visible on Skyrim+/FO4 multi-emitter FX)

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-05).

## Description
The `NiParticleSystem` parser reads the block's `NiAVObjectData` base (incl. local TRS relative to its parent NiNode) into `_av` and **immediately discards it**; the retained struct is only `{ original_type, modifier_refs }`. Both walkers then position the emitter purely from the host-node world transform. Any non-zero local translation/rotation authored on the particle block is lost. The legacy path (`legacy_particle.rs:522`) **retains** `av`, confirming the field is meaningful and the modern drop is the outlier.

## Location
- `crates/nif/src/blocks/particle.rs:903` and `:1219` (`let _av = NiAVObjectData::parse(stream)?;`)
- consumed-side: `crates/nif/src/import/walk/mod.rs:1148` and `:512`

## Evidence
`particle.rs:903`/`:1219` both bind to `_av` (confirmed). `ImportedParticleEmitter` (types.rs:880) has no translation/rotation field; `walk_node_particle_emitters_flat` composes only the parent chain.

## Impact
Emitters with a non-zero local offset spawn at the host node origin instead of the offset position. Vanilla torches/fires place the system at the node origin (zero offset) → invisible on the common case; surfaces on offset multi-emitter NIFs (campfire smoke above fire, FO4 steam stacks).

## Suggested Fix
Retain the parsed local transform on `NiParticleSystem`, compose it into `parent_transform` in the flat walker, and add a `local_translation`/`rotation` field to `ImportedParticleEmitter` for the hierarchical path so the scene builder anchors at host-world × block-local. The data is already on the wire and merely thrown away.

## Related
#401 (emitter import), #984 (force fields), #707 (color curve).

## Completeness Checks
- [ ] **SIBLING**: Fix both the modern path (`:903`) and the FO76 mesh-particle path (`:1219`); verify the hierarchical AND flat walkers both consume the retained transform
- [ ] **CANONICAL-BOUNDARY**: The retained transform flows through `extract_emitter_params`/the import-walk emitter path — keep emitter kinematics resolved at the parser→ECS boundary, not re-derived per frame in `byroredux/src/systems/particle.rs`
- [ ] **TESTS**: Fixture with a non-zero local offset on the particle block asserts the emitter spawn position includes it
