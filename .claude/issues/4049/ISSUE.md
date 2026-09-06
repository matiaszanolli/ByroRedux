# #4049 — REN-2026-09-06-D9-03: the `bind_inverses` upload-failure path retries unboundedly with an un-gated per-frame `warn!`, against this subsystem's own convention

**Labels**: low, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D9-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (the `unwrap_or_else` on `upload_pending_bind_inverses`)
- **Status**: NEW
- **Description**: The #3569 requeue turns a failed `bind_inverses` upload into an
  indefinite per-frame retry: the entries go back into `SkinSlotPool::pending_uploads`
  (prepended, so they are drained first next frame), get re-attempted, and — if the
  underlying host-visible map/flush failure is persistent rather than transient — fail
  again, log again, and requeue again, forever. `log::warn!("Failed to upload pending
  bind_inverses: {e}")` has no `Once` gate, no rate limit, and no attempt counter. Every
  sibling failure path in this exact file and its callee already has one:
  `failed_skin_slots` (#900, added because a retried `create_slot` logged 58 WARNs per 300
  frames), `failed_skin_blas` (#2802, the BLAS sibling of the same fix), and
  `SkinSlotPool::overflow_warned` (one-shot, with a silent `overflow_attempt_count` for the
  magnitude). This path is the odd one out, and #3569 is what made it retry at all.
- **Evidence**: `bind_inverse_upload_failed` is set unconditionally in the error arm with no
  counter alongside it; `requeue_pending` unconditionally re-inserts. `grep -rn
  "bind_inverse_upload_failed"` returns one write-true site, one reset site, and one reader
  (`app_frame.rs`) — no suppression state anywhere.
- **Impact**: On a persistently failing device (OOM on the upload heap, device-lost
  in progress), one WARN per frame per failing batch until the cell unloads — the same
  log-flooding #900 and #2802 were filed to stop, plus a per-frame staging write + flush
  attempt that will not succeed. It also masks the *first* failure in the flood, which is
  the diagnostically useful one.
- **Related**: #3569, #900, #2802, `SkinSlotPool::overflow_warned`. Pairs with `D9-01`
  (same error arm) and `D9-04` (same requeue).
- **Suggested Fix**: Add a bounded-retry counter (or a `Once`-gated warn plus a silent
  cumulative count surfaced through `SkinCoverageStats` / `skin.coverage`, mirroring
  `overflow_attempt_count`), and after N consecutive failures stop requeuing and instead
  route the affected slots through the `D9-01` "defined fallback" so they render bind-pose
  rather than retrying forever.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
