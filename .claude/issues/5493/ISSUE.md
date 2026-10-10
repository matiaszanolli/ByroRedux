# #5493: ESM-2026-10-09-D3-01: SCOL/PKIN `FLTR` is a zstring (object-window filter path), but both parsers decode it as a `u32` FormID array and remap every 4-byte slice. The result is garbage FormIDs plus about 2.8k (FO4 + DLCs) and 3.9k (Shattered…

**Labels**: bug, esm-plugin, game:fo4, game:starfield, medium

**Source**: `docs/audits/AUDIT_ESM_2026-10-09.md` — finding `ESM-2026-10-09-D3-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. Same class and scale as ESM-2026-09-29-D2-01 (the TRDA `FFFF` sentinel, MEDIUM, about 5.7k warnings). The decoded value has no consumer.
- **Dimension**: FormID Remap, Load Order & ESL Space (inverse shape 4)
- **Record / Sub-record**: `PKIN` / `FLTR`, `SCOL` / `FLTR`
- **Location**:
  - PKIN: arm at `crates/plugin/src/esm/records/pkin.rs:130-150`; docs at `:34-40` and `:79-84`.
  - SCOL: arm at `crates/plugin/src/esm/records/scol.rs:191-210`; doc at `:23` ("filter form IDs").
  - Callers: `crates/plugin/src/esm/cell/support.rs:802-803`, `:891-892`. Both pass `reader.get_form_id_remap()`.
- **Status**: NEW. The PKIN `u32` decode came from #815 (closed), the SCOL one from #405 (closed), and #3400 (closed) added the remap.
- **Description**: xEdit defines `wbFLTR := wbString(FLTR, 'Filter')` (`FO4.pas:5274`, `FO76.pas:7040`, `SF1.pas:5697`). The quest parser already reads its own `FLTR` as a zstring (`misc/quest.rs:553`). SCOL and PKIN instead read `len / 4` little-endian `u32`s and call `remap_fid` on each. On a plugin with masters, `FormIdRemap::remap` (`esm/reader.rs:577-585`) sends any top byte above the master count to its "genuinely suspicious" `warn!` arm. ASCII path bytes always qualify.
- **Evidence** (`scripts/fltr.py`, `scripts/fltr_warn.py`):
  - `Fallout4.esm`: 2,244 SCOL and 230 PKIN `FLTR`s, every one NUL-terminated text (`SetDressing\IndustrialMachines\`, `DummyObjects\`, `\lights\`).
  - Would-be warnings (slice mod-index > master count):

    | plugin | warnings | slices composed into a "live" global id |
    |---|---|---|
    | DLCRobot | 739 | 20 |
    | DLCworkshop01 | 15 | 0 |
    | DLCCoast | 813 | 50 |
    | DLCworkshop03 | 25 | 0 |
    | DLCNukaWorld | 1,251 | 84 |
    | **FO4 DLC total** | **2,843** | |
    | ShatteredSpace | 3,857 | 154 |
- **Impact**:
  - Every FO4 DLC or Shattered Space load order floods the log with false "malformed file" warnings, which buries real remap failures (the #4172 / #5075 concern).
  - `PkinRecord::filter` / `ScolRecord::filter` hold string bytes posing as FormIDs. A future workshop-filter consumer would resolve garbage.
  - The tests pin the wrong shape: `pkin.rs:265`, `:281`, `:331` and `scol.rs:340`, `:498`.
- **Related**: #815, #405, #3400 (all closed); ESM-2026-09-29-D2-01/D2-02 (#5075/#5076, same class).
- **Suggested Fix**: Decode `FLTR` with `read_zstring` into a `filter: String` on both records, cite the xEdit line, and drop the `remap_fid` calls. Re-pin the tests on real string payloads. Optionally add a DLC fixture test asserting no remap warning.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
