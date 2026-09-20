# EXT-D6-2026-09-19-03: probe_lod_corpus cannot open BA2 archives — blind for FO4/FO76/Starfield

- **ID**: EXT-D6-2026-09-19-03
- **Labels**: low,import-pipeline,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4502

**Severity**: LOW (tooling) · **Dimension**: Distant LOD (audit tooling) · **Tier Violated**: no-fabrication (the anti-fabrication tool fabricates "zero" for BA2 games) · **Game Affected**: FO4, FO76, Starfield
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D6-2026-09-19-03)

**Location**: `crates/bsa/examples/probe_lod_corpus.rs:15-20` (opens `BsaArchive::open`, which hard-rejects on `magic != b"BSA\0"` — `crates/bsa/src/archive/open.rs:26-31`)

**Description**
The probe's doc comment frames it as the full-corpus LOD census, but it can only open BSA: every BA2 (FO4/FO76/Starfield) silently prints `skip <path>`, so a reader of its output sees `_far.nif=0 distantlod=0` rows meaning "not opened", not "none" — precisely the #3321-class false-premise trap the probe was written to prevent. This audit's first corpus table was exactly such a misread before re-census via `ba2_grep`/`probe_substring`.

**Impact**
Misleading corpus measurements for all BA2 games; tooling-only.

**Suggested Fix**
Dispatch on the leading magic (`BSA\0` vs `BTDX`) to `BsaArchive`/`Ba2Archive`, or print an explicit "BSA-only tool" error instead of `skip`.

## Completeness Checks
- [ ] **TESTS**: Probe run over one archive of each family produces real counts or a loud error
