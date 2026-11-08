# ESM-D2-02: FormID-array sub-records are hand-chunked in four places; the trailing remainder is silently dropped

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6782
Labels: low, esm-plugin, bug, tech-debt
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `MUSC.TNAM`, `MUST.SNAM`, `WTHR.HNAM` / `WGDR`, every `KWDA`
- **Location**:
  - `crates/plugin/src/esm/records/misc/music.rs:92` and `:180` (`chunks_exact(4)` + `chunk.try_into().unwrap()`)
  - `crates/plugin/src/esm/records/weather.rs:734` (`parse_tod_form_list`)
  - `crates/plugin/src/esm/records/common.rs:367` and `:467` (`kwda_keywords`, `.expect("four-byte chunk")`)
- **Status**: NEW (the music and weather copies landed in `03f9d2158` and `3bfcbd58b`; the KWDA pair pre-dates them)
- **Description**: The Dim 2 text names "an `unwrap` on a slice-to-array in a decoder" as the regression `e04caa7e8` (#6367) removed, and asks repeating rows to check `len / stride` with an explicit remainder. These loops are infallible (`chunks_exact` guarantees four bytes) but each is a separate copy of the same read. None reports a non-zero remainder: `parse_tod_form_list` documents "a trailing partial word is ignored" and `kwda_keywords` has a test asserting it. The two music loops also `filter(!= 0)`, which is right for a track list but is a different NULL policy from the time-of-day list (which keeps `None` slots because position is meaningful).
- **Evidence**: real data is clean, so no live failure. Skyrim.esm: 50/50 `MUSC.TNAM` and 13/13 `MUST.SNAM` are multiples of 4. Skyrim Update.esm WTHR `HNAM` is 16 B on 69/69 records. Fallout4.esm WTHR `WGDR` is 32 B on 63/63. SeventySix.esm WTHR `HNAM` is 32 B on 123/123. xEdit types: `wbArray(TNAM/SNAM, wbFormIDCk)`, `wbStruct(HNAM/WGDR, 4 or 8 x wbFormIDCK)`.
- **Impact**: a truncated or corrupt array decodes as a shorter valid one instead of declining. The same read exists four times.
- **Suggested fix**: one `SubReader::form_id_array()` (or a `common.rs` helper) that errors on a non-zero remainder, taking the `remap`. Route the four sites through it and keep the NULL policy at the call site.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
