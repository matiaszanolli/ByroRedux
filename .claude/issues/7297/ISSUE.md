# #7297: SCR-D2-03: A Papyrus message box the UI queue refuses leaves its ticket waiting forever; the run parks until a load or 64 later tickets

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No (mod scripts can spam boxes, #6012's premise)
- **Location**: `byroredux/src/notifications.rs:48-62` (`route_script_messages`), `crates/scripting/src/messages.rs:159-172` (`issue`), `:190-197` (`take_papyrus_answer`)
- **Status**: NEW (the drop-at-16 bound is #6012, CLOSED; this is its consequence for the new ticketed box)
- **Description**: `route_script_messages` refuses a box when 16 are already waiting ("script message box dropped") and never calls anything for the dropped message. For `MessageAnswer::Papyrus(ticket)` the ticket was already issued into `PapyrusMessageAnswers::waiting`, so `take_papyrus_answer` returns `None` forever and `fragment_continuation_system` keeps the run in `still_pending`. Only `issue()`'s eviction at 64 outstanding tickets or `forget_boxes` on a live load ever releases it. The skill names "a ticket nothing ever answers" as the finding.
- **Evidence**:
  ```rust
  // notifications.rs:51-59
  if boxes.0.len() >= MAX_PENDING_BOXES { log::warn!("script message box dropped, ...") } else { boxes.0.push_back(message); }
  // messages.rs:195-197
  (!answers.waiting.contains(&ticket)).then_some(0)   // None while the ticket is "waiting"
  ```
- **Impact**: A quest run (e.g. a polled `OnUpdate` handler that `Show()`s, or any flood of boxes) that hits the cap hangs silently; the continuation also occupies a `FragmentExecutionQueue` slot and is re-polled every frame.
- **Related**: #6012, SCR-D2-02.
- **Suggested Fix**: In the drop branch, if `message.answer` is `Papyrus(ticket)`, call `record_papyrus_answer(world, ticket, 0)` (the box's default) so the run resumes with button 0 instead of hanging.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
