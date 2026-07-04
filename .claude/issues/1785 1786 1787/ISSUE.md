# #1785 — CONC-D3-02: animation_system access declaration omits three color-sink component writes

**Severity**: LOW · **Domain**: ecs (`byroredux-core` scheduler API; registration lives in `byroredux` binary)
**Location**: `byroredux/src/main.rs:783-813` (declaration) vs. `byroredux/src/systems/animation.rs:150-172` (writes)

`apply_color_channels` lazily takes `world.query_mut::<AnimatedAmbientColor>()`,
`AnimatedSpecularColor`, and `AnimatedShaderColor` for their respective color
channels. The `add_to_with_access` declaration at main.rs:791-812 declares
`AnimatedDiffuseColor` and `AnimatedEmissiveColor` writes but omits the other
three, despite the comment "The declaration is the UNION across all paths."

Impact: latent — animation is the only parallel system in `Stage::Update`
today. A future parallel system touching any of the three storages would be
co-scheduled as "no conflict" by the analyzer, opening a genuine cross-thread
write-write / ABBA window invisible to the #1394/#1602 startup asserts.

Related: CONC-D4-01 (#1787, sibling declaration gap).

Suggested fix: add `.writes::<AnimatedAmbientColor>()
.writes::<AnimatedSpecularColor>() .writes::<AnimatedShaderColor>()` to the
animation declaration in main.rs.

Completeness checks called out in the issue:
- LOCK_ORDER: completed declaration is the true UNION across every `apply_color_channels` path
- SIBLING: cross-check CONC-D4-01 (#1787) in the same pass
- TESTS: a declaration-vs-acquisition check pins the animation write surface

---

# #1786 — CONC-D3-04: CommandRegistry read guard held across arbitrary command execution; help re-enters the same lock

**Severity**: LOW · **Domain**: ecs (debug console dispatch)
**Location**: Dispatchers `crates/debug-server/src/evaluator.rs:413-417`,
`byroredux/src/main.rs:268-269`, `byroredux/src/main.rs:2688-2689`; re-entry
`byroredux/src/commands/world_info.rs:17`

All three command dispatch sites hold a `ResourceRead<CommandRegistry>` while
calling `reg.execute(world, expr)` (structurally unavoidable — the registry
owns the boxed `ConsoleCommand` objects). Every command body runs with a live
read guard on the `CommandRegistry` RwLock. `HelpCommand::execute` re-acquires
it read-only. The always-on thread-local tracker permits read-read, and no
runtime writer exists, so this is currently benign.

Impact: two latent failure modes, both gated on code that doesn't exist yet:
(a) any future command taking `resource_mut::<CommandRegistry>()` panics via
the always-on tracker; (b) a cross-thread writer queued on the lock between
the dispatcher's read and `help`'s re-entrant read could deadlock
`std::sync::RwLock` (platform-dependent).

Related: CONC-D3-01 (same tracker behavior).

Suggested fix: document the contract on `ConsoleCommand::execute` ("runs
under a read guard on `CommandRegistry` — commands must never acquire it
mutably"); optionally have `HelpCommand` receive the listing via the
dispatcher instead of re-locking.

Completeness checks called out in the issue:
- LOCK_ORDER: the read-guard-held-across-execute contract is documented so no future command re-acquires it mutably
- SIBLING: all three dispatch sites carry the same contract note
- TESTS: (optional) a test that a mutable `CommandRegistry` acquire under dispatch trips the tracker

---

# #1787 — CONC-D4-01: physics_sync_system under-declares its read surface (ContactConfig + #1698 faller-dump reads)

**Severity**: LOW · **Domain**: ecs (declaration) / physics (`byroredux-physics`, body)
**Location**: `crates/physics/src/sync.rs:226-244` and `:371` (body) vs
`byroredux/src/main.rs:887-908` (declaration). Consolidates CONC-D3-03.

`physics_sync_system` is registered in the `Stage::Physics` parallel batch
with a declared surface that omits four accesses its body performs:
- `ContactConfig` resource read — `world.try_resource::<ContactConfig>()` in
  `register_newcomers` (sync.rs:371). The same undeclared read exists in
  `player_controller_system` (character.rs:230-233).
- `RenderLayer` (component read), `FormIdComponent` (component read),
  `FormIdPool` (resource read) — the #1698 awake-faller diagnostic
  `dump_awake_fallers` (sync.rs:242-244), reachable from the body at
  sync.rs:169-171, gated behind `BYRO_PROFILE_FALLERS` + a one-shot
  `AtomicBool`.

Impact: no live hazard today — `physics_sync_system` is the only system in
`Stage::Physics`. Latent "silently defeats the analyzer" class: a future
`Stage::Physics`/`Stage::Early` parallel system that *writes* any of the four
types would have `analyze_pair` return `None` instead of `Conflict`,
invisible to the #1394/#1602 startup asserts.

Related: CONC-D3-02 (#1785)/03 (same declaration-completeness class).

Suggested fix: append `.reads_resource::<byroredux_physics::ContactConfig>()`,
`.reads::<RenderLayer>()`, `.reads::<FormIdComponent>()`,
`.reads_resource::<FormIdPool>()` to the registration at main.rs:890-907, and
`.reads_resource::<ContactConfig>()` to the `player_controller_system`
declaration (main.rs:655-670).

Completeness checks called out in the issue:
- LOCK_ORDER: both declarations are the true UNION of their read surface
- SIBLING: the consolidated CONC-D3-03 `player_controller_system` `ContactConfig` read is fixed alongside
- TESTS: a declaration-vs-acquisition check pins the four added reads
