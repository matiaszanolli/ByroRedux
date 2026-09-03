# #3772 — UI-D2-2026-08-30-02: the #2969 drop-counter latch is not reset on menu swap, so a new menu's first N drops are silent when the previous menu dropped N

**Severity**: LOW · **Location**: `byroredux/src/app_frame.rs`, `byroredux/src/main.rs`
**Source**: `docs/audits/AUDIT_UI_2026-08-30.md` (UI-D2-2026-08-30-02)

`App::ui_dropped_host_calls` latches a bare `u64` reading of `UiManager::dropped_host_calls()`
with no menu identity attached. `host_call_gap`'s decrease guard correctly treats a decrease as
"new menu, fresh bridge" and stays silent — but a swap landing on a frame where the NEW bridge
has already evicted `N < latched` reads exactly like a same-bridge decrease (impossible in
practice) and is silently absorbed instead of reported. Menu B's first N lost calls then never
surface, and the latch overwrites to B's smaller value, so only B's *next* increase past that
value gets reported.

## Fix implemented

- `App` gained `ui_dropped_host_calls_menu: Option<String>` — the menu name the latch was last
  updated against.
- New pure function `host_call_gap_for_menu(latched, latched_menu, menu, reported)`
  (`byroredux/src/main.rs`, alongside `host_call_gap`) makes the reset **explicit**: effective
  latch is `0` whenever `menu != latched_menu`, then delegates to `host_call_gap`'s existing
  increase-only logic. `app_frame.rs`'s drain loop now calls this instead of `host_call_gap`
  directly, and updates both the latch and its menu name together.
- Extracted as a standalone testable function (mirroring why `host_call_gap` itself was already
  pulled out) rather than inlining the reset check in `app_frame.rs`, so the exact scenario the
  issue's table describes is unit-testable without the full per-frame render machinery.

Regression tests (issue's own TESTS checklist item):
`a_swap_to_a_bridge_that_already_dropped_fewer_than_the_old_latch_reports_all_of_them` pins the
issue's exact lossy scenario — menu A latches at 1000, menu B swaps in already having dropped
999 (`999 < 1000`, the case `host_call_gap(1000, 999)` alone reads as a decrease and silently
returns `None`); `host_call_gap_for_menu` correctly reports `Some(999)` instead. Plus
`same_menu_still_reports_only_increases` and `no_prior_menu_is_treated_as_a_reset` as sanity
bookends.

**SIBLING** (issue's own checklist item): checked every other `App`-resident UI latch.
`ui_reported_host_methods: HashSet<(String, String)>` is already keyed by `(menu_name, method)`
directly, so it's immune to this exact bug by construction — no reset logic needed.
`ui_reported_host_methods_capped: bool` is a genuinely session-lifetime (not per-menu) latch —
it guards the aggregate `HashSet`'s own memory cap across the whole session, not a per-menu
signal, so it's correctly *not* per-menu-reset. No sibling gap found.

Full workspace: `cargo test --no-fail-fast` 7041 passing, 0 failing.
