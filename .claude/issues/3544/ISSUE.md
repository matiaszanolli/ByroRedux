# #3544 — SK-D3-02: crates/facegen's .egt and .tri parsers have zero consumers anywhere in the workspace

**Severity**: MEDIUM · **Dimension**: NPC Equip + FaceGen
**Location**: `crates/facegen/src/egt.rs`, `crates/facegen/src/tri.rs`, crate doc in `crates/facegen/src/lib.rs`

## Fix

Verified the premise: `EgtFile`/`EgtMorph`/`TriHeader` genuinely have no
consumer anywhere in the workspace outside the crate's own tests and a
throwaway audit probe. Wiring a real EGT compositor is out of scope for
this LOW-effort fix (the crate's own `tri.rs` doc already calls its body
parse "M47-tier work" — a real feature build, not a single-site fix), so
applied the issue's own second suggested option: corrected the doc claims
and marked both modules explicitly deferred, so the next reader doesn't
assume Phase 3c (the EGT half) shipped.

Found the root of the misleading claim while investigating: this
codebase's own "Phase 3c" milestone label is used for two *different*
things that happen to share a number — `eval.rs`'s "Phase 3c" (FGGA
asymmetric geometry morphs) is genuinely wired (confirmed:
`byroredux/src/npc_spawn/resumable.rs:1057` calls `apply_morphs` with
`egm.fgga_morphs`), while `lib.rs`/`egt.rs`'s "Phase 3c" (the EGT texture
compositor) is not. Left `eval.rs`'s doc untouched — it's accurate — and
corrected only the EGT-texture-compositor claims:

- `crates/facegen/src/lib.rs` — crate doc's "Phase 3c consumes the EGT
  compositor output" replaced with an explicit #3544 deferral paragraph
  naming both gaps (EGT compositor, `.tri` body parse) as measured, not
  silently working.
- `crates/facegen/src/egt.rs` — module doc's "The face-tint compositor
  (M41.0 Phase 3c) blends them into the base diffuse texture at NPC load
  time" (present tense, implying it exists) corrected to state no
  consumer exists; `EgtMorph`'s own doc softened from "The compositor
  (Phase 3c) applies it as…" to "A future compositor (#3544, no
  implementation exists yet) would apply it as…".
- `crates/plugin/src/esm/records/actor/mod.rs::NpcFaceGenRecipe::fgts` —
  "Applied in the face-tint compositor (Phase 3c)" corrected to state
  FGTS is parsed and carried but nothing applies it yet.
- `crates/facegen/src/tri.rs` — added the same "no consumer" note
  alongside its existing (already-honest) "deferred to M47-tier work"
  framing, per the SIBLING check below.

## SIBLING (issue's own checklist item — "if EGT is wired, check the
`.tri` header stub for the same shape")

EGT was not wired (documentation-only fix), so the conditional doesn't
literally apply — but `tri.rs` has the identical "parsed but never read"
shape the issue's own evidence table already names (`TriHeader`: no
consumers), so added the same deferral marker there for consistency
rather than leaving it asymmetric.

## TESTS (issue's own checklist item — "a regression test pins whichever
outcome is chosen… a doc/deferral marker the tests read")

`facegen_docs_do_not_overclaim_an_egt_compositor`
(`crates/facegen/src/lib.rs`) — asserts the stale claim string is gone
from the crate doc and that all three touched files (`lib.rs`, `egt.rs`,
`tri.rs`) carry the `#3544` marker.

Hit the by-now-familiar self-matching trap while writing it:
`include_str!("lib.rs")` embeds the whole file, including the test
module itself — my first draft's own doc comment and assertion string
both contained the literal stale-claim text, so the assertion matched its
own describing prose instead of the real (already-fixed) module doc,
producing a false failure. Fixed by scanning only the file slice before
`#[cfg(test)]` (matching the established convention this session has hit
twice before, in `crates/core/src/ecs/components/material.rs` and
`crates/audio/src`).

**Reintroduce-and-revert verification**: temporarily restored the exact
stale crate-doc text — confirmed
`facegen_docs_do_not_overclaim_an_egt_compositor` failed with the
expected message. Restored the fix and reran — all 30 tests in
`byroredux-facegen` pass again.

## Verification

- `cargo check -p byroredux-facegen -p byroredux-plugin --tests`: clean
  (the pre-existing unrelated `grup_walker.rs:469` `unused_mut` warning
  is present and out of scope).
- `cargo test -q -p byroredux-facegen`: 30 passing, 0 failing (+1 new).
- `cargo test -q -p byroredux-plugin`: 913 passing, 0 failing (doc-only
  change to `actor/mod.rs`, no behavioral surface).
- `cargo test -q --no-fail-fast` (full workspace): **7167 passing, 0
  failing**.
