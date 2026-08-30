# #3612 — REN-2026-08-30-D16-06: ROADMAP's M58 row does not record that bloom shipped, and the shaders' deferral has no tracking home

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3612 --json state`.

---

- **Severity**: Low
- **Dimension**: Bloom
- **Location**: `ROADMAP.md:812` (M58 row); referenced from `crates/renderer/shaders/bloom_downsample.comp:16` and `crates/renderer/shaders/bloom_upsample.comp:15`
- **Status**: OPEN — new
- **Description**: Both shipped bloom shaders explicitly defer the
  Jimenez/Kawase filter upgrade and say *"See the M58 row in ROADMAP.md for
  tracking"* / *"Upgrade target tracked in ROADMAP.md M58 row"*. The M58 row
  sits unannotated in the *planned*-milestone table and describes
  `Kawase-blur bloom (5-pass dual filter, ~2 ms total)` as future scope — it
  records neither that the bloom sub-slice shipped (Session 33, `33f48b5`,
  `HISTORY.md:2609`) nor the box-filter-for-now decision the shaders point at.
  The neighbouring M55 row *does* carry exactly this treatment
  (**"Fog slice shipped 2026-07-26→08-01 (Session 62)…"**), so the convention
  exists and M58 was simply missed.
- **Evidence**:
  - `crates/renderer/shaders/bloom_downsample.comp:10`–`16` — the box-vs-13-tap rationale and the ROADMAP pointer
  - `crates/renderer/shaders/bloom_upsample.comp:13`–`15` — same pointer
  - `ROADMAP.md:812` — the row, with no shipped annotation and no mention of the deferral
  - `ROADMAP.md:809` — the M55 row's shipped-slice annotation, the pattern to copy
  - `HISTORY.md:2609` — `33f48b5` "M55 volumetrics + M58 bloom + M-LIGHT v1"
- **Impact**: A dangling cross-reference: two shipping shaders cite a tracking
  location that tracks nothing, so the deliberate box-filter decision (and
  the composed 5× pyramid DC gain documented at `bloom_upsample.comp:18`–`35`,
  which the eventual re-tune must account for) is recorded only in shader
  comments. Whoever picks up M58's remaining scope has no signal that the
  bloom slice is already live and that `BLOOM_INTENSITY` was tuned against an
  unnormalised pyramid.
- **Suggested Fix**: Annotate the M58 row in the M55 style: bloom slice
  shipped Session 33, current filter is a 4-tap box down / 4-tap box + add up
  over `BLOOM_MIP_COUNT = 5`, add site relocated after composite by #2796,
  remaining M58 scope = Jimenez 13-tap/9-tap (needs the SIGGRAPH 2014 slides
  in-repo per the no-guessing rule) + DOF + motion blur + 3D-LUT grading +
  AgX/Tony McMapface, and note that the intensity re-tune is coupled to the
  pyramid's DC gain.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D16-06

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
