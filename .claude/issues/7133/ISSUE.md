# #7133: PAR-D3-2026-11-20-01: `decode_np_ragdoll` hard-codes 64-bit Havok 2014 offsets but no longer (or ever) checks the file says so, and #6954 widened what reaches it

**Labels**: low,nif-parser,nif,physics,bug,game:fo4
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7133

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D3-01 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Version Gating
- **Trigger Input**: a `bhkRagdollSystem` blob whose header says pointer width 4 (`byte 0x10 = 4`), or v8/`hk_2010` contents, but which still carries a virtual fixup naming `hknpRagdollData`.
- **Location**: `crates/nif/src/blocks/collision/np_ragdoll.rs:303-345` (`decode_np_ragdoll`), `crates/hkx/src/container.rs:70-78` (the `4 | 8` and `8 | 11` allowance)
- **Status**: NEW
- **Description**: the module doc states "little-endian, 64-bit pointers" and every row offset (`hkArray` count at `+8`, member stride 16, `+0` pointers) assumes it, but `decode_np_ragdoll` never consults `pack.header.layout_rules[0]` or `file_version`; its only selector is the class name `hknpRagdollData`. Before #6954 nif accepted any version; after it the container admits 32-bit and v8 packfiles for hkx's sake, so the "nif is now as strict as hkx" statement is true of the container but not of what this decoder needs (it needs strictly v11 + 64-bit, which is stricter than hkx's own set).
- **Evidence**: probe1 `np`: real `character\characterassets\skeleton.nif` ragdoll blob decodes to 18 bodies / 17 constraints; with only `blob[0x10] = 4` it still decodes to 18 / 17 (the 64-bit offsets are applied to a file that says 32-bit). Setting v8 + `hk_2010` fails only incidentally (section header size shifts: "global fixups precede local fixups"), not by a gate.
- **Impact**: none for shipped content (all 75 FO4 ragdoll blobs are v11 / 8-byte pointers). A hostile or corrupt blob decodes to a garbage skeleton instead of an `Err`; every read is bounds-checked, so no panic/OOM. Contract (c) "silent" only.
- **Related**: #6954 (container unification); `parks_authored_pose` consumer unaffected.
- **Suggested Fix**: in `decode_np_ragdoll`, after `parse_havok_packfile`, require `pack.header.file_version == 11 && pack.header.layout_rules[0] == 8` and add a test mutating those bytes of the synthetic blob.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
