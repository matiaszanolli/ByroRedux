# NIF-D1-01: Three FO76 particle blocks silently drop their #BS_F76# fields

**Issue**: #1896 · **Severity**: MEDIUM · **Labels**: medium, nif-parser, nif, bug
**Dimension**: Stream Position · **Filed from**: docs/audits/AUDIT_NIF_2026-07-06.md (nif-deep suite)
**Game**: Fallout 76 (bsver 155 / #BS_F76#) only
**Location**: crates/nif/src/blocks/particle.rs — parse_particles_data (NiPSysData), parse_rotation_modifier (NiPSysRotationModifier), BSPSysSimpleColorModifier::parse

## Description
FO76 nif.xml inserts #BS_F76#-gated fields into three particle blocks these parsers don't model.
Constant per-type drift, byte-exact: NiPSysData +12B ×7478, NiPSysRotationModifier +17B ×4571,
BSPSysSimpleColorModifier +52B ×7306 (last one already documented in-code at particle.rs:540).

## Evidence
nif_stats --drift-histogram on SeventySix - Meshes.ba2 → 19,355 drift events across exactly these
three types, each cross-checked against nif.xml #BS_F76# additions.

## Impact
Negligible today (discarded stubs, block_sizes recovers, no cascade). Real cost: ~19k drift events
raise the NIF_STATS_MAX_DRIFT_EVENTS noise floor; latent trap if promoted to typed consumption.

## Suggested Fix
Gate the #BS_F76# fields under bsver >= FO76 (12B/17B fixed) so consumed == block_size, matching
read_starfield_tail. Alt: extend the BSPSysSimpleColorModifier comment to the other two.
