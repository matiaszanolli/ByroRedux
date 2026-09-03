# #2585 — SK-D5-LZ4-LOW-02: Post-decompression size-mismatch check is warn, not surfaced to any caller-visible metric

**Severity**: LOW · **Dimension**: BSA v105 (LZ4)
**Location**: `crates/bsa/src/archive/extract.rs`

## Resolution: no action required (per the issue's own text)

Re-verified against current source: the code still matches the issue's
description exactly — a declared/actual size mismatch after LZ4 frame
decode logs `log::warn!` (with codec/path/both sizes/signed delta) and
returns `Ok` regardless, mirroring the BA2 zlib path's identical
`debug`-level choice (this one deliberately bumped to `warn` per the
comment directly above it, "keeps the signal visible without breaking
parse-rate on borderline content").

The issue's own **Suggested Fix** and **TESTS** checklist both say this
explicitly: "None required now" / "N/A — deliberate design choice, no
action required." The issue exists purely as a documentation artifact —
per its own **Description**, "Recorded only because no `nif_stats`-style
counter exists for the BSA layer either, so a future audit doesn't have
to re-derive that this is intentional." Its own **Impact** section
confirms it is unexercised on real data (the full sweep produced zero
such warnings across all 65,637 files).

That documentation purpose is now served by this ISSUE.md and the closed
issue itself — no code change is warranted, and forcing one against an
issue that explicitly asks for none would be exactly the kind of
speculative fix this project's conventions caution against. Closing as
resolved-by-verification.
