# #7126: PAR-D1-2026-11-20-03: `HavokPackfile::objects()` / `class_name_at` is O(virtual fixups x class names); a 2.85 MB `bhkRagdollSystem` blob takes 4.5 s

**Labels**: low,nif-parser,nif,physics,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7126

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D1-03 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `crates/nif/src/blocks/collision/havok_packfile.rs:215-243` (`class_name_at`, `objects`), reached from `crates/nif/src/blocks/collision/np_ragdoll.rs:319` (`decode_np_ragdoll`) and `crates/nif/src/import/collision/ragdoll.rs:371`
- **Status**: NEW (the same shape was fixed for the hkx reader in #4648/#4649; #6954 shared the *container* but the nif view kept its own lookup)
- **Trigger Input**: an FO4 NIF whose `bhkRagdollSystem` blob is a valid v11 packfile with N one-character class-name records (7 B each) in `__classnames__` and N virtual fixups in `__data__` (12 B each), every fixup naming the last record.
- **Description**: `class_name_at` does `class_name_offsets.iter().position(..)` per call and `objects()` calls it once per virtual fixup. Both tables are only file-bounded, so the work is quadratic in the blob size. `Fixups::pointer` / `class_at` in `np_ragdoll.rs` do the same linear `find` per joint, but those are capped by `MAX_ELEMENTS` 256.
- **Evidence**: probe `hk_probe <N>` (release; blob built by hand, parses `Ok`): N=20,000 (380 KB) objects() 80 ms; N=80,000 (1.5 MB) 1.28 s; N=150,000 (2.85 MB) 4.47 s; parse itself 6.7 ms.
- **Impact**: a modded ragdoll skeleton NIF stalls its importer for seconds per MB of blob; nothing allocates unboundedly. Vanilla blobs hold five objects.
- **Related**: #6954, #4648, #4649.
- **Suggested Fix**: build a `HashMap<u32, usize>` (or sort `class_name_offsets` and `binary_search`) once in `parse_havok_packfile` and use it in `class_name_at`; cap virtual-fixup / class-name counts the way `MAX_ELEMENTS` does for the ragdoll arrays.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
