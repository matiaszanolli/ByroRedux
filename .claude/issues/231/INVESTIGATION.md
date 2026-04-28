# Issue #231 — Investigation (SI-04 only; SI-02 deferred per audit)

## Decision

Implemented SI-04 in full. SI-02 (NIF parser standalone interning into StringPool) deferred — the audit itself flags it as "low priority, one-time cost" and doing it would couple the standalone `nif` crate to `core::string::StringPool` for an architecturally clean parser path. Skipped clip-name interning: the audit's "compared frequently" rationale doesn't hold for `clip.name` (only used for logging, never in a hot path), so the value is purely cosmetic and the type ripple isn't worth it.

## Wins from interning text_keys

1. **Per-fire allocation gone** — `byroredux::systems::animation_system` previously called `label.to_owned()` every time a text key fired (2 emission sites: AnimationPlayer hot path, AnimationStack hot path). Now the event component carries `FixedString` directly.
2. **Storage dedup** — labels like `"hit"`, `"FootLeft"`, `"sound: wpn_swing"` repeat across hundreds of clips per actor. StringPool stores each unique label once.
3. **Visitor lookup cleanup** — the previous `visit_stack_text_events` had a workaround `clip.text_keys.iter().find(|(_, l)| l == label)` to recover a `&'clip str` lifetime past the visitor closure. That re-find is gone — `FixedString` is `Copy` and lifetime-free.
4. **Dedup is integer comparison** — the seen-set in `visit_stack_text_events` and the per-frame `seen_labels` scratch in `systems.rs:497` now use `Vec<FixedString>::contains`, which is a `FixedString == FixedString` integer compare instead of `&str == &str` byte compare.

## Files touched (7)

- `crates/core/src/animation/types.rs` — `text_keys: Vec<(f32, FixedString)>`
- `crates/core/src/animation/text_events.rs` — visitor sig `FnMut(f32, FixedString)`; `collect_text_key_events` adds `&StringPool` param
- `crates/core/src/animation/stack.rs` — `visit_stack_text_events` seen-set is `Vec<FixedString>`; `collect_stack_text_events` adds `&StringPool` param
- `crates/core/src/animation/mod.rs` — 4 tests updated to thread a local `StringPool`
- `byroredux/src/anim_convert.rs` — labels interned via the existing `&mut StringPool` already threaded through `convert_nif_clip`
- `byroredux/src/systems.rs` — both emission sites push `AnimationTextKeyEvent { label: sym, time }`
- `crates/scripting/src/events.rs` — `AnimationTextKeyEvent.label: FixedString`; struct now `Copy`. No existing readers, so zero downstream API churn.

## Notes

- `clip.name` left as `String` (unrelated to hot paths; not worth the type ripple).
- SI-02 not addressed; the audit explicitly defers it.
- `cargo check` clean, full test suite passes, zero new warnings.
