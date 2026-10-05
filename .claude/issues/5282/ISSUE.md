# #5282: NIFAL-D3-2026-10-05-01: Affected-node light scoping is untranslated for every `NiLight` kind, but `nifal.md` parks it only for `NiAmbientLight`; #5189's name allowlist is the only mitigation and is recorded nowhere in the spec

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5282
- **Labels**: low,nifal,documentation,legacy-compat,game:oblivion
- **Source**: `docs/audits/AUDIT_NIFAL_2026-10-05.md` (NIFAL-D3-2026-10-05-01)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Skinning/Lights · **Tier Violated**: parked-not-leak (record-keeping) · **Game Affected**: Oblivion, FO3, FNV, Skyrim (pre-FO4 node `effects` lists)
- **Location**:
  - `docs/engine/nifal.md:268-286` (the only scoping note, ambient-only);
  - `crates/nif/src/blocks/node.rs:22,83` (`NiNode.effects` parsed for bsver < FO4);
  - `crates/nif/src/import/walk/texture_effect.rs:18-54` (the only walker of `effects`);
  - `byroredux/src/cell_loader/spawn.rs:1099-1101` and `:1146-1163` (the allowlist skip).
- **Status**: NEW. Related to #5189 (closed) and NIF-D4-2026-10-05-01/-02 (the doc wording and the count predicate; not this gap).
- **Description**: #5189 established, and NIF-D4-2026-10-05-01 refined, that Gamebyro scopes every `NiDynamicEffect` to its
  affected-node subtrees. Oblivion-era content writes the on-light list empty and registers scope on `NiNode.effects`; 47 root
  nodes list the artifact lights there.
  - The importer parses `NiNode.effects`, but walks it only for texture effects. `ImportedLight` therefore carries no scope, and
    every point/spot/directional NIF light spawns unscoped.
  - The fix for the one known case is a consumer-side, census-backed name skip (`__MAX_Default_Light`, 48 Oblivion carriers).
  - `nifal.md`'s Lights section is marked **converged**. It mentions affected-node scoping only as an `NiAmbientLight` parking
    note ("none with `affected_node_names`"). It records neither the general gap nor the spawn-gate allowlist.
- **Evidence**: `grep -rn affected_node byroredux/src crates/renderer/src` finds no consumer. The `effects` walkers are in
  `walk/texture_effect.rs` only.
- **Impact**: There is no known vanilla population beyond the allowlisted artifact. A scoped non-artifact light (modded, or an
  unsurveyed vanilla mesh) would light the whole scene. The next audit cannot tell parked from dropped.
- **Related**: #5189, #5123, #3557, NIF-D4-2026-10-05-01/-02, the baseline ledger row for `NiAmbientLight`.
- **Suggested Fix**: Add a Lights ledger row in `nifal.md` §2: "affected-node scope (on-light list and pre-FO4 `NiNode.effects`)
  is not translated for any kind; the exporter artifact is dropped by name at `spawn_nif_lights` (#5189)". Optionally census
  `effects`-listed non-artifact lights with `crates/nif/examples/ambient_light_census.rs`'s approach.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
