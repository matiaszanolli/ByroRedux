# #3638 — FO4-2026-08-30-D6-02: TXST DecalData is parsed and never read — 303 vanilla DODT payloads dropped while the DNAM sibling is consumed

**Severity**: LOW · **Dimension**: 6
**Location**: `crates/plugin/src/esm/cell/mod.rs::DecalData`

## Investigation — the "oversight vs. deliberate deferral" question

`DecalData`'s own struct-level doc comment already carried a deferral
note ("Renderer-side decal rendering… consumes the width / depth /
parallax / colour fields once the M28 decal pipeline extension lands"),
which at first reading looked like it might make the issue's premise
stale — the omission reading as deliberate, not an oversight.

Checked whether "M28" is real: it isn't, for this purpose. `ROADMAP.md`
is unambiguous — `M28` (and `M28.5`) is the Rapier3D physics bridge /
kinematic character controller milestone, entirely unrelated to decal
rendering. No decal-pipeline milestone is tracked anywhere in
`ROADMAP.md` or `HISTORY.md`. So the existing deferral note pointed at a
milestone that doesn't exist for this purpose — a misnomer that could
mislead a future reader into believing real tracking exists when it
doesn't. Worse, the field-level doc on `decal_data` itself (two doc-hops
away from `DecalData`'s struct comment) carried no deferral note at all,
so a reader landing there first — the more natural place to look — saw
nothing explaining the gap.

Wiring real decal rendering (actually consuming
width/height/depth/shininess/parallax/flags/RGB in the render path) is a
genuine feature build with visual correctness I can't verify without a
GPU render — out of scope for a LOW-severity single-issue fix. Applied
the issue's own documentation alternative instead, corrected this time
to not invent a false milestone reference.

## Fix

- `DecalData`'s struct-level doc: removed the wrong "M28 decal pipeline
  extension" claim, replaced with an explicit #3638 note stating no
  consumer exists and no decal-consuming milestone is tracked anywhere,
  plus the measured 303-payload count and the DNAM-sibling-is-consumed
  asymmetry the issue's evidence names.
- `TextureSet::decal_data`'s own field-level doc: added a one-line
  cross-reference to the #3638 note so a reader landing on the field
  directly (not the struct) also sees the gap immediately.

## TESTS (issue's own checklist item — "a regression test pins one of
the 303 vanilla DODT payloads reaching whatever consumer is added")

No consumer was added (documentation-only fix, per the reasoning
above), so that literal test doesn't apply. Added
`decal_data_doc_does_not_claim_a_nonexistent_m28_decal_milestone`
instead: asserts the stale milestone claim is gone and the `#3638`
marker is present at BOTH doc sites (struct-level and field-level).

Hit the self-matching trap twice while writing it — once in `mod.rs`'s
own corrective doc (which initially quoted the exact stale phrase
verbatim while explaining what it used to say) and once in the test's
own doc comment (same mistake, one level removed). Fixed both by
describing the stale claim in general terms rather than quoting its
literal text, matching the established convention this session has hit
several times before for exactly this hazard.

**Reintroduce-and-revert verification**: temporarily restored both
stale doc texts (the struct-level M28 claim and removed the field-level
note) — confirmed the new test failed (`"DecalData's struct-level doc
must carry the #3638 marker"`). Restored the fix and reran — all 13
tests in `esm::cell::tests::txst` pass again.

## Verification

- `cargo check -p byroredux-plugin --tests`: clean (the pre-existing
  unrelated `grup_walker.rs:469` `unused_mut` warning is present and
  out of scope).
- `cargo test -q -p byroredux-plugin --lib esm::cell::tests::txst::`: 13
  passing, 0 failing (+1 new).
- `cargo test -q -p byroredux-plugin`: 914 passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7178 passing, 0
  failing**.
