# #6954: TD2-2026-11-15-01: The Havok classic-packfile container is decoded twice, in `nif` and in `hkx`, with different hardening in each

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6954
- **Labels**: medium,nif-parser,nif,import-pipeline,physics,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: MEDIUM (duplicated logic, divergent bug-fix history) · **Effort**: medium · **Labels**: `tech-debt` · **Existing**: none
- **Age**: `crates/hkx/src/packfile.rs` 02c24e4f1 (2026-08-01); `crates/nif/src/blocks/collision/havok_packfile.rs` 83565a9b0 (2026-08-31, #3809)
- **Location**: `crates/nif/src/blocks/collision/havok_packfile.rs:60-62` (constants), `:320-` (`read_local_fixups`, `read_global_fixups`, `read_virtual_fixups`), `parse_havok_packfile` (`:382-`); `crates/hkx/src/packfile.rs:1-250` (`Packfile::parse`)
- **Description**: Both parse the same container: magic pair `0x57e0e057 / 0x10c0c010`, header fields at `0x0C` (version), `0x10..0x13` (layout rules), `0x14` (section count), `0x28` (contents-version string), the section table (19-byte name, then seven `u32` offsets), the three fixup tables (8-byte local, 12-byte global, 12-byte virtual, each ending at a `0xFFFFFFFF` sentinel), and the `__classnames__` names. The two have drifted apart, one fault class at a time:
  - **hkx handles v11's predicate area**: `table_start = 0x40 + u16 @ 0x3E` (`packfile.rs:100-114`; FO4 animation clips carry a 0x10-byte one, found under #5901). nif hard-codes `SECTION_TABLE_START = 0x40` and never reads `0x3E` (`grep 0x3E` in `havok_packfile.rs` is empty). A v11 blob with a predicate area would read its section table 0x10 bytes early. The 25 sampled FO4 `bhkSystemBinary` blobs have none, so it is latent.
  - **hkx validates section ordering** (`start <= local <= global <= virtual <= exports <= end <= len`) and range-checks each global fixup's target section and each virtual fixup's class section (`:134-197`). nif does none of these; it clamps reads with `end.min(data.len())` and keeps the raw offsets.
  - **nif fixed `Vec::with_capacity(num_sections)` from an on-disk `u32` (#4624) and `u32 + u32` overflow in `absolute_end` (#4155)**. hkx has its own cap (`section_count > 64`) and checked adds, found independently.
  - **hkx fixed the quadratic class-name string materialisation (#4648/#4649)**: it stores `(source, name_start, name_len)` and compares in place. nif's `class_names: Vec<String>` is per-record, so it is not quadratic, but it is a different representation of the same table.
  - Each decoder carries its own `read_u32` / `read_cstr` pair (`nif havok_packfile.rs:263,268`, `hkx packfile.rs:373,388`).
- **Suggested Fix**: one container decoder. hkx already has the more strictly validated one, and it has no dependency on nif, so the lower-risk direction is `crates/nif` depending on `byroredux-hkx`. Expose a public `hkx::container::{PackfileHeader, Section, ClassNames}` view (header, section table with the validated offsets, the three fixup tables, the `__classnames__` list with name offsets) from the existing `Packfile::parse`, and rebuild `HavokPackfile` / `PackfileSection` in `nif` as a thin adapter over it. Keep nif's `is_havok_tagfile_error` (the `TAG0` check, #6302) in nif, since it is the caller-side gate. The existing synthetic-blob tests in `havok_packfile.rs` and the corpus example `crates/nif/examples/havok_blob_recon.rs` become the equivalence check. `np_ragdoll.rs:309` is the only production caller.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
