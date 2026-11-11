# FO4-2026-11-10-D4-01: `RecordHeader::form_version` doc claims FO3/FNV carry none, but the reader returns their real form versions

Labels: low,documentation,doc-rot,esm-plugin,game:fo4,legacy-compat
Source: docs/audits/AUDIT_FO4_2026-11-10.md

- **Severity**: LOW (doc-rot; no behaviour impact today)
- **Dimension**: Dim 4, shared ESM walker. **Owner: `/audit-esm`** (shared mechanism, not an FO4 decoder).
- **Location**: `crates/plugin/src/esm/reader.rs:655-658` (doc), `:911-922` (reader).
- **Status**: NEW. No open or closed issue matches (`gh` search for the header field and `form_version`; no title hits in `issues.json`).
- **Description**: 02909f1ee (#6658) added `form_version` to `RecordHeader`. The doc says "`0` on Oblivion, FO3 and FNV, whose header carries none". The code reads the `u16` at offset 20 whenever `header_size >= 24`, and FO3 / FNV use a 24-byte header. So only Oblivion (20-byte header) yields 0.
- **Evidence**: raw census of the first 3 MB of `FalloutNV.esm`. The u16 at record offset 20 is 15 on 6,099 records, 14 on 347, 13 on 88, 9 on 87, 1 on 219. Fallout4.esm records read as expected, 63 to 131.
- **Impact**: none yet. The only consumer is the `PSDT` month offset, gated on `form_version >= 122`, which no FO3/FNV record reaches. A future "if form_version == 0 then pre-Skyrim" test would be wrong. Not an FO4 behaviour issue.
- **Related**: #6658, `PackMonth::from_stored`.
- **Suggested Fix**: reword the doc to "0 on Oblivion; on FO3 / FNV the same 2 bytes hold a small value (13-15) that nothing keys on", or zero the field for pre-Skyrim variants in the reader.
