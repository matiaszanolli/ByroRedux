# Investigation — #1442 LC-D5-03 KF-sequence dispatch missing NiKeyframeController alias

**Domain:** animation / nif (import dispatch)

## Root cause
`import_sequence` (`crates/nif/src/anim/sequence.rs`) dispatched controlled
blocks on the resolved controller-type *string*, matching only
`"NiTransformController"`. The block parser (`blocks/mod.rs`) deliberately
aliases `"NiTransformController" | "NiKeyframeController"` (#144), and the
embedded-animation path (`anim/entry.rs`) already aliases both. A controlled
block whose type string resolved to the classic `"NiKeyframeController"` name
fell to the `_ =>` drop and its transform channel was silently lost.

## Premise note
The issue flagged the premise as unconfirmed against sample FNV/Oblivion KF data.
The fix is strictly additive parity — it only adds handling for a string that
currently drops, so its correctness does not depend on the premise: no content
using the classic name → the new arm never fires (no behaviour change); content
that does → now handled instead of dropped. No failure mode is introduced, so it
is safe to apply without a live .kf repro.

## Fix
- `crates/nif/src/anim/sequence.rs`: `"NiTransformController" |
  "NiKeyframeController" => { … }` (mirrors the parser + embedded path).
- SIBLING: `crates/nif/src/lib.rs::is_animation_block` listed `"NiKeyframeData"`
  but not `"NiKeyframeController"` — the same alias gap (it gates
  `options.skip_animation`). Added the controller alias there too.
- `crates/nif/src/anim/entry.rs` already aliases both — left unchanged.

## Tests
`import_sequence_dispatches_keyframe_controller_alias` — builds a sequence with a
NiLookAtInterpolator-backed controlled block and asserts BOTH
`"NiTransformController"` (baseline) and `"NiKeyframeController"` (fix) produce a
transform channel keyed by the node.

## Verification
cargo test 2794 passed; no warnings in touched files.
