# ESM-D4-02: FO4 and Starfield nest the whole dialogue/scene tree under QUST, and the `_ =>` arm discards it

**Issue**: #2908
**Severity**: HIGH
**Labels**: bug, import-pipeline, high, legacy-compat
**Source report**: `docs/audits/AUDIT_ESM_2026-08-13.md` (`/audit-esm` Dimension 4)

---

**Dim 4** · `QUST`/`DIAL`,`INFO`,`SCEN`,`DLBR` · `dispatch_misc_gameplay_a.rs:99-107`, `grup_walker.rs:59-81`

`extract_records(reader, end, b"QUST", …)` recurses into child GRUPs but
`skip_record`s anything that is not a `QUST`. Oblivion→Skyrim ship separate
top-level `DIAL`/`SCEN` GRUPs; FO4 has **neither label at all**, and Starfield's
are present but empty.

```
FO4  GROUP QUST: {DIAL: 35,443, DLBR: 132, INFO: 78,087, QUST: 1,336, SCEN: 3,568}
SF   GROUP QUST: {DIAL: 68,154, DLBR:  79, INFO: 126,347, QUST: 2,077, SCEN: 7,613}
SF   GROUP DIAL: {}   SF GROUP SCEN: {}
```

**`index.dialogues` and `index.scenes` are 0 on FO4 and Starfield** — 117,230 /
202,193 records dropped inside a group the walker reports as handled, which is
why the top-level-only metric in `AUDIT_STARFIELD_2026-08-12` Dim 4 cannot see it.
The M47 quest/scene runtime and INFO topic resolution are inert on both games.
**Fix**: give `QUST` a multi-type walker (same shape as `extract_dial_with_info`),
or generalise `extract_records` to a `&[(&[u8;4], handler)]` table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record parsers, the other `merge_from`, the sibling walker)
- [ ] **REMAP**: If a FormID is read, it goes through `reader.remap_form_id` — raw plugin-local ids must never reach a globally-keyed map (see ESM-D3-01/02/03)
- [ ] **REAL-DATA FIXTURE**: Any byte-layout change is pinned by a verbatim payload lifted from a shipped master (plugin + FormID + EditorID in a comment), never a synthetic fixture built from the parser's own assumption (see ESM-D2-07)
- [ ] **MERGED-PATH TEST**: If the fix touches `EsmIndex`, at least one test goes through `parse_record_indexes_in_load_order`, not just `parse_esm` (see ESM-D4-01)
- [ ] **TESTS**: A regression test pins this specific fix
