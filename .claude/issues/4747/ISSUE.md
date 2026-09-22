# AUD-2026-09-21-D5-06: Status docs present REGN background music as shipped with no #3816 qualifier; ROADMAP misattributes reverb ordering

**Issue**: #4747
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW
**Dimension**: 5 — Engine Consumers
**Location**: `docs/feature-matrix.md:156`, `:148`; `ROADMAP.md:837`, `:1267`

## Description
`feature-matrix.md`'s REGN-background-music row (`a924244ee`, predates #3787/#3811/#3816) shows `✓` with no qualifier, but `music_form` cannot resolve as a `SOUN` on any supported game until #3816 (open) lands MUSC/MUST/MSET/RDMD decode — the mechanism ships and is tested, but plays nothing in production today. ROADMAP's M44 row still says `reverb_zone_system` is "registered in `boot.rs` ahead of `audio_system`," restating the registration-order premise #4146 (closed) already removed from the code comment in favor of the actual parallel-vs-exclusive stage-membership guarantee. The "cache" in "BSA WAV decode + cache ✓" refers to `SoundCache`, which no engine code currently installs (see #4739).

## Evidence
`git blame -L 156,156 docs/feature-matrix.md` → `a924244ee`. #3816 confirmed OPEN. Neither doc file changed since the report's own HEAD.

## Impact
Documentation only, but these are the repo's two authoritative status references.

## Related
#3816 (open); #3523, #3088 (closed, earlier drift on same rows); #4146 (closed, the code fix ROADMAP still contradicts); #4739 (this report, the unused `SoundCache`).

## Suggested Fix
Mark the REGN row "mechanism ✓, plays nothing until #3816" in both docs. Replace the ROADMAP reverb sentence with the parallel-vs-exclusive mechanism. Qualify/drop "cache" until an engine `SoundCache` installer exists.
