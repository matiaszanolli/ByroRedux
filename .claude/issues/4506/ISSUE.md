# EXT-D7-2026-09-19-06: test-gap ledger — five exterior claims whose final acceptance is human-only

- **ID**: EXT-D7-2026-09-19-06
- **Labels**: low,terrain-exterior,bug,test-gap
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4506

**Severity**: LOW (test-gap ledger; per-claim impact noted in the audit) · **Dimension**: Acceptance harness · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-06)

**Location**: see per-claim evidence below

**Description**
Five Dim 3/4/5 claims read as "gated" in the specs but their final acceptance is a human looking at a frame:
1. **Tier cross-fade** (ground cover): unit guard pins buffer slabs, not the fade; the eval harness captures only static poses; #4056 acceptance pending.
2. **FSR reactive/linear-compression masks** (#4297): the guard is shader-text `contains` scanning only; `m-exteriors.sh:220` hardcodes `--upscaler taa` in every mode, so ground-cover masks are never exercised against the real upscaler on a live frame.
3. **Cloud march rendering** (coverage → density, WTHR layer mips): unit tests pin source shape; cycle mode image-healths PNGs but checks no cloud invariant.
4. **HNAM `sunlight_dimmer` multiply**: unpinned (EXT-D4-2026-09-19-03) and no harness gates it.
5. **Waterline delta oracle** (`m-exteriors.sh:535-545`): near-vacuous — a >0.01 full-frame difference between captures 250-450 units apart with different look angles; any scene change clears it.
COVERED, not a gap: sentinel water is gated twice (unit guard + harness FLT_MAX/INT_MIN hard fail).

**Related**: EXT-D7-2026-09-19-01/04; #4056, #4297

**Suggested Fix**
Prioritise the dimmer multiply unit test (one test away) and a backlit-pose luminance floor in the ground-cover eval; the rest can stay manual but should say so at the claim site (specs §11.x / skyal / watal).

## Completeness Checks
- [ ] **TESTS**: Items 1-5 each get a gate or an explicit "manual acceptance" note at the claim site
