# ESM-D2-01: New decoders added bare numeric gates, offsets and raw-index reads that #6586 named elsewhere

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6781
Labels: low, esm-plugin, bug, tech-debt
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `XCLL` / `LGTM.DATA` height-fog tail; WEAP FO3/FNV `DNAM`; `LIGH.DATA`; `LIGH.WGDR`; PERK Starfield `DATA`; WTHR `HNAM` / `WGDR`
- **Location**:
  - `crates/plugin/src/esm/cell/lighting.rs:147` (`< 128`), `:150` (`&body[92..]`), `:160` (`>= 136`)
  - `crates/plugin/src/esm/records/misc/world.rs:1519-1520` (`>= 92`, `skip_or_eof(32)`)
  - `crates/plugin/src/esm/records/items.rs:463` (`skip_or_eof(15)`), `:483` (`get(40)`), `:489` (`get(104..108)`), `:499` (`skip_or_eof(44)`)
  - `crates/plugin/src/esm/cell/support.rs:196-237` (private `read_f32` closure over `sub.data[off]`, `read_f32(24)` for Near Clip), `:369-372` (`WGDR` as `u32::from_le_bytes([sub.data[0], ...])`)
  - `crates/plugin/src/esm/records/misc/magic.rs:506` (`sub.data[sub.data.len().min(4) - 1]`)
  - `crates/plugin/src/esm/records/weather.rs:1300`, `:1303` (`sub.data.len() >= 16`)
- **Status**: NEW (the Dim 2 checklist's "a new bare numeric gate or gap chain is the regression")
- **Description**: #6586 (`004a03fc3`) moved the `PROJ` / `EXPL` / `PKDT` / `AOR2` / `COBJ` gates into named constants and added `SubReader::skip_to_or_eof` for layout seeks. The decoders added after it did not follow. The height-fog tail restates the 92 / 128 / 136 sizes that `cell/walkers.rs:67` already names as `XCLL_SIZES_FALLOUT4 = [28, 92, 128, 136]`, so one layout is now spelled in two files. WEAP DNAM `skill_actor_value` (offset 104, added with the `Skill` decode) and `vats_base_to_hit` (40) are raw `.get()` reads inside an arm that otherwise uses a `SubReader` with `skip_or_eof(15)` / `skip_or_eof(44)` hand-summed gaps. The LIGH `DATA` decode is a private raw-index closure, and the new Near Clip read inherits it.
- **Evidence**: every width and offset listed above is correct against xEdit, so this is not a decode bug:
  - FO4 `XCLL`/`LGTM` tail: 88 Inherits, 92 Near Height Mid ... 124 Fog High Far Scale, 128/132 Far Height (FO4.pas:9764-9774, :12483-12493), colours are 4-byte `wbByteColors` (`rgb_color` consumes 4).
  - FNV WEAP `DNAM` {40} Base VATS To-Hit, {68} Override AP, {104} Skill `itS32` (`wbDefinitionsFNV.pas` WEAP).
  - TES5/FO4 LIGH `DATA` Near Clip at 24.
  - SF1 `PERK` `DATA` Flags is the last byte, crew byte `wbFromSize(4)`.
- **Impact**: none today. The risk is the one #6586 closed: an offset table and the read order drifting apart (the `skip_or_eof(15)` + `skip_or_eof(44)` chain is exactly that shape).
- **Suggested fix**: name `XCLL_HEIGHT_FOG_TAIL_AT = 92`, `_NEAR_LAYER_LEN = 128`, `_FAR_LAYER_LEN = 136` once (re-use for the `XCLL_SIZES_FALLOUT4` table and the LGTM gate). Name `WEAP_DNAM_VATS_TO_HIT` / `_SKILL` and convert the arm to `skip_to_or_eof`. Replace the `WGDR` hand `from_le_bytes` with `SubReader::new(&sub.data).u32()`. Add `WTHR_TOD_FORM_LIST_MIN_LEN = 16`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
