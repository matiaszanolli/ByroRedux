# TD1-2026-09-05-03: `papyrus_provider.rs` is a compiler front-end, an IR, and an interpreter in one 3711-LOC file

Labels: bug, low, tech-debt, scripting

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-03), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/scripting/src/papyrus_provider.rs` (3711 production / 6158 total LOC)
- **Status**: NEW
- **Age**: created `6254d996`, 2026-09-01 ("feat(scripting): lower typed Papyrus provider calls") — 350 total LOC at birth, **6158 today across 51 commits in 4 days**
- **Description**: The module doc says it "resolves a legal `Provider.Function(...)` … to the
  principal-qualified SDK route and validates typed arguments, but it never enters Wasm or touches
  the ECS while lowering." That describes the *first half* of the file. The second half is a
  full statement interpreter that does touch the ECS: `papyrus_provider_system` (361 LOC — the
  file's only >200-LOC function), `execute_statements`, `evaluate_condition`, `evaluate_provider_value`,
  `apply_provider_arithmetic`, `compare_condition_values`, `materialize_provider_arguments`. The
  lowering half and the execution half are roughly equal in size and share only the IR types.
- **Evidence**: the file falls into five contiguous, non-interleaved regions:

  | Region | Symbols (first → last) | ≈LOC |
  |---|---|---|
  | runtime plumbing | `PapyrusProviderRuntime` → `register` | 130 |
  | catalog | `PapyrusProviderRoute` → `PapyrusProviderCatalog::contains_provider` | 120 |
  | call lowering (front-end) | `TypedPapyrusProviderCall` → `lower_literal` (incl. `storage_util_arity`, `legacy_container_arity`, `validate_*_arity`, `lower_provider_invocation` at 194 LOC) | 650 |
  | IR + resources | `PapyrusProviderEvent` → `PapyrusProviderHandler::projected_mod_event_locals` | 400 |
  | program lowering (AST → IR) | `lower_provider_program` → `resolve_mod_event_senders` (incl. `lower_statements` at 152 LOC, `lower_condition_at_depth`, `sdk_type`, `default_value`) | 1130 |
  | execution (back-end) | `papyrus_provider_system` → `compare_ordered` | 1160 |

  `MAX_PROVIDER_HANDLER_NESTING`/`MAX_PROVIDER_CONTINUATIONS`/`MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS`/
  `MAX_PENDING_PAPYRUS_MOD_EVENTS` are the only symbols the two halves genuinely share besides the IR.
- **Impact**: the two halves have different test shapes (lowering is pure and table-testable;
  execution needs a `World`) but currently share one 6158-line `#[cfg(test)]` boundary, so every
  lowering test recompiles the interpreter. Same 4-day growth-rate amplification as TD1-…-01/02.
- **Related**: TD1-…-02 (the `byroredux_sdk::compatibility` route constants it imports 24 of).
- **Suggested Fix**: `papyrus_provider/{mod,runtime,catalog,ir,lower_call,lower_program,execute}.rs`.
  The IR module is the natural seam — it is what both halves import and nothing else does. Nothing
  crosses a lock or scheduler boundary, so the move is mechanical.
- **Effort**: medium

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved

