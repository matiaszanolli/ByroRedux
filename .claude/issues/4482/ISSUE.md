# EXT-D7-2026-09-19-01: ground-cover eval harness re-mints washed-out reference frames with exit 0

- **ID**: EXT-D7-2026-09-19-01
- **Labels**: high,terrain-exterior,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4482

**Severity**: HIGH (harness credibility — the workaround for "unit guards cannot see GPU output" is itself silently fragile) · **Dimension**: Acceptance harness · **Game Affected**: FNV, Skyrim SE
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-01)

**Location**: `scripts/renderer-eval-groundcover.sh:119-131`; protocol exists only in the audit skill prose; `docs/engine/exal-groundcover.md` §11.5 lacks it

**Description**
The 2026-09-15 washout produced a protocol — measure luminance std first (below ~10/255 = no scene contrast), treat only `gc-backlit-*` poses as ground-framing, confirm the veil is gone before trusting an A/B. That protocol is encoded nowhere in the script: its sole hard check is `[[ ! -s "${png}" ]]`. It still captures all four poses unconditionally, hashes them into `manifest.tsv`, and exits 0.

**Evidence**
Measured with the repo's own ImageMagick invocation: the committed `gc-backlit-*` references at `00d4ef5d0` measure sd 0.0092–0.0106 (≈2.3–2.7/255) — the ground-framing poses in the accepted baseline ARE the veil (frontlit frames measure sd 0.077–0.090). Even `m-exteriors.sh`'s `image_health` sd floor (>0.005) would pass them.

**Impact**
Every ground-cover visual claim (tier cross-fade #4056 acceptance, palette, wind A/Bs) is judged through a harness that cannot distinguish its known-bad baseline from a fixed one; an A/B over washed frames reports "no delta" for a real fix.

**Related**: EXT-D7-2026-09-19-03, #4056, #3807

**Suggested Fix**
Add the luminance-std probe to `capture()` (fail, or stamp a `washed_out=yes` manifest column, when a `gc-backlit-*` frame lands under 0.039 sd); record the protocol in exal-groundcover.md §11.5 so it survives outside the audit skill.

## Completeness Checks
- [ ] **SIBLING**: Apply the same sd precheck to any future eval script that mints sha256 "reference" baselines
- [ ] **TESTS**: Re-mint the `gc-backlit-*` references once the veil fix lands, with sd recorded in the manifest
