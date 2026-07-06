# #1834 (ECS-2026-07-02-01, MEDIUM) + #1835 (ECS-2026-07-02-02, LOW) — save-registry gaps

## #1834 — ActorValues absent from the M45 save registry (active data loss)
`ActorValues` is stamped on every NPC placement root at spawn and mutated at
runtime by the `setav`/`modav` console commands (`commands/actor_value.rs`
`query_mut::<ActorValues>`), but derived no serde and was in neither
`build_save_registry()` nor `MUTABLE_DELTA_COLUMNS`. A save→load dropped every
base/permanent/temporary/damage layer and re-derived only the class-auto-calc
base on respawn — `GetActorValue` conditions then read the wrong value.

**Fix**:
- `crates/core/src/ecs/components/actor_values.rs`: `#[cfg_attr(feature =
  "inspect", derive(serde::Serialize, serde::Deserialize))]` on `ActorValue` +
  `ActorValues`. NOTE: core gates serde on `inspect` (like Transform/Inventory),
  NOT the `"save"` gate the issue text suggested — `"save" = ["inspect"]`, and
  the ScriptTimer `"save"` gate is a *scripting-crate* convention.
- `byroredux/src/save_io.rs`: registered `ActorValues` in `build_save_registry`,
  added `"ActorValues"` to `MUTABLE_DELTA_COLUMNS` (delta-safe: HashMap keyed by
  global-space AVIF FormID u32 + four f32 layers — no FixedString/EntityId/
  session handle), and extended the `AUDITED` tripwire set.
- Test `actor_values_survive_save_load_round_trip`: a real save→encode→decode→
  restore round-trip proving an edited actor value's four layers survive (fails
  pre-fix: empty query → `.expect` panic).

## #1835 — four CHARAL/faction components share the pattern (latent)
`FactionRanks`, `CharacterLevel`, `Background`, `Perks` are stamped write-once
at spawn from static ESM `NPC_` data with **no runtime mutator** anywhere in
production (verified: no `get_mut` outside tests, no `setlevel`/`addperk`/
`setfactionrank` command). So a save→load re-derives byte-identical values on
respawn — a correct no-op today, not data loss. The issue explicitly asks for
NO forced registration now, plus a structural guard so the gap can't recur
silently.

**Fix** (the durable structural guard, not premature registration):
- `crates/save/src/registry.rs`: added `pub fn component_names()` accessor so
  callers can audit registry membership.
- `byroredux/src/save_io.rs` test `npc_spawn_stamped_components_are_saved_or_
  intentionally_rederived`: enumerates the persistent gameplay-state components
  `npc_spawn.rs` stamps and asserts each is registered XOR in an explicit
  `REDERIVED_NOT_SAVED` allowlist (the four CHARAL types, documented as
  write-once-from-ESM). The XOR catches BOTH a new unclassified stamp AND a type
  wrongly left in the allowlist after being registered. When a runtime mutator
  lands, the fix is: register it + drop it from the allowlist in the same commit
  (per #1835) — the test enforces exactly that pairing.

Not chosen: registering the four now. They are re-derivable from static ESM, so
persisting them risks a save carrying stale values if the ESM derivation later
changes — re-deriving is the more correct behaviour until a mutator exists.

## Domain / verification
ecs+save → touched `byroredux-core`, `byroredux-save`, `byroredux` (3 files).
Scoped suites green (core 5, save 20+10, save_io 13 incl. 2 new); core builds
with `--features inspect` (serde-active path); full workspace green, no new
warnings.
