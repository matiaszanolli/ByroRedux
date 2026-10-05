# #5262: NIF-D4-2026-10-05-01: the #5189 `affected_node_names` doc says Oblivion has no on-light Affected Nodes list "pre-10.1.0.0"; Oblivion is v20.0.0.x and carries the field on the wire

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5262
- **Labels**: low,nif-parser,nif,documentation,doc-rot,game:oblivion
- **Source**: `docs/audits/AUDIT_NIF_2026-10-05.md` (NIF-D4-2026-10-05-01)

_From `docs/audits/AUDIT_NIF_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW (doc-rot; behaviour is correct)
- **Dimension**: Geometry Handoff (doc on the parse→import type)
- **Game Affected**: Oblivion (v20.0.0.4/5, bsver 11)
- **Location**: `crates/nif/src/import/types.rs:52-62` (`ImportedLight::affected_node_names`); `:117-121` (`ImportedTextureEffect` points at it)
- **Status**: NEW. #5189 (closed) fixed the spawn and wrote this doc. The wording came from REN-D10-2026-10-03-01's suggested fix. No issue covers the doc.
- **Description**: the doc reads "pre-10.1.0.0 files (Oblivion) carry no on-light list at all, so the scope is serialized on the node side". Two facts contradict it:
  - nif.xml:3503-3504 gates `Num Affected Nodes` / `Affected Nodes` `since="10.1.0.0" vercond="#NI_BS_LT_FO4#"`, so the list **is** on the wire for Oblivion, and the parser reads it (`blocks/base.rs:200`, `pre_fo4 && version >= V10_1_0_0`). The pre-10.1 groups are the `until="4.0.0.2"` Morrowind-era ones.
  - The scratch probe over `Oblivion - Meshes.bsa` found 48 carrier files and 95 `__MAX_Default_Light` NiDirectionalLights. Every one has the field **present with count 0**, and 47 root NiNodes list them in `effects`.
  The conclusion is right (empty ≠ unrestricted; scope lives on `NiNode.effects`), but the stated reason is a version gate that does not exist.
- **Impact**: this doc is where an editor lands before touching `NiDynamicEffect`. "Oblivion has no on-light list" invites gating the read off for v20.0. That is a 4-byte under-read per light on a format with no `block_sizes`, which truncates the scene.
- **Related**: #5189, #335, REN-D10-2026-10-03-01, #721/#1240 (NiDynamicEffect gate history).
- **Suggested Fix**: reword to "Oblivion-era content (v20.0.0.x) writes the list empty and registers scope on the node side (`NiNode.effects`); FO4+ drops the list at the wire level."

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
