# SCR-D2-2026-10-17-04: Deferred and event-enqueued runs bypass `MAX_FRAGMENT_CONTINUATIONS`; `SendCustomEvent` fan-out grows the saved queue without bound

Issue: #5984
Labels: high, scripting, bug, save-load

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-04: Deferred and event-enqueued runs bypass `MAX_FRAGMENT_CONTINUATIONS`; `SendCustomEvent` fan-out grows the saved queue without bound

- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: cap at `fragment/systems.rs:121-130`; uncapped pushes at `fragment/budget.rs:164-185` (`defer`), `fragment/events.rs:230-266` (`enqueue`), `fragment/effects.rs:792,937,983`; amplifier `fragment/vm/registrations.rs:163-181` (`send_custom_event`)
- **Status**: NEW (relation to #5681 below)
- **Untrusted-Input**: Yes
- **Description**: #5681 added `MAX_FRAGMENT_CONTINUATIONS = 4_096` so "a mod that re-fires ... cannot grow the queue - saved with the world - without limit". The cap is applied in exactly one place: when `fragment_continuation_system` re-partitions entries whose `remaining_seconds > 0` (`still_pending.truncate`). Every zero-delay entry - budget deferrals (`budget::defer`), custom/remote/animation event deliveries (`events::enqueue` -> `defer`), `Interpret` yields - goes `ready`, never counts against the cap, and when the frame budget is exhausted the system returns early (`systems.rs:52-60`) or re-pushes the ready tail unchanged (`:145-161`) with no cap at all. `send_custom_event` queues *every* registered receiver's handler as a new queue entry each call and nothing tracks event generation or sender depth. A script that registers for its own custom event and re-sends it twice in the handler (`RegisterForCustomEvent(Self, "Ping")` / `Event Script_Ping(...)  SendCustomEvent("Ping") x2`) adds one net entry per run: the queue grows without bound at roughly the frame budget's run rate (~600 runs/frame), each entry holding a cloned `ScriptInstanceData` (`budget.rs:172`, `vmad.cloned()`), and it is serialised into every save (`FragmentExecutionQueue` is a saved resource).
- **Evidence**: `still_pending` (the only truncated vector) is filled only with entries that stayed `remaining_seconds > 0` (`systems.rs:76-110`); the existing test `continuation_queue_drops_newest_past_the_cap` (`tests.rs:3726`) uses timed entries and so cannot see the hole. No other reference to `MAX_FRAGMENT_CONTINUATIONS` exists (`grep`).
- **Impact**: Unbounded memory/save growth from a mod script (or a plain Papyrus event storm), the exact outcome #5681 set out to prevent, now reachable through the #5763/#593b1cfaa paths added after it. Not a crash, but unbounded allocation driven by mod bytes (HIGH under the skill's escalation rule).
- **Related**: #5681 (closed; fix incomplete for zero-delay entries), #5764 (budget-split residual).
- **Suggested Fix**: Enforce the cap at the push sites (one `FragmentExecutionQueue::push` that drops the newest and warns past the cap, used by `defer`, `enqueue` and the three `effects.rs` sites), and give `send_custom_event` a per-frame delivery ceiling.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
