# #7005: UI-D3-2026-11-15-02: `docs/feature-matrix.md` and `docs/COMPATIBILITY.md` still quote the July catalog sizes (74 / 138)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/7005
- **Labels**: low,ui,documentation,doc-rot
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: Catalog & AVM1 Scanner
- **Profile**: both
- **Existing**: none (#2729 / #3153 were different rows).
- **Location**: `docs/feature-matrix.md:278` ("74-method catalog + request routing shipped"), `docs/feature-matrix.md:279` ("138-method installed-corpus catalog"), `docs/COMPATIBILITY.md:298` ("74 SkyUI methods + 12 request contracts"), `docs/COMPATIBILITY.md:299` ("138 installed-corpus methods").
- **Evidence**: HEAD catalogs are 142 (16 requests) and 269. `git log -S"74-method catalog" -- docs/feature-matrix.md` returns only 631765c79 (2026-07-25), so the row was never updated for #2966 (269) or #3103/#4720/#5274 (142, 16 requests). `ui.md:18-20`, `ui.md:695-696` and `ROADMAP.md:308` carry the right numbers (142 / 269 / 16 requests), so these are the only compatibility-table sites still stale.
- **Impact**: the two tables a reader checks for per-game support understate both catalogs by roughly half; the Skyrim row also misses that the catalog is now corpus-measured.
- **Suggested Fix**: update the four cells to 142 methods (16 request contracts, all measured) and 269 methods. Point them at `ui.md#scaleform-host` (or drop the number and link) so the next regeneration cannot strand them again.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
