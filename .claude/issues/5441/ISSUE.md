# #5441: NIF-D3-2026-10-08-04: the FO76 parse-rate docs still say 102,968 NIFs over "4 of the 20" archives; the gate walks 5 archives and 102,980 NIFs

**Labels**: low,nif-parser,nif,documentation,doc-rot,game:fo76
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5441

**Source**: `docs/audits/AUDIT_NIF_2026-10-08.md` — `NIF-D3-2026-10-08-04` (HEAD `00f580e09`)

- **Severity**: LOW (doc-rot)
- **Dimension**: Block Dispatch Coverage (reported coverage)
- **Game Affected**: Fallout 76
- **Location**:
  - `ROADMAP.md:251`, `:757` (including the cumulative 562,057);
  - `docs/engine/game-compatibility.md:95`, `:345`;
  - `docs/engine/nif-parser.md:676`, `:678` (cumulative 562,057).
- **Status**: NEW. #5080 (closed) refreshed these rows from the 2026-09-29 measurement, but #4628 had already widened the gate with `SeventySix - Startup.ba2` (12 NIFs).
- **Description**: today's `parse_rate_fo76_all_meshes` reports "5/5 archive(s) present, 102980 NIFs", which matches the baseline's 102,980. The docs carry the pre-#4628 4-archive figure, and the cumulative total is off by the same 12 NIFs: it should be 562,069.
- **Impact**: cosmetic. Readers get a corpus size and archive list that the gate no longer walks.
- **Related**: #5080, #4628.
- **Suggested Fix**: in the next `/session-close`, update the FO76 rows to 102,980 over 5 archives (Meshes, StaticMeshes, GeneratedMeshes01/02, Startup) and the cumulative to 562,069.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (ROADMAP.md, game-compatibility.md, nif-parser.md FO76 rows + cumulative totals)
