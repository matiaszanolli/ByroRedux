# #2720: Navigator pump has two silent permanent-freeze paths; the preload stall branch is indistinguishable from a hang

**Found independently by 2 audits in the same `ui-deep` suite run** — merged here.

### CONC-D7-UI-04 — CONCURRENCY_UI view

*Navigator pump has two silent permanent-freeze paths*

- **Severity**: MEDIUM (`_audit-severity.md`: "Missing error handling on recoverable paths")
- **Dimension**: 7 — Worker Threads (local-executor pump liveness)
- **Location**: `crates/ui/src/player.rs:199-227`, `crates/ui/src/player.rs:343-362`,
  `crates/ui/src/navigator.rs:105-116`, `crates/ui/src/navigator.rs:126-133`
- **Status**: NEW
- **Description**: two distinct latches, both of which stop the movie forever with no diagnostic
  after the first log line.
  1. **Sticky error.** `ScaleformNavigator::fail` pushes onto `NavigatorState::errors`, which is
     **never cleared** — there is no `clear`/`drain`/`take` on that field anywhere.
     `ScaleformNavigatorRuntime::first_error()` returns `errors.first().cloned()`, so once any single
     fetch fails it returns `Some` on every subsequent call. `tick` copies that into
     `self.resource_error`, and `tick`'s first statement is `if self.resource_error.is_some() { return; }`.
     Net effect: **one** missing dependency — and `fail` is invoked for the entirely routine
     `Ok(None)` "resource was not found in the configured archive" case, with the navigator holding
     exactly **one** `Rc<dyn ScaleformResourceProvider>` (one archive) — permanently freezes the whole
     menu, not just that asset.
  2. **Silent non-settle.** `drive_archive_preload` gives up after
     `MAX_ARCHIVE_PRELOAD_PASSES = 64` and returns `Ok(false)`; `tick` maps that to a bare `return`.
     No error is recorded, `dirty` is not set, and the check re-runs next frame — so an unsettled
     preload suppresses `player.tick()` indefinitely with no timeout and no diagnostic. Note the
     constructor treats the identical condition as a hard `Err` ("did not settle after … passes");
     only the per-frame path swallows it.
- **Evidence**: `navigator.rs:128` — `self.state.borrow_mut().errors.push(message.clone());`
  (only mutation of `errors`); `navigator.rs:114` — `self.state.borrow().errors.first().cloned()`;
  `player.rs:200-202` — the `resource_error.is_some()` early return; `player.rs:206` —
  `Ok(false) => return,`.
- **Trigger Conditions**: any menu whose `ImportAssets` graph references a file absent from the
  single configured archive (cross-archive font/shared-menu imports are the obvious case), or any
  preload that needs more than 64 passes.
- **Impact**: menu frozen for the rest of the session, last-uploaded frame left on screen. Currently
  **latent**: `SwfPlayer::from_resource_provider` / `UiManager::load_swf_from_resource_provider` have
  no callers outside `crates/ui` tests, so the engine's `--swf` path (which uses `SwfPlayer::new`,
  `navigator: None`) never enters either latch. Severity is stated by impact, not reachability, per
  the severity scale's opening rule — and this is the path M48 is heading for.
- **Related**: CONC-D7-UI-02 (same "test-wired, not engine-wired" gap); CONC-D7-UI-06.
- **Suggested Fix**: make a failed *dependency* fetch non-fatal (record it, keep ticking, surface it
  through a `resource_errors()` accessor) and reserve the hard latch for a failure of the root movie.
  For the non-settle path, either escalate to `Err` after N consecutive frames or expose a
  "preload stalled" state instead of an invisible `return`.

---

---

### SAFEUI-06 — SAFETY_UI view

*the archive-preload stall branch is silent — an unsettled preload is indistinguishable from a hang*

- **Severity**: LOW
- **Dimension**: 3 (error handling)
- **Location**: [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs):199-214, 343-362
- **Status**: NEW
- **Description**: `tick` runs `drive_archive_preload` first when a navigator
  runtime exists. `Ok(false)` — meaning `MAX_ARCHIVE_PRELOAD_PASSES` (64)
  passes elapsed without `preload` settling — returns from `tick` with no log,
  no `resource_error`, and no state change. The same condition is a hard error
  at construction (`from_resource_provider` line 118 turns it into an
  `anyhow!`), but at tick time it is swallowed.
- **Impact**: A menu that triggers a mid-playback `ImportAssets` fetch that
  never settles freezes with no diagnostic; `resource_error()` reports `None`
  and `current_frame()` simply stops advancing. Retrying next frame is the
  right behaviour — the problem is that it is invisible.
- **Suggested Fix**: One-shot `log::warn!` on the first `Ok(false)`, with a
  consecutive-stall counter promoted into `resource_error` past some threshold.

---
**Sources**: `docs/audits/AUDIT_CONCURRENCY_UI_2026-08-12.md` (CONC-D7-UI-04), `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (SAFEUI-06)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

