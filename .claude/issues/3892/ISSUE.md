# #3892: TD8-2026-09-05-09: `QuestStageState`'s four dynamic-subscription methods are superseded by three static subscriber-ID constants and survive only on their own unit tests

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-09) via `/audit-publish`, 2026-09-05. Labels: `low,scripting,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3892 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-09), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `crates/scripting/src/quest_stages.rs` — `subscribe_to_quest_events`, `subscribe_to_retained_quest_events`, `acknowledge_quest_events`, `unsubscribe_from_quest_events` (lines 334–361), plus the private `QuestEventLog::subscribe` / `acknowledge` and the `next_subscriber_id` counter behind them
- **Status**: NEW
- **Effort**: trivial (≤30 min) to small (≤2 h, depending on how much of `QuestEventLog`'s dynamic half goes with them)

**Description**
The quest-event log was designed with dynamic subscriber registration (`subscribe → poll → acknowledge → unsubscribe`). Production settled on a different model: three compile-time constants, and every caller passes one directly to `poll_quest_events`.

```rust
pub const SCENE_QUEST_EVENT_SUBSCRIBER:    QuestEventSubscriberId = QuestEventSubscriberId(1);
pub const FRAGMENT_QUEST_EVENT_SUBSCRIBER: QuestEventSubscriberId = QuestEventSubscriberId(2);
pub const TERMINAL_QUEST_EVENT_SUBSCRIBER: QuestEventSubscriberId = QuestEventSubscriberId(3);
```

All three production consumers — `crates/scripting/src/scene/playback.rs`, `crates/scripting/src/fragment.rs` (×2), and `quest_stages.rs`'s own terminal system — use a constant. `subscribe_*` / `unsubscribe_*` / `acknowledge_*` have **zero production callers anywhere in the workspace**; their only callers are five sites inside this file's own `#[cfg(test)]` mod (which starts at line 1269).

`acknowledge_quest_events` is the sharpest instance, because its doc comment asserts a caller that does not exist: *"Fragment dispatch uses this after synchronously cascading its own SetStage calls so those same transitions are not dispatched again on its next cadence."* `fragment.rs` calls only `poll_quest_events`.

**Evidence**
```
$ grep -RIn "subscribe_to_quest_events\|subscribe_to_retained_quest_events\|unsubscribe_from_quest_events\|acknowledge_quest_events" \
      --include="*.rs" crates byroredux tools
  crates/scripting/src/quest_stages.rs:334,340,355,359          # the four definitions
  crates/scripting/src/quest_stages.rs:1497,1498,1535,1572,1604 # its own cfg(test) mod (starts 1269)
  →  no other file in the workspace mentions any of them

$ grep -RIn "poll_quest_events" --include="*.rs" crates byroredux
  crates/scripting/src/scene/playback.rs:494   → SCENE_QUEST_EVENT_SUBSCRIBER
  crates/scripting/src/fragment.rs:722, 2454   → FRAGMENT_QUEST_EVENT_SUBSCRIBER
  crates/scripting/src/quest_stages.rs:1171    → TERMINAL_QUEST_EVENT_SUBSCRIBER
```

**Impact**
Two competing subscriber-lifecycle models coexist in one type. A reader adding a fourth quest-event consumer has to work out which is canonical, and the tests actively suggest the wrong one (they use `subscribe_*`, production does not). `acknowledge_quest_events`'s doc describes fragment-dispatch behaviour that is not implemented, so a reader debugging duplicate stage dispatch is pointed at a mechanism that never runs.

**Related**: #2727 (catalog-drift guard only reachable via `#[ignore]`, same "tested path ≠ shipped path" shape, CLOSED), TD8-2026-09-05-06 (identical shape in the Starfield CDB provider)

**Suggested Fix**
Either delete the four methods and the dynamic half of `QuestEventLog` (rewriting the five tests to use the three constants, which is what they should be exercising), or — if dynamic subscription is genuinely wanted for mod-supplied consumers — wire at least one production caller and delete the static constants. Do not leave both. At minimum, correct `acknowledge_quest_events`'s doc comment, which currently asserts a caller that does not exist.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
