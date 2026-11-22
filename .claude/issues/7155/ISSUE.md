# #7155 — PAR-D1-2026-11-21-02: `np_ragdoll` `Fixups::pointer` / `class_at` (and nif `HavokPackfile::class_name_at`) are still linear per call

Labels: low,import-pipeline,nif-parser,bug,game:fo4

- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `crates/nif/src/blocks/collision/np_ragdoll.rs:269-299` (`Fixups::pointer`, `Fixups::class_at`); `crates/nif/src/blocks/collision/havok_packfile.rs:216` (`class_name_at`)
- **Status**: NEW (sibling of #7126, closed: only `objects()` got the map)
- **Trigger Input**: a `bhkRagdollSystem` blob with a few MB of local or virtual fixups, plus 256 bodies (`MAX_ELEMENTS`) whose compound shapes carry 64 children (`MAX_COMPOUND_CHILDREN`). The bodies can share one compound object.
- **Description**: each lookup runs `.iter().find` over the whole fixup table. That is about 33k calls × several hundred thousand entries.
- **Evidence**: the code was read (`data.local_fixups.iter().find(|f| f.src_offset == src)`; `class_name_offsets.iter().position`). The cost is an estimate from the caps; it was not timed.
- **Suggested Fix**: build the `src → dst` and `data_offset → class` maps once when `Fixups` is constructed, and give `class_name_at` the map `objects()` already builds.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
