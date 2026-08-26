# Issues 2377, 3147, 3162, 3163 — investigation results (no code changes)

All four issues were investigated. Three are stale audit findings already
fixed by later, unrelated-looking commits before this session; the fourth is
a tracking epic that isn't ready to close.

## #2377 — Exterior runtime readiness epic (tracking issue, not a code bug)
Meta-issue depending on 9 sub-issues. Checked all nine via `gh issue view`:
- CLOSED: #2368, #2375, #2376, #2374, #2370, #2371, #2373 (7/9)
- OPEN: #2369 (EX-14/15 — ground cover, persistent refs, parent worlds, FO4
  spatial data), #2372 (EX-16 — REGN/NAVM/audio/AI integration)

Not something to "fix" via a single code change — it's a coordination issue
that closes when its dependencies close. Posted a status comment; left open.

## #3147 — archive-backed menu-load path had no engine consumer
**Already fixed** by `4e1afcbe` (2026-08-24, "refactor: unify actor value key
space and improve documentation" — an unrelated-sounding commit that also
landed this). Verified at HEAD:
- `byroredux/src/scene.rs:95` `archive_menu_args()` parses `--menu` +
  `--menu-archive`, doc comment explicitly cites `#3147`.
- `byroredux/src/scene.rs:1500-1555` wires `Archive::open` →
  `ScaleformProfile::detect` → `UiManager::load_swf_from_resource_provider`.
- Regression tests at `scene.rs:1602-1627`.
- `docs/engine/ui.md:39` Status row already lists the archive-backed menu
  launch; `ROADMAP.md:763` (M48) already describes it as live.
No code change needed.

## #3162 — quicksave/quickload abort was silent (log::info! only)
**Already fixed** by `06f86742` + `eb582353` (both 2026-08-24, 4 days after
the 2026-08-20 audit). Verified at HEAD:
- All four call sites (`app_events.rs` F5/F9, `main.rs` pause-menu ×2,
  `main.rs` `--load` boot queue) now route through
  `surface_save_load_output()` (`main.rs:740`), which `log::warn!`s on
  failure vs `log::info!` on success and pushes to
  `debug_ui.push_player_message`/`push_console_line`.
- `quickload_latest` (`save_io.rs:897`) now loops `slots_by_recency`,
  skipping undecodable/failing slots and falling back to the next-newest
  with an explicit "falling back to valid slot N" trace.
No code change needed.

## #3163 — apply_deltas mid-column failure left world half-overlaid
**Already fixed**, same commit `06f86742` (2026-08-24). Verified at HEAD:
- `crates/save/src/driver.rs:176` `validate_snapshot_types()` — a
  non-mutating decode-only pass over every component/resource column.
- `byroredux/src/save_io.rs:1228-1238` calls it in
  `execute_pending_save_loads` **before** `reload_interior_session` /
  `reload_exterior_session` (the irreversible teardown), aborting with
  `notify_player` on any typed-decode failure — exactly the suggested
  pre-flight-before-teardown fix, explicitly commented `#3163`.
- `reconcile_dead_actor_runtime_state` now runs in **both** the `Ok` and
  `Err` arms of the `apply_deltas` match (`save_io.rs:1277`, `:1290`), and
  the `Err` arm `return`s instead of falling through into
  `validate_world`/`apply_player_pose`.
- Regression coverage in `crates/save/tests/round_trip.rs`.
No code change needed.
