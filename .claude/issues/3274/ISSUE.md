# 3274: AUD-2026-08-24-D6-01: REGN ambient music shipped but status docs still mark it unbuilt; test count drifted a 4th time (regression of #3088)

**Severity**: LOW · **Report**: `docs/audits/AUDIT_AUDIO_2026-08-24.md` (AUD-2026-08-24-D6-01)

## Description

`ede48ffb` + `3ef05d1b` (2026-08-23) shipped REGN ambient background-music dispatch — a full FormID → archive path → streaming decode → `AudioWorld::play_music` pipeline. None of the three authoritative status sources reflect it:

1. `crates/audio/src/lib.rs:129-138` — "Future work" list still names REGN ambient soundscapes as unbuilt.
2. `docs/feature-matrix.md:146` — `| Region ambient (REGN) | ✗ |`, flatly wrong; music dispatch is `✓` (partial).
3. `ROADMAP.md:705`'s M44 row — same "pending" framing.

**Also a regression of #3088** (CLOSED): the row's test-count sentence (refreshed by #3088's fix the same day it was measured) has already drifted again — `systems/audio.rs` shows 12 tests, ROADMAP says 11; `asset_provider/audio.rs`'s 13 tests are never counted at all. True total: 60, not ROADMAP's "46".

## Location

`crates/audio/src/lib.rs:129-138`, `docs/feature-matrix.md:145-146`, `ROADMAP.md:705`, `byroredux/src/systems/audio.rs`, `byroredux/src/asset_provider/audio.rs`

## Impact

Documentation only. A contributor scoping "REGN ambient" from `feature-matrix.md` alone would re-derive a shipped feature from scratch or report its absence as a gap.

## Related

#3088 (CLOSED, test-count regression), #3181 (same doc-drift class, just fixed for water).

## Suggested Fix

Split the docstring's REGN bullet into "shipped (music)" + "pending (incidental/sounds)"; flip `feature-matrix.md:146` to `~ Partial`; update ROADMAP's pending clause and refresh the test-count sentence to 12/60 (or drop specific numbers in favor of the sentence's own advice to run the `cargo test` commands).

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix
