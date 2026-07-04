# #1820 — SPT-NEW-01: detect_variant / SpeedTreeVariant are dead code — no production or test consumer

**Severity**: LOW · **Domain**: import-pipeline (`byroredux-spt`)
**Location**: `crates/spt/src/version.rs:90-100` (`detect_variant`), `:24-49`
(`SpeedTreeVariant` + impl), `crates/spt/src/lib.rs:61`

`detect_variant` and `SpeedTreeVariant` are re-exported from `lib.rs` but have
zero call sites outside `version.rs`'s own unit tests and one
`#[cfg(feature = "recon")]` dev tool (`examples/spt_dissect.rs:63`). The
production `parse_spt` independently re-validates `MAGIC_HEAD` via
`bytes.starts_with(...)` (`parser.rs:48`) and never consults `detect_variant`;
the placeholder importer is variant-agnostic. Confirms the Dimension-4
checklist expectation — nothing downstream depends on the variant being
correct — so the documented `V5Fnv` default for every `__IdvSpt_02_` file
(including Oblivion 4.x) is benign.

Impact: none at runtime. Maintenance only — the API reads as a live per-game
dispatch hook but is inert, which can mislead a contributor into "fixing" the
`V5Fnv` default or wiring it where the per-REFR route already works.

Related: distinct from SPT-NEW-03 / #1711 (that is `bs_bound`, a different
field).

Suggested fix: either wire `detect_variant` into the cell-loader `.spt` route
as a logged sanity check, or mark it `#[allow(dead_code)]` with a "reserved
for Phase 2 variant dispatch" note.

Completeness checks called out in the issue:
- SIBLING: check for other per-game dispatch hooks in `crates/spt` wired at design time but never connected to a caller
- TESTS: if wired into the cell-loader route, add a test asserting the logged sanity check fires; if marked `#[allow(dead_code)]` instead, no test needed

---

# #1821 — SPT-NEW-06: format-notes.md's 14000-band tail-tags worked example doesn't byte-align with where the live walker bails

**Severity**: LOW · **Domain**: import-pipeline (`byroredux-spt`, docs-only)
**Location**: `crates/spt/docs/format-notes.md:588-609` ("Open: 14000-band tail
tags in the 4 outliers")

The doc's worked example attributes the 4 Oblivion outliers' second bail to
tag `14007` (out of `TAG_MAX = 13999`) at an eyeballed hex offset, dismissing
the walker's actually-recorded value `768` as a mis-decode. But the walker's
deterministic byte accounting genuinely lands the next tag read on the
`00 03 00 00` (= 768) u32 — the prior entry `tag=13013` (`FixedBytes(7)`)
consumes exactly 4 + 7 = 11 bytes, placing the cursor at the offset that
reads `768`, not 3 bytes earlier where `14007` sits. `SptScene::unknown_tags
== [(768, …)]` in the live corpus run matches the walker's real cursor, not
the doc's hex table.

Evidence: live corpus run reports `tag=768 (0x0300)` at offsets 4507 / 5641 /
5946 / 6211 for the 4 outliers — exactly the walker's own byte math, which
contradicts the doc's `14007`-at-a-different-offset narrative.

Impact: documentation-only; parser behaviour is correct and the acceptance
gate is unaffected. Forward-looking risk: the doc recommends a "re-run with
TAG_MAX = 16000, extend dispatch_tag" follow-up anchored on this worked
example — a contributor adding a `14007` arm from the doc's byte table would
find the walker never reads `14007` there, silently wasting the
dictionary-expansion attempt.

Related: SPT-D1-01 / #999 (the 13005 fix this doc section chronicles the
follow-up to; that disambiguation itself is confirmed correct).

Suggested fix: re-run `spt_recon`/`spt_dissect` starting from the walker's
actual `tail_offset` (not a manual hex-dump position) and correct the worked
example's offset/value pairing before anyone acts on the "extend TAG_MAX"
follow-up.

Completeness checks called out in the issue:
- SIBLING: check the rest of `format-notes.md`'s worked-example byte tables for the same eyeballed-vs-actual-cursor drift
- TESTS: the corrected offset/value pairing quoted in the doc is reproduced from a live `--ignored` corpus run before committing the doc edit
