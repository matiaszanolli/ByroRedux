# #4034 — REN-2026-09-06-D4-04: `image_health_docs_no_longer_claim_fence_alone_proves_host_visibility` scans `draw.rs` for a call site #3282 moved to `sync_and_acquire_frame.rs`

**Labels**: low, renderer, sync, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/resources.rs`
  (`image_health_docs_no_longer_claim_fence_alone_proves_host_visibility`, the
  `("draw.rs (collect_image_health call site)", draw_src)` entry in its
  three-way loop)
- **Status**: NEW — sibling of the open #3442, different pin and different file
- **Description**: #2740 corrected three comments that claimed a fence wait
  alone makes a device write host-visible (it does not — a fence's access scope
  is device-side only), and pinned the correction with a negative source scan
  over three files. One of the three is `draw.rs`, labelled *"collect_image_health
  call site"*. The #3282 split moved that call site — and the corrected comment
  attached to it — into `sync_and_acquire_frame.rs`. `draw.rs` no longer
  contains the string `collect_image_health` at all, so that third of the pin is
  vacuously green while the comment it was written to guard is unscanned.

  The live comment in `sync_and_acquire_frame.rs` is currently **correct**
  (*"The fence wait above proves submission completed (device-side access scope
  only) — it does NOT by itself prove the GPU write is host-visible"*), so there
  is no live defect — only a guard that has quietly stopped guarding.
- **Evidence**:
  - `grep -rn "collect_image_health" crates/renderer/src/` → definition and
    tests in `resources.rs`, the field doc in `context/mod.rs`, the init comment
    in `context/init.rs`, and the **call site in
    `context/sync_and_acquire_frame.rs`**. No hit in `draw.rs`.
  - The test builds its needles at runtime (`["provably", "idle"].join(" ")`)
    specifically so its own source cannot satisfy them — the technique is sound;
    only the file list is stale.
- **Impact**: Reintroducing the retired claim at the live call site passes
  `cargo test`. The same class as #3442, which is filed against the `(f + 1) %
  MAX_FRAMES_IN_FLIGHT` pin for the same reason.
- **Related**: #2740, #2793, #3282, #3442 (open, same class).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Replace the `draw.rs` entry with
  `include_str!("sync_and_acquire_frame.rs")` (keeping the `draw.rs` entry costs
  nothing and guards against the comment migrating back). No production change.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
