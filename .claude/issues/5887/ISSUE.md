# #5887: PEX-D4-2026-10-15-05: remote and custom event handler names take two shapes: `.psc` gives `Sender.Event`, `.pex` gives `Sender_Event`, and the VM's matcher binds only the latter

- **Labels**: low,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser (frontend parity)
- **Location**: `crates/papyrus/src/parser/script.rs:376-388` (`format!("{}.{}")`). The other side is `crates/pex/src/decompile/lower.rs:358-359`, which strips `::remote_` and leaves `Actor_OnDeath`. The matcher is `crates/scripting/src/fragment/events.rs:204-213`: `_<event>` suffix for remote events, exact mangled `script_Event` for custom events.
- **Status**: NEW (same family as open #5443)
- **Untrusted-Input**: No (consistency)
- **Description**: e96d687ec's rationale is that a dotted name makes name-matching consumers decline the handler. The engine's only consumer of these names, `handler_in`, wants to *bind* remote and custom handlers, and it matches the compiler's mangled spelling. So a `.psc`-sourced `Event ObjectReference.OnActivate(…)` or `Event QF_X.Ping(…)` never fires. The VM fixtures avoid this by hand-writing the mangled form in `.psc` (`fragment/vm/tests.rs:5255`, `Event ObjectReference_OnActivate(…)`).
- **Evidence**: `lower.rs:359` `name.strip_prefix("::remote_")`, and `events.rs:206-210` `lower.ends_with(&format!("_{event}"))`. Probe: `.psc` gives the name `Actor.OnDeath`.
- **Impact**: This is latent. `.psc` reaches the VM only through `script.exec`, which runs one statement and cannot define events, and through tests. A `.psc`-vs-`.pex` AST equality check is impossible for any FO4 script with remote handlers.
- **Suggested Fix**: Normalise at the parser boundary. Either the `.psc` frontend emits the compiler's `Sender_Event` spelling, which a script's own `OnDeath` still declines, or both frontends carry a structured `remote_sender: Option<Identifier>` on `Event`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
