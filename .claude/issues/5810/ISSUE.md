# TD2-2026-10-15-09: Eight extension dispatch systems repeat the 25-line host-acquire -> lock -> dispatch -> apply -> drain -> emit -> fault-warn tail (the ECS-adapter half of what #5592 consolidated in `dispatch.rs`)

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-09`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/extensions/systems.rs:247-289` (custom events), `:546-612` (activation), `:615-663` (cell load), `:665-742` (equipment), `:744-796` (input), `:798-835` (session), `:837-935` (hit), `:937-962` (update); host acquisition alone appears 15 times (`ExtensionHostSlot>()` at `:56..:939`)
- **Status**: NEW (#5592 closed the `ExtensionHost::dispatch_*` side, `91a28a4c9`, 2026-10-14)
- **Age**: `55cbc0d6c` (#3843 split, 2026-09-11)
- **Effort**: small
- **Description**: each system ends with `let mut host = host.lock().expect("ExtensionHost mutex poisoned by a host panic"); let stats = host.dispatch_X(...); apply_pending_world_commands(world, &mut host); let diagnostics = host.take_diagnostics(); drop(host); emit_diagnostics(diagnostics); if stats.faults > 0 { log::warn!("extension X dispatch: events={} deliveries={} commands={} faults={}", ...) }`, and the same 10-line `slot.host()` preamble.
- **Evidence**: clone scan `:326-348` vs `:754-793`; eight textual copies of the `events={} deliveries={} commands={} faults={}` format string.
- **Impact**: the next dispatch family (or any change to the drop-before-emit ordering, which is lock-order-sensitive after #5535) is eight edits.
- **Related**: #5592, #5535.
- **Suggested Fix**: `fn with_extension_host(world: &World, label: &str, f: impl FnOnce(&mut ExtensionHost) -> DispatchStats)` in `systems.rs` owning the preamble and the whole tail; each system keeps only its event capture and the one `host.dispatch_*` call.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
