# TD1-2026-09-05-04: `mod-runtime/runtime.rs` holds 19 separate `impl <wit>::Host for HostState` blocks in one 3495-LOC file (the SKILL's per-binding axis is CORRECT — verified)

Labels: bug, low, tech-debt

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-04), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/mod-runtime/src/runtime.rs` (3495 production / 4588 total LOC)
- **Status**: NEW
- **Age**: created `9f619355`, 2026-08-06 — 312 total LOC at birth, **4588 today across 37 commits**
- **Description**: Unlike TD1-…-02, the axis the skill guessed here holds up under inspection. The
  file's WIT host surface is already physically partitioned into one `impl` block per interface;
  they are simply all in the same file. Rust allows `impl Trait for Type` in any module of the
  owning crate, so this is a pure relocation with no signature changes.
- **Evidence**: the 19 host-impl blocks, in file order —
  `events`, `state`, `wit_legacy_containers`, `wit_storage`, `world_state`, `content_catalog`,
  `actor_values`, `inventory`, `factions`, `faction_relationships`, `perks`, `packages`,
  `animation`, `reputation`, `world_spatial`, `script_functions`, `console`, `logging`, `context`
  — spanning ≈`:1591`–`:3495`, i.e. **~1900 of the 3495 production lines**. The remainder is:
  - `SandboxRuntime` (`new` / `config` / `catalog` / `compile` / `instantiate`) ≈570 LOC;
  - `ModInstance` (`initialize`, the ten `on_*` guest entry points, the `set_*_snapshot` setters,
    `reject_deferred_commands`, `shutdown`, `enter`, `quarantine`) ≈750 LOC;
  - `struct HostState` + its 17 `require_*` capability guards (`require_actor_values_read` …
    `require_storage_write`) ≈250 LOC — **the crate's trust boundary, currently buried between
    two host-impl blocks**;
  - ~25 free SDK↔WIT converters (`sdk_entity_ref`, `sdk_form_ref`, `sdk_storage_key`,
    `sdk_storage_value`, `wit_actor_value_state`, `wit_inventory_snapshot`, `wit_perk_snapshot`,
    `wit_entity_projection`, …) ≈240 LOC.

  One production function exceeds 200 LOC: **`SandboxRuntime::new` (389 LOC, `:218`–`:606`)**. It is
  not a construction chain — it is a declarative registration wall (25 `register_capability` /
  `register_service` / `CapabilityDescriptor` / `ServiceDescriptor` sites) around ~15 lines of real
  wasmtime setup (`Config`, `Engine::new`, `Linker::new`, `Extension::add_to_linker`).
- **Impact**: `crates/mod-runtime` is named in `_audit-common.md` as a **trust boundary with no
  owner audit skill**. The 17 `require_*` guards are the enforcement surface for that boundary and
  they are currently unreadable as a set, because they sit at `:3148` between the `console` and
  `logging` host impls. Concentrating them in one `capabilities.rs` is a review-quality win
  independent of the LOC count.
- **Related**: `/audit-safety` Dimension 11 owns this crate incidentally; TD1-…-01 (`extensions.rs`
  is its only host-side consumer).
- **Suggested Fix**: `runtime/{mod,sandbox,instance,host_state,capabilities,convert,host/*.rs}` —
  one file per WIT interface under `host/`, the guards in `capabilities.rs`, the converters in
  `convert.rs`. Separately, lift `SandboxRuntime::new`'s descriptor list into a
  `const CAPABILITY_DESCRIPTORS: &[(&str, &str)]` + `const SERVICE_DESCRIPTORS: &[…]` table and
  loop over it — that alone removes ~350 of its 389 lines.
- **Effort**: medium

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved

