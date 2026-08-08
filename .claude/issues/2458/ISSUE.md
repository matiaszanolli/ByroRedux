# SUBSYS-03: Bone-name to entity binding is case-sensitive on the skinning + ragdoll paths but case-insensitive everywhere else

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2458
**Finding ID**: SUBSYS-03 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 7 — Subsystem coverage vs legacy
**Location**: `byroredux/src/scene/nif_loader.rs:412,449,966-968,994-998,1089`; `byroredux/src/ragdoll.rs:83-104`; contrast `crates/core/src/string/mod.rs:40-88`
**Status**: NEW

## Description
`StringPool` ASCII-lowercases every intern explicitly to match Gamebryo's `GlobalStringTable` behavior, so `Name(FixedString)` comparisons (and animation channel binding) are case-insensitive. The skeleton binding path bypasses that pool entirely: `node_by_name: HashMap<Arc<str>, EntityId>` is keyed on the raw, case-preserved NIF node name, and both the skin-bone lookup and the ragdoll template lookup do exact-match `Arc<str>` comparisons — byte-exact, case-sensitive. Two different normalisation regimes for the same conceptual identifier, visible across six adjacent lines in `nif_loader.rs`.

## Evidence
Confirmed directly: `StringPool::get_or_intern`/`get` (`crates/core/src/string/mod.rs:56,86`) call `.to_ascii_lowercase()`. `node_by_name` (`nif_loader.rs:412`) is a plain `HashMap<Arc<str>, EntityId>` with no normalization, looked up at `:968,997` via `node_by_name.get(&bone.name)`/`node_by_name.get(n)`.

`external_skeleton` and the body/head NIF's own bone names come from independently authored string tables (`npc_spawn/resumable.rs:540,562` loads skeleton.nif separately from body/head NIFs), so the cross-file exposure is real, not theoretical.

## Impact
A case-only divergence between an armour/body NIF's skin bone list and skeleton.nif's node names silently unresolves that bone — `compute_palette_into` substitutes identity, producing the "exploded limb" artefact for skinning, or drops the ragdoll body (potentially below the 2-body floor, disabling ragdoll entirely) with a `log::warn!`. Vanilla content is self-consistent, so this mostly bites 3rd-party/modded skeletons and outfits — precisely ByroRedux's target compatibility surface, and Bethesda's own tooling is case-insensitive so mods have no incentive to be byte-exact.

## Suggested Fix
Key `node_by_name`/`rest_pose_by_name`/`SkeletonMap` on `FixedString` through the same `StringPool` used for `Name`, so every bone-name comparison shares one normalisation. Cheaper interim: add a lowercased-key fallback lookup with a `log::warn!` when it's what resolves, measuring real-content incidence.

## Completeness Checks
- [ ] **TESTS**: A regression test binds a skin/ragdoll with a case-mismatched bone name (e.g. `Bip01 Spine` vs `bip01 spine`) and confirms it resolves
- [ ] **SIBLING**: Confirm `SkeletonMap`/animation channel binding elsewhere use the same normalized key as `Name`
