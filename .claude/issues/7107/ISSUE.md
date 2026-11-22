# #7107 — ESM-D2-11: `OTFT.INAM`, `CELL.XCLR`, `kwda_words` still hand-chunk FormID arrays and drop a ragged tail; doc says every array uses `u32_array`

Labels: low,esm-plugin,tech-debt,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D2-11)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D2-11**

- **Severity**: LOW
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `OTFT` / `INAM` (outfit items), `CELL` / `XCLR` (region list); `KWDA` via `kwda_words`
- **Location**: `crates/plugin/src/esm/records/outfit.rs:80` (`sub.data.as_chunks::<4>().0`, `u32::from_le_bytes(*chunk)`, test `malformed_inam_short_payload_is_dropped` pins the drop), `crates/plugin/src/esm/cell/helpers.rs:364-370` (`read_form_id_array`, doc: trailing bytes "always alignment padding"), `crates/plugin/src/esm/records/common.rs:489-492` (`kwda_words`: `&data[..len - len % 4]` then `u32_array()`, which makes the decline unreachable)
- **Status**: NEW (sibling sites of #6782, which listed only MUSC `TNAM`, MUST `SNAM`, WTHR `HNAM`/`WGDR`; the checklist's rule is "a hand-rolled `chunks_exact(4)` / `as_chunks::<4>()` that drops a tail is the regression")
- **Description**: these are the same read as the four sites #6782 moved onto `SubReader::u32_array`, with the opposite policy (silently shorten). Each documents the drop as intentional ("alignment padding"), but the claim is unsourced: nothing shows a padded vanilla body.
- **Evidence**: Skyrim.esm (walker, this HEAD): 18 FormID-array sub-record kinds checked (`KWDA` on 16 record types, 8,800+ sub-records; `OTFT.INAM` 481; `CELL.XCLR` 8,441), zero ragged bodies. So the "tolerant of a padded vanilla body" justification in the `kwda_words` doc is not borne out. Also `common.rs:488` says "every FormID array reads through `SubReader::u32_array`", which these sites contradict.
- **Impact**: a truncated body decodes as a shorter valid array instead of declining (nothing on vanilla).
- **Suggested fix**: pick one policy and say it once. Either route the three through `u32_array` (decline on ragged, update `malformed_inam_short_payload_is_dropped`) or correct the doc and the checklist text to say that array tails are tolerated, with `u32_array` for the declining cases.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
