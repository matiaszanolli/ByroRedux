# Issue #1822: SPT-NEW-07 — MaybeStringElseBare (tag 13005) can misparse a bare 13005 before the geometry tail

**Severity**: LOW · **Domain**: spt (SpeedTree parser, `byroredux-spt`)

`crates/spt/src/parser.rs:84-120`, the `MaybeStringElseBare` arm for tag 13005:
consumes the tag's u32, peeks the NEXT u32, and treats the entry as `Bare` iff
that peeked value is a known dictionary tag. If a bare 13005 is the last
parameter entry and is immediately followed by the binary geometry tail (an
out-of-range u32), the peek sees a non-tag value → takes the String branch →
`read_string_lp()` reads that tail u32 as a byte length and consumes that many
bytes of the geometry tail as a bogus string. No `unknown_tags` diagnostic is
recorded, so the corpus harness would score the file "clean" despite desync.

Live corpus (113 Oblivion files): 0 files trigger this today (bare 13005 is
always followed by a known tag in vanilla data) — latent gap, not an active
regression. Existing test `tag_13005_at_eof_does_not_panic` only covers the
EOF-immediately-after case, not "13005 followed by non-EOF out-of-range tail".

**Suggested fix**: gate the String branch on the peeked value being a
plausible length (`< remaining_bytes` and below a sane ceiling — corpus max
~525B, the 64KiB cap is far too loose) rather than just "not a known tag".
Alternative: require the read bytes to be printable-ASCII (BezierSpline blobs
always are) and fall back to `Bare` otherwise.

**Completeness**: check other `MaybeStringElseBare`-style bimodal-tag arms for
the same tail-swallow direction (only one exists today per issue text); add a
regression fixture — bare 13005 immediately followed by an out-of-range tail
u32 resolves as `Bare` with `tail_offset` at the 13005 successor.

---

# Issue #2342: FO4-M49-D1-01 — Stale exterior-absorption comment in wrld.rs

**Severity**: LOW · **Domain**: esm (`byroredux-plugin`), doc-only

`crates/plugin/src/esm/cell/wrld.rs:493-503`'s `CellData` construction comment
still claims exterior precombine absorption is dormant pending `#1221`. That
landed in `1ed8dc0b`; `0ace5caf` ("docs(cell-loader): correct stale pre-M49
precombine comments") fixed the equivalent comments in
`byroredux/src/cell_loader/{exterior,load,precombined}.rs` but missed this
plugin-crate copy. No runtime effect — comment-only, risk is future
misdiagnosis.

**Suggested fix**: update the comment to reflect that #1221/#1222 landed —
exterior cells invoke the precombine spawn pass and honor the
conditional-absorption gate identically to interior cells.

**Completeness**: confirm no other plugin-crate copies of this stale comment
exist. TESTS: N/A.

---

# Issue #2348: OBL-D7-01 — README.md still frames Oblivion exterior as wiring-gated

**Severity**: LOW · **Domain**: documentation only (README.md)

`README.md:129-130` (present tense) reads "Oblivion exterior gated on TES4
worldspace + LAND wiring" — implying the wiring is still the blocker.
`ROADMAP.md:349,424` is explicit the wiring is done ("implemented and
game-agnostic... only an on-device exterior render bench is pending").
Direct textual contradiction between the two docs.

**Suggested fix**: reword README.md:129-130 to match ROADMAP.md's framing:
"Oblivion exterior: worldspace/LAND wiring implemented, on-device render
bench pending."

**Completeness**: check README.md for the same stale-framing pattern on other
per-game exterior/milestone claims ROADMAP.md has since updated.

---

# Issue #2369: EX-14/15 — Stream ground cover, persistent refs, parent worlds, and FO4 spatial data

**Severity**: MEDIUM · **Domain**: renderer + import-pipeline + terrain-exterior (EXAL)

Plan-shaped issue (not a scoped bug): GRAS/REGN placement + full SpeedTree
tree rendering replacing billboard-only coverage, deterministic/streamed/
deadline-budgeted density, correct persistent/temporary ref ownership across
parent worldspaces and boundary crossings, FO4 precombine/previs/occlusion
render+collision+fallback+mod-invalidation coverage, no double geometry
between absorbed refs and precombined meshes, and soak telemetry proving
clean unload. Depends on near terrain, streaming telemetry, and ownership
soak (other milestone-sized work).

This is a milestone-sized feature plan spanning rendering, streaming, and
world-data ownership — not a single-site or few-file fix. Same shape as
#2376 (EX-06/07) from the prior fix-issue pass, which the user held open
pending its own planning session.
