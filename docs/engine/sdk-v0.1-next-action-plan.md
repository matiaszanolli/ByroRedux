# ByroRedux SDK: next action plan

Status: **in progress**
Checkpoint: `44ad6b2b` (`feat(scripting): preserve SCPT load-order name probes`)
Date: 2026-09-01

This is the hand-off plan after the first extended SDK implementation pass.
The engine-owned boundary is useful already, but the SDK is not finished and
the v0.1 release gates are not met. Work should continue one bounded slice at
a time, with tests, a conventional commit, and an Ensue checkpoint for each
slice.

## Priority order

### 1. Reconcile the contract and documentation

- Update the main development plan so its older “remaining StorageUtil list”
  wording matches the delivered list, array, slice, and form-filter packs.
- Keep the unsupported boundary explicit: file-backed and object-scoped
  StorageUtil state, cross-mod shared state, JContainers JDB/path/JSON/Lua and
  keyed-map packs, physical input polling/injection, menu registration or
  mutation, arbitrary Scaleform access, and binary extender DLLs.
- Add a single compatibility status table linking each provider alias to its
  route, capability policy, fixture, and known semantic gaps.

Done when the plan has no contradictory “implemented/pending” claims and a
reader can distinguish a native engine service, a compatibility alias, and an
unsupported call.

### 2. Finish the provider execution substrate

- **Delivered:** typed integer/float arithmetic and string concatenation now
  lower from source and execute through the engine provider host, including
  nested provider results and continuation-safe assignments. Save format v17
  records the new expression nodes.
- **Delivered in this slice:** `Game.GetPlayer()` now resolves through the
  engine's canonical `PlayerEntity` resource and the same stable generational
  entity-handle registry used by sandbox projections. No-player/flycam worlds
  return `None`, and malformed arguments fail closed.
- **Delivered with the player bridge:** Papyrus `ObjectReference` locals can
  hold nullable opaque entity results and pass them to typed provider calls;
  receiver-method dispatch and entity comparisons remain intentionally out of
  scope.
- Continue with receiver object expressions and the next justified latent
  primitive.
- Add broader event coverage only when the canonical ECS payload and save
  behavior exist; preserve guard-free guest entry and whole-handler rejection
  on unsupported syntax.
- Complete source/byte-level PEX parity tests, continuation migration tests,
  and malformed-result/fault isolation coverage.

Done when an equivalent `.psc` and byte-level `.pex` fixture produces the same
typed result and deferred mutation across a wait and save/reload boundary.

### 3. Close the preserved-SCPT bridge deliberately

- **Delivered in this slice:** source-backed `GetNthModName <index>` now
  lowers through the same bounded immutable content catalog as the existing
  SCDA opcode path. Source and source-less conditional fixtures agree, and a
  non-numeric source argument rejects translation before attachment.
- Extend the bounded SCDA expression evaluator and non-literal argument
  handling only for commands with a semantic SDK route.
- Add the remaining safe load-order/content commands and explicit diagnostics
  for `GetSourceModIndex`, reference construction, and unsupported xNVSE/OBSE
  operations.
- Keep eager archive scans, command-pack classification, and general ObScript
  execution as separate milestones; do not infer behavior from unknown opcode
  bytes.

Done when source-backed and source-less SCPT fixtures agree, malformed records
fail closed, and unsupported commands are reported before launch.

### 4. Add compatibility packs behind canonical services

Implement these in fixture-driven slices, in this order:

1. **Content/form metadata:** names, keywords, weights, stable lookup, and
   source/projectile form projection where the resolver can prove identity.
2. **Input:** custom action registration and rebinding-aware queries; defer
   physical polling/injection until a trust, lifetime, and capability contract
   is written.
3. **UI:** a bounded menu registration/data/action service for engine-owned
   widgets and notifications; never expose arbitrary Scaleform objects or
   mutation of renderer internals.
4. **StorageUtil/JContainers follow-ups:** file-backed collections, object
   scopes, JDB/path/JSON/Lua, keyed maps, pools, and timed release only where
   bounded persistence and principal isolation are well-defined.

Done for each slice means a real source/PEX call site, a semantic route, a
negative/unsupported test, and no dependency on an extender DLL.

### 5. Complete gameplay and lifecycle gaps

- Add the remaining actor services (magic, appearance, broader animation
  graph control, inventory instance mutation) only after stable identity and
  atomic reconciliation rules exist.
- Define safe spawn/despawn and reference lifecycle semantics; never return
  raw ECS IDs.
- Finish schema migrations, corrupt/missing-extension/downgrade handling,
  status and resource telemetry, in-memory-host parity, and richer
  new-game/cell/save transitions.

Done when the same lifecycle fixture passes in-memory and live, reverse-order
shutdown is observable, and save/load failures isolate one extension without
damaging the base save or another principal.

### 6. Run conformance and release gates

- Add at least one real extender-dependent fixture per covered compatibility
  pack and record supported, mapped, and unsupported calls before launch.
- Run the full SDK, runtime, scripting, executable, documentation, safety,
  and Studio gates from the main development plan.
- Publish the v0.1 compatibility statement and porting guidance for native
  plugins; explicitly state that binary ABI/memory-hook plugins must be
  ported.

Done when all required gates pass and every claimed alias has behavior,
persistence, and failure-handling evidence.

## Session discipline

Do not attempt the whole backlog in one session. Select one numbered slice,
write its contract and negative cases first, implement it, run the narrow and
workspace-relevant tests, commit only owned files, then update Ensue with the
commit, tests, and remaining boundary. Generated codebase-memory artifacts and
unrelated lockfile changes stay out of feature commits.

## Explicit non-goals

The project will not load SKSE/F4SE/xNVSE/OBSE DLLs, emulate their binary ABI,
expose process addresses or raw pointers, or claim compatibility for an
unimplemented call merely because its provider name is recognized.
