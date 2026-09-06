# #4038 — REN-2026-09-06-D5-04: the three `pending_destroy_*` accessors have no caller anywhere in the workspace, and their doc comments name a `ctx.scratch` surface and a unit test that do not exist

**Labels**: low, memory, renderer, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (observability / dead API)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` —
  `AccelerationManager::pending_destroy_blas_count`,
  `pending_destroy_scratch_count`, `pending_destroy_static_bytes`.
  Claimed surface: `CtxScratchCommand` (`byroredux/src/commands/world_info.rs`).
- **Status**: **NEW.** Same class as `REN-2026-09-05-D1-02`
  (`TlasIntegritySnapshot`), which has now survived two sweeps without an
  issue number — worth filing together.
- **Description**: All three are `pub`, all three are documented as telemetry
  surfaces, and `grep -rn` across the whole workspace returns for each only
  its own definition plus doc references — zero call sites, zero test uses.
  The doc comments are specific and wrong:
  - `pending_destroy_static_bytes`: *"Companion to
    [`Self::pending_destroy_blas_count`] for `ctx.scratch` telemetry — the
    count alone can't show how much VRAM the queue is holding."*
    `CtxScratchCommand::execute` reads only `ScratchTelemetry.rows` (CPU-side
    `Vec` len/capacity rows produced by `VulkanContext::fill_scratch_telemetry`
    and `build_render_data`) plus the material dedup ratio. There is no
    deferred-destroy row, and `fill_scratch_telemetry` cannot be adding one —
    it would be a call site, and there are none.
  - `pending_destroy_blas_count`: *"Surfaced for [`drain_pending_destroys`]'s
    unit test and shutdown telemetry — the count must reach zero after a
    drain."* No such test exists (`AccelerationManager` needs a live device),
    and nothing logs it at shutdown.
  - `pending_destroy_scratch_count`: *"Surfaced for the deferred-destroy
    regression test and shutdown telemetry."* Same.
- **Evidence**: `grep -rn "pending_destroy_static_bytes()\|pending_destroy_blas_count\|pending_destroy_scratch_count" --include='*.rs' .`
  → definitions and doc-links only. `CtxScratchCommand::execute`'s body
  reads `tlm.rows`, `tlm.renderer_row_count`, `tlm.materials_*` and nothing
  else.
- **Impact**: A deferred-destroy queue that stops draining — the failure mode
  the countdown exists to make impossible, and the one a shortened countdown
  or a missed tick would produce — is unobservable at runtime. There is no
  console surface, no log, and no assertion. Secondarily, three doc comments
  assert a telemetry integration that a reader can reasonably act on
  ("`ctx.scratch` will tell me how much the queue holds") and will not find.
- **Related**: `REN-2026-09-05-D1-02` / `REN-2026-08-30-D1-01`
  (`TlasIntegritySnapshot`, the same "computed, `pub`, no reader" pattern in
  the same subsystem), #1228 (the underlying AS-telemetry gap),
  `REN-2026-09-06-D5-03` (the fourth #3840 symbol with no effective consumer).
- **Suggested Fix**: Add three rows to `ScratchTelemetry` from
  `VulkanContext::fill_scratch_telemetry` — `pending_destroy_blas`,
  `pending_destroy_scratch` (counts) and `pending_destroy_static_bytes`
  (bytes) — which makes all three doc comments true and gives `ctx.scratch`
  the queue-depth view it already claims to have. If that is not wanted,
  delete the accessors and the sentences that promise them; a `pub` accessor
  with no reader in a binary-only workspace is the #3884 class the project
  just spent a commit removing.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
