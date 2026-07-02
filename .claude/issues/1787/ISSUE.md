# CONC-D4-01: physics_sync_system under-declares its read surface (ContactConfig + #1698 faller-dump reads)

_Filed as #1787 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: LOW · **Dimension**: Scheduler Access Declarations · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D4-01)

## Location
`crates/physics/src/sync.rs:226-244` and `crates/physics/src/sync.rs:371` (body) vs `byroredux/src/main.rs:887-908` (declaration). Consolidates CONC-D3-03.

## Description
`physics_sync_system` is registered in the `Stage::Physics` parallel batch via `add_to_with_access` with a declared surface (main.rs:890-907) that omits four accesses its body performs:
- (a) **`ContactConfig` resource read** — `world.try_resource::<ContactConfig>()` in `register_newcomers` (sync.rs:371; present since 525c690c, 2026-05-22). The same undeclared read exists in `player_controller_system` (character.rs:230-233).
- (b) **`RenderLayer` (component read), `FormIdComponent` (component read), `FormIdPool` (resource read)** — the #1698 awake-faller diagnostic `dump_awake_fallers` (sync.rs:242-244), reachable from the system body at sync.rs:169-171, gated behind `BYRO_PROFILE_FALLERS` + a one-shot `AtomicBool`.

## Evidence
`grep -n "ContactConfig\|RenderLayer\|FormIdComponent\|FormIdPool" crates/physics/src/sync.rs` vs main.rs:890-907 — the four types appear in the body, not the declaration. `grep ContactConfig byroredux/src/main.rs` → only the `insert_resource` at :548, no `Access` mention.

## Impact
No live hazard today — `physics_sync_system` is the **only** system registered in `Stage::Physics`, so it pairs against nothing and the missing entries are all read-side. Latent "silently defeats the analyzer" class: a future `Stage::Physics`/`Stage::Early` parallel system that *writes* any of the four types would have `analyze_pair` return `None` (both declared, no visible overlap) instead of `Conflict`, invisible to the #1394/#1602 startup asserts (which detect *undeclared systems* and *declared conflicts*, not declared-but-incomplete surfaces).

## Related
CONC-D3-02/03 (same declaration-completeness class).

## Suggested Fix
Append `.reads_resource::<byroredux_physics::ContactConfig>()`, `.reads::<RenderLayer>()`, `.reads::<FormIdComponent>()`, `.reads_resource::<FormIdPool>()` to the registration at main.rs:890-907, and `.reads_resource::<ContactConfig>()` to the `player_controller_system` declaration (main.rs:655-670).

## Completeness Checks
- [ ] **LOCK_ORDER**: Both `physics_sync_system` and `player_controller_system` declarations are the true UNION of their read surface
- [ ] **SIBLING**: The consolidated CONC-D3-03 `player_controller_system` `ContactConfig` read is fixed alongside
- [ ] **TESTS**: A declaration-vs-acquisition check (or `sys.accesses` assertion) pins the four added reads
