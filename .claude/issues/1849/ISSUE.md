# LC0702-05: WRLD NAM3/NAM4 LOD-water + OFST cell-offset table skipped, untracked

**Source audit**: `docs/audits/AUDIT_LEGACY-COMPAT_2026-07-02.md` (finding LC0702-05)
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1849
**Labels**: low, legacy-compat, import-pipeline, bug

**Severity**: LOW
**Dimension**: EXAL — exterior record coverage (WRLD walker)
**Location**: `crates/plugin/src/esm/cell/wrld.rs` (WRLD sub-record dispatch — currently matches `EDID`/`CNAM`/`WNAM`/`PNAM`/`NAM0`/`NAM9`/`NAM2`/`DNAM`/`ZNAM`/`ICON`/`DATA`, everything else including `NAM3`/`NAM4`/`OFST` falls to the `_ => {}` default arm)
**Status**: NEW

## Description

`docs/engine/exal.md` §5.4 records that the WRLD `NAM3`/`NAM4` LOD-water fields
and the `OFST` cell-offset table are "currently skipped in `wrld.rs`" and feed
the LOD ring rather than the full-detail scene. Unlike the sibling VWD-flag gap
(#1731), no open issue tracks this skip — it risks being re-derived from
scratch in a later audit instead of being picked up as a scoped follow-up.

This is a distinct, narrower gap than the one closed by #965 (`OBL-D3-NEW-01`):
that fix landed `WCTR`/`NAM0`/`NAM9`/`NAM2`/`PNAM`/`ICON`/`DATA` etc., but its
own suggested-fix text explicitly deferred `OFST` ("a separate consumer... can
land later") and never mentioned `NAM3`/`NAM4` at all. Both fields remain
unparsed in the current `crates/plugin/src/esm/cell/wrld.rs` dispatch.

## Evidence

- `docs/engine/exal.md` §5.4: "What runtime LOD **does** need that we don't
  parse yet ... the WRLD `NAM3`/`NAM4` LOD-water fields + `OFST` cell-offset
  table currently skipped in `wrld.rs`."
- `crates/plugin/src/esm/cell/wrld.rs` WRLD-record sub-record match arm (grep
  for `NAM3`/`NAM4`/`OFST` → 0 hits in the parser; only mentioned in comments
  elsewhere, e.g. `crates/plugin/src/esm/cell/mod.rs:782-783`).
- Fresh dedup pull (`gh issue list` + `gh search issues`) found no open issue
  matching `NAM3`/`NAM4`/`OFST`/"LOD-water"; the closest hit, #965, is CLOSED
  and its own body defers `OFST` rather than closing it.

## Impact

None at runtime today — distant LOD water is not modelled, so there is no
regression to fix. Purely a tracking gap: the deferred parser work is
documented in the EXAL spec but has no issue, so a future audit or contributor
has to re-derive it from `exal.md` instead of picking up a scoped ticket.

## Related

- #1731 (`LC-D7-02`) — the sibling VWD-flag parser gap, which *is* tracked;
  this issue mirrors that shape for the LOD-water fields.
- #965 (`OBL-D3-NEW-01`, closed) — landed most of the WRLD record but
  explicitly deferred `OFST` and never covered `NAM3`/`NAM4`.
- `docs/engine/exal.md` §5.4, GameVariant §4 "Distant terrain source" row.

## Suggested Fix

Parse `NAM3` (LOD water type FormID), `NAM4` (LOD water height, f32), and
`OFST` (per-cell LAND offset table) in the WRLD sub-record dispatch in
`crates/plugin/src/esm/cell/wrld.rs`, surfacing them on the worldspace record
so a future LOD-water consumer can read them without a new parser gap. `OFST`
is a streaming-perf optimization only (not correctness-blocking) and can be
scoped separately from `NAM3`/`NAM4` if that's a smaller first cut.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (verify parity across Oblivion/FO3/FNV/Skyrim+ WRLD records — field presence/format may differ by game version)
- [ ] **TESTS**: A regression test pins this specific fix (parse a WRLD record with `NAM3`/`NAM4`/`OFST` present and assert the values are captured on the worldspace record)
