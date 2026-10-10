# #5514: ESM-2026-10-09-D2-01: The WTHS reflection walker mis-decodes `DIFF` (index + inline value pairs ending in `0xFFFF`, read as a flat index list) and files `USRD` as unknown. It also duplicates `byroredux-sfmaterial`'s reflection reader

**Labels**: bug, esm-plugin, game:starfield, low

**Source**: `docs/audits/AUDIT_ESM_2026-10-09.md` — finding `ESM-2026-10-09-D2-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. The new public fields are wrong, but no runtime consumer reads them yet. The values half is explicitly deferred.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `WTHS` / `RDIF` (`DIFF`, `USRD` chunks)
- **Location**: `crates/plugin/src/esm/records/weather_settings.rs`:
  - `:30`: grammar doc `DIFF := type: u32, (field_index: u16)*`
  - `:234-240`: the decode
  - `:114-116`: the `diff_field_indices` doc
  - `:121-123`: the `unknown_chunks` doc, "Empty on vanilla data"
  - `:241-246`: the chunk match, which lacks `USRD`/`MAPC`
  - `:262`: a dead `let _ = GameKind::Starfield;`
- **Status**: NEW (#5364, closed, introduced it).
- **Description**: In the BGS reflection container, a `DIFF` chunk is an object diff: a class type ref followed by `(u16 field index, inline value)` pairs, terminated by `0xFFFF`. `byroredux-sfmaterial` already implements this (`crates/sfmaterial/src/reader.rs:829-846`, validated on the full vanilla `materialsbeta.cdb`; chunk kinds in `chunk.rs` include `USRD` "user object diff" and `MAPC`). The WTHS walker treats every `u16` after the type ref as a field index, so the value bytes and the terminator land in `diff_field_indices`.
- **Evidence** (`scripts/wths_diff.py`, `scripts/wths_usrd.py`, `Starfield.esm`):
  - In 163/163 `RDIF` `DIFF` chunks, at least one "index" is ≥ the root class's 25-field count, and every chunk ends in `0xFFFF`. Example, `Weather_Clear_C0_Clear`: `[0, 0xFF0D, 0xFFFF, 0x8397, 0x000E, 2, 0, 4, 0, 0xFFFF, 0xFFFF]`. Here `0x000E8397` is the parent FormID written inline as field 0's value.
  - 47 `USRD` chunks in 23 `RDIF` records (5 more in ShatteredSpace) go to `unknown_chunks`.
  - Chunk tallies match `BETH.chunk_count` on all 187 blobs, so the container framing itself is right.
- **Impact**: The documented meaning of `diff_field_indices` ("which fields of the parent's class the child overrides") is false. The real-data test checks only `.is_some()`. The planned values reader (#5364 follow-up) would start from a wrong grammar while a validated one already exists in the workspace, contrary to the global "improve existing code rather than duplicate logic" rule.
- **Related**: #5364, #5424 (closed). EXT-D1-2026-10-09-02 is the vacuous `resolve_default_weather` test on the consumer side; this finding does not duplicate it.
- **Suggested Fix**: Parse `REFL`/`RDIF` through `byroredux-sfmaterial`'s reader, factoring out a reflection-container entry point if needed. Failing that, decode `DIFF`/`USRD` as index + value pairs to the `0xFFFF` terminator, using the CDB reader's skip rules. Fix the two docs and drop the dead line.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
