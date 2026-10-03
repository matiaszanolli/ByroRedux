# #5194 — REN-D9-2026-10-03-02: The #679 forced rebuild drops a working skinned BLAS before its replacement exists. A failed rebuild leaves the actor out of the TLAS until an unrelated eviction.

**Labels**: medium,renderer,vulkan,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM. The error path is recoverable but unhandled. The effect is visual only: one actor's RT shadows, reflections and GI are lost.
- **Dimension**: Skinning
- **Location**:
  - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`, `record_skinned_blas_refit`: the `if accel.should_rebuild_skinned_blas(entity_id) { … accel.drop_skinned_blas(entity_id); }` arm, the `failed_skin_blas.insert` in the build-result `Err` arm, and the `failed_skin_blas.clear()` in the eviction branch.
  - `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`, `build_skinned_blas_batched_on_cmd`: the Phase 1 result-buffer / AS-create `Err` arms, the Phase 2 scratch-grow `Err` arm, and the Phase 4 `self.drop_skinned_blas(p.entity_id)` before insert.
- **Status**: NEW. Searched "skinned BLAS rebuild drop", "should_rebuild_skinned_blas", "failed_skin_blas", "refit threshold rebuild fails". #4884 (CLOSED) fixed the same shape for the scratch buffer only.
- **Description**:
  - **Drop happens before the attempt.** Once `refit_count >= skinned_blas_refit_limit(entity)`, the first-sight loop calls `drop_skinned_blas` immediately. The old BLAS goes onto `pending_destroy_blas` with `DEFAULT_COUNTDOWN`, so its memory is still held for two more frames. The entity is then queued as a fresh BUILD.
  - **What happens on failure.** If that BUILD fails (result-buffer allocation, AS create, or the scratch grow #4884 just made non-destructive), three things follow:
    - the entity is inserted into `failed_skin_blas`;
    - the refit loop skips it (`!accel.has_skinned_blas`);
    - `build_tlas` omits it (`missing_skinned_blas`).
  - **Retry is gated on an unrelated event.** The retry is suppressed until `failed_skin_blas.clear()`, which runs only when the SkinSlot eviction pass evicts *something*. In a stable interior where every actor stays in view, that never happens.
  - **Why the BUILD is likely to fail here.** The replacement must be allocated while the dropped BLAS's memory is still pending destroy. Under VRAM pressure, an allocation that the in-place UPDATE never needed is exactly the one likely to fail.
  - **The phases already support build-then-swap.** Phase 4 already calls `drop_skinned_blas` just before inserting the new entry (#2481), so the call-site pre-drop is not needed for correctness on success.
  - **The rebuild is routine.** With #3669's jitter, every moving actor passes through this window every 600–660 dirty frames.
- **Evidence**:
  ```rust
  if accel.should_rebuild_skinned_blas(entity_id) {
      log::info!(…"refit chain reached {} frames, dropping for fresh BUILD (#679)"…);
      accel.drop_skinned_blas(entity_id);              // live BLAS gone here
  }
  let needs_blas = accel.skinned_blas_entry(entity_id).is_none();
  …
  Err(e) => { log::warn!(…"first-sight BLAS build failed"…); self.failed_skin_blas.insert(entity_id); }
  ```
  The #4884 commit message names the outcome it was fixing ("freezing RT shadows/reflections/GI on animated NPCs under exactly the VRAM pressure that triggers the shrink"). The forced-rebuild path reaches the same outcome by a different route.
- **Impact**: Under VRAM pressure, a moving NPC that hits its periodic rebuild can permanently (per cell stay) lose its RT shadow, reflection and GI contribution. Without the rebuild it would only have a degraded-quality BVH. Raster is unaffected. Telemetry shows it as `missing_skinned_blas` plus one WARN.
- **Related**: #679 (forced rebuild), #3669 (jitter), #4884 (scratch allocate-before-retire), #2481 (Phase 4 drop-before-insert), #2802 (`failed_skin_blas`).
- **Suggested Fix**:
  - Do not pre-drop on the rebuild arm. Queue the entity for BUILD while its old entry stays live, and let Phase 4's existing `drop_skinned_blas` retire the old BLAS only after the new one is recorded.
  - On `Err`, keep refitting the old BLAS (reset or saturate `refit_count`) instead of recording it in `failed_skin_blas`.
  - Add a source-order pin.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
