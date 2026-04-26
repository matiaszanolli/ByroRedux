# Issue #689: O6-N-01: KF importer has no NiSequenceStreamHelper + NiKeyframeController path — every Oblivion KF parses to zero clips

**Severity**: HIGH
**Files**: `crates/nif/src/anim.rs:252` (`import_kf`), `crates/nif/src/blocks/controller.rs:1828-1859` (parser stub)
**Dimension**: Blockers (cross-cuts Oblivion / FO3 / FNV)

`import_kf` only walks `NiControllerSequence::controlled_block`s. `NiSequenceStreamHelper` parses cleanly (`controller.rs:1840-1859`) but the comment at `:1835-1838` is explicit:

> "We don't currently consume this from the animation importer — that work remains as a follow-up"

`NiKeyframeController` is in the block-type dispatch list (`crates/nif/src/lib.rs:102`) but `import_kf` has no case arm for it (look around lines 740-810 for the controller-type switch).

**Net effect**: every Oblivion-era KF (door open, creature idle, NPC walk cycle) parses without error and produces **zero AnimationClips**.

**Impact**: All Oblivion door idles, creature idles, NPC walk cycles dead-on-arrival. Anvil interior already opens despite this because it has no animated content; any dungeon, hallway with auto-doors, or cell with creatures gets bind-pose-only animation. Cross-cuts FO3/FNV (which also use this chain pre-BSVER-24).

**Fix sketch**: Add a Path 3 in `import_kf`:
1. Detect a `NiSequenceStreamHelper` root.
2. Walk its `extra_data` chain for `NiTextKeyExtraData` + per-bone `NiKeyframeController`s. Each `NiKeyframeController` carries a target `name` and a `data_ref` → `NiKeyframeData`.
3. Reuse `extract_translation_channel` / `extract_rotation_channel` / `extract_scale_channel` against the keyframe data.
4. Resolve target node by name via `anim_convert::build_subtree_name_map` (already wired).

Estimate: 1-2 days per the 04-17 audit estimate.

**Renderable-interior milestone**: this is the missing piece for any Oblivion cell with creatures or animated doors.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
