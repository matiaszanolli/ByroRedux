# #3444 — CONC-D3-2026-08-27b-02: #2153's hold-stack reduction never happened — let config = *config; shadows but does not drop the PoolRegenConfig guard

**Severity**: MEDIUM · **Dimension**: 3 (ECS Lock Ordering & Deadlock)
**Location**: `crates/core/src/character/regen.rs::pool_regen_tick_system`

## Fix

Verified the premise: `let config = *config;` (line 165) SHADOWS the old
`config` binding — the original `ResourceRead<PoolRegenConfig>` guard is
neither moved nor dropped at that point, so its `Drop` (the thing that
actually calls `lock_tracker::untrack`) still ran at end-of-function-scope.
The real hold-stack at the `ActorValues` acquire stayed `{PoolRegenConfig,
CharacterRuleset, ActorValues}` — 3 deep, exactly what #2153 was filed
against — despite the adjacent comment claiming the guard was "dropped
here." Rust's drop semantics are the whole argument, matching the issue's
own "verification path" note.

Applied the issue's own suggested one-line fix exactly: renamed the guard
binding to `config_guard`, copied its fields into a freshly-named `config`,
and added an explicit `drop(config_guard);` — mirroring the immediately
adjacent `accumulator`/`drop(accumulator);` pattern the original #2153
comment already cited as the model but didn't actually follow.

## SIBLING (issue's own checklist item — "at minimum
`byroredux/src/combat.rs`'s `melee_damage_charal_bonus`; sweep for other
`let x = *x;` copy-out-of-guard sites")

`combat.rs`'s instance is already fixed — under a **different** issue
(#3473), using an even tighter shape: `.map(|config| config.melee_damage_avif)`
extracts just the one field needed directly in the `try_resource(...)`
chain, ending the guard's borrow at the `.map()` call itself, no separate
`let x = *x;` line at all.

Swept the whole workspace for `let <name> = *<name>;` (a broader regex
than an exact-name backreference, to catch case/spacing variants) and
found 5 more matches: `crates/sdk/src/legacy_containers.rs` (a plain enum
destructure inside a test, no `World` storage involved),
`crates/physics/src/sync.rs` (dereferencing a `&f32` from `Vec` tuple
iteration), `crates/renderer/src/vulkan/context/{depth_capture,screenshot}.rs`
(both copy a `vk::Buffer` out of a plain `Option<&(...)>` struct field, no
`World` guard involved), `byroredux/src/commands/scene.rs` (dereferencing
an already-collected `Vec` tuple's `EntityId`). None acquire a `World`
storage guard (`ResourceRead`/`ComponentRef`/`QueryRead`) at the shadowed
binding — this specific hazard class (a lock guard silently outliving
where the code claims it's dropped) needs the shadowed value to actually
BE a tracked guard, and none of these five are. No fix needed at any of
them.

## LOCK_ORDER (issue's own checklist item)

No `query_2_mut`/paired-acquisition API involved — this is a single
resource guard's drop timing, corrected via an explicit `drop(...)` call,
not a reordering of a `TypeId`-sorted pair.

## TESTS (issue's own checklist item — "pin it with a source-assert test
in `regen.rs`'s existing test module")

The existing `config_guard_is_dropped_before_the_ruleset_acquire` test
was itself part of the problem: it source-asserted on the textual
position of `"let config = *config;"` relative to the `CharacterRuleset`
acquire — a check that can only ever verify TEXT ORDER, never actual drop
timing, so it passed cleanly even though the guard it claimed to pin was
never really dropped. Updated it to source-assert on the new explicit
`"drop(config_guard);"` line instead, which is a real, checkable proxy
for the drop actually happening (a shadow can't produce that exact
substring; only a genuine `drop()` call can).

**Reintroduce-and-revert verification**: temporarily removed the explicit
`drop(config_guard);` line (restoring the shadow-only shape at the
call site, though keeping the renamed bindings) — confirmed the updated
test failed with the expected message. Restored the fix and reran — all
8 tests in `character::regen::tests` pass again.

## Verification

- `cargo check -p byroredux-core --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-core --lib character::regen::`: 8 passing,
  0 failing (existing test corrected, not a net-new count).
- `cargo test -q --no-fail-fast` (full workspace): **7187 passing, 0
  failing**.
