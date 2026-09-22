# PAR-D6-2026-09-21-01: BA2 never checks name_table_offset against the file length, so a truncated mod archive fails with a bare "failed to fill whole buffer"

Labels: low,bug,import-pipeline

## Description
`crates/bsa/src/ba2.rs:194` and `:301`: `reader.seek(SeekFrom::Start(name_table_offset))?` is followed directly by `read_exact` with no `> file_len` check anywhere in between. The BSA sibling names the equivalent field before seeking (`archive/open.rs:110-131`, #3368) so its own error at least identifies which offset was bad; the BA2 path does not.

Verified unchanged at HEAD `ee6d3fb39`: the seek-then-read_exact sequence at both cited lines still has no bounds check ahead of it.

## Evidence
Probe `dup-scan` over the installed FO4 Data: `cuwp - textures.ba2` (BTDX v1 DX10, 338 files) declares `name_table_offset` 1,250,980,735 in a 214,135,265-byte file, which looks like a truncated download. `Ba2Archive::open` -> `Err("failed to fill whole buffer")`, surfaced as "BA2 '<path>': failed to fill whole buffer" with no offset/size context.

## Impact
Correct rejection, but an uninformative error — the operator cannot tell a truncated archive from a reader bug from the message alone.

## Related
#3368 (the BSA-side equivalent check this should mirror), PAR-D1-2026-09-21-07 (sibling I/O-discipline finding)

## Suggested Fix
Validate `name_table_offset <= file_len` (and record offset plus size) at open, with a named error mirroring #3368.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix