# Safety Audit — 2026-07-06

**Scope:** `/audit-safety` full sweep — unsafe-block invariants, memory-corruption / UB,
per-frame / per-cell leaks, Vulkan spec compliance, FFI lifetimes across the cxx bridge.
Part of the *nif-deep* suite: extra attention paid to `crates/nif/` unsafe code and the
`allocate_vec` / bounded-allocation paths.

**Method:** Grepped all `unsafe` in `crates/` + `byroredux/` (`unsafe`=596 in renderer,
7 in nif, 6 in core, 1 each in cxx-bridge/pex, 2 in byroredux; 0 in save/sfmaterial/
plugin/bgsm/physics/audio/spt/facegen). Re-read every SAFETY comment against its call
site. Verified the ten regression guards the SKILL enumerates. Deduped against 36 open
issues (`/tmp/audit/issues.json`) + closed-issue search.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 0 |
| **Total NEW** | **2** |

Two NEW findings, both MEDIUM. No CRITICAL/HIGH: every acceleration-structure, drop-order,
FFI-lifetime, and repr(C)-layout invariant checked out intact. The renderer FFI mass is
disciplined (the raw unsafe-vs-SAFETY gap is inflated by inline-first-line SAFETY comments
and `unsafe fn` decls — see SAFE-D4-01).

---

## NEW Findings

### SAFE-D2-01: Header-parser `read_sized_string` allocates unbounded (residual #388 gap)
- **Severity**: MEDIUM
- **Dimension**: 2 — Memory Corruption / UB (unbounded allocation on malformed input)
- **Location**: `crates/nif/src/header.rs:387-395` (`read_sized_string`); reached from
  `header.rs:192` (block-type names) and `header.rs:266` (string-table entries)
- **Status**: NEW
- **Description**: The #388 OOM-hardening sweep added `MAX_SINGLE_ALLOC_BYTES` (256 MB) +
  remaining-bytes guards via `NifStream::check_alloc`, but only to the **stream body**.
  The header parser runs *before* the `NifStream` wrapper exists and reads its inline
  strings through `read_sized_string`, which does `vec![0u8; len]` with `len` an untrusted
  u32 straight from the file — no cap, no remaining-bytes check before the allocation.
  The surrounding *count* guards (`num_block_types`, `num_strings`) bound how many strings
  are read, but not the *length* of each. A corrupt block-type name or string-table entry
  can therefore claim `len = 0xFFFF_FFFF` and force a single ~4 GB zeroing allocation
  before `read_exact` fails and the parse aborts cleanly.
- **Evidence**:
  ```rust
  // header.rs:387
  fn read_sized_string(cursor: &mut Cursor<&[u8]>) -> io::Result<String> {
      let len = read_u32_le(cursor)? as usize;
      let mut buf = vec![0u8; len];          // <- no check_alloc / cap; len up to u32::MAX
      cursor.read_exact(&mut buf)?;
      ...
  }
  ```
  Contrast the guarded body path `stream.rs:220` (`read_bytes` → `check_alloc(len)?`) and
  the bulk path `stream.rs:360` (`read_pod_vec` → `check_alloc(byte_count)?`). `#388`
  (CLOSED, "OOM abort on NiTextKeyExtraData — unchecked Vec::with_capacity") fixed the
  stream side; the header string readers were left out of that sweep.
  (`read_short_string`, `header.rs:397`, uses a u8 length → max 255 bytes → not a concern.)
- **Impact**: A crafted/corrupt NIF *header* triggers a transient ~4 GB allocation per
  offending string on any of the seven supported games' load paths. Bounded at u32 and
  fails cleanly afterward, so not a corruption/UB hazard — but on a memory-constrained host
  a single such alloc can OOM/abort the process. Malformed-input DoS / defense-in-depth gap;
  the stream body already treats this exact class as worth guarding.
- **Related**: #388 (CLOSED, stream-side fix); #113 (alloc-cap origin); Dimension 2.
- **Suggested Fix**: Thread the same budget check into the header readers — either a free
  helper `check_header_alloc(len, cursor.get_ref().len() - cursor.position())` mirroring
  `check_alloc` (cap at `MAX_SINGLE_ALLOC_BYTES` + reject `len > remaining`), or reject
  `len > remaining` inline before `vec![0u8; len]` in `read_sized_string`.

### SAFE-D4-01: ~134 renderer FFI `unsafe {}` blocks carry no SAFETY comment
- **Severity**: MEDIUM
- **Dimension**: 4 — Unsafe-Block Discipline (batched, per `_audit-severity` Special Rule
  "unsafe block without safety comment = MEDIUM")
- **Location**: `crates/renderer/src/vulkan/` (batched). Representative sites:
  `buffer.rs:734`, `water.rs:236,273,308,330,653,660`, `instance.rs:89,111`,
  `context/mod.rs:1677,1779,1990,1996,2041,2088`.
- **Status**: NEW
- **Description**: Of 607 `unsafe fn`/`unsafe {}` tokens in the renderer, 70 are `unsafe fn`
  declarations (obligation delegated to callers — acceptable) and ~134 are `unsafe {}`
  blocks with no `SAFETY:` note in a ±5-line window. Spot-checks confirm these are almost
  all single ash object-creation FFI calls (`create_descriptor_set_layout`, `create_fence`,
  `create_graphics_pipelines`, …) — individually low-hazard (no raw-pointer lifetime, no
  mapped-memory `from_raw_parts`), but each still trips the project's own
  "unsafe-without-comment = MEDIUM" rule. NOTE: the raw grep gap (596 unsafe vs 403 SAFETY)
  overstates the problem — many blocks put the `SAFETY:` comment on the first line *inside*
  the block (e.g. `buffer.rs:233`), which a naive preceding-line scan misses; the true
  count of genuinely undocumented blocks is ~134, not ~190.
- **Evidence**:
  ```rust
  // water.rs:236 — no SAFETY comment
  let water_caustic_set_layout = unsafe {
      device.create_descriptor_set_layout(&...CreateInfo::default().bindings(&...), None)
  };
  // context/mod.rs:1677 — no SAFETY comment
  let transfer_fence = Arc::new(Mutex::new(unsafe {
      device.create_fence(&vk::FenceCreateInfo::default(), None).context("...")?
  }));
  ```
- **Impact**: Documentation/hardening only — no unsound invariant found among the sampled
  sites. Left uncommented, a future edit to any of these blocks (e.g. adding a
  `from_raw_parts` on mapped memory next to an existing create-call) inherits no stated
  invariant to check against. Bread-and-butter Dimension-4 sweep item.
- **Related**: `_audit-severity` Special Rules; SKILL Dimension 4 ("batch the comment-less
  blocks"). Distinct from #1861 (a specific fence/cmd-buffer *leak* on error paths, already
  OPEN).
- **Suggested Fix**: Add a one-line `SAFETY:` to each block tying it to the standard ash
  precondition (device live, handles created by this device, not in use by an in-flight
  command buffer). Best done file-by-file; not urgent. Consider a `clippy::undocumented_unsafe_blocks`
  lint (allow at crate root, deny per-file as files are cleaned) to prevent regrowth.

---

## Regression Guards Verified (PASS — not findings)

Each of these is a guard the SKILL flags; all confirmed intact against current code:

- **Dimension 1 — cxx bridge is still a no-pointer placeholder.** `crates/cxx-bridge/src/lib.rs`
  exposes only `fn native_hello() -> String` (owned cxx::String return, no `*const`,
  `&[u8]`, `Box<…>`, or Rust-reference-taking `extern "C++"`). Dimension stays dormant. PASS.
- **Dimension 2 — ECS cached-pointer contract (#35/#1367).** `StorageRef`/`StorageRefMut`
  (`query.rs:58-144`) and `ComponentRef::deref` (`query.rs:282-291`) cache a `*const`/`*mut`
  resolved once in `new()`; every deref carries a SAFETY comment tying validity to the pinned
  lock guard, and `&mut *self.storage` is gated behind `&mut self` (`storage_mut`, `query.rs:139`).
  No guard drops before its pointer. PASS.
- **Dimension 2 — NIF POD reads.** `read_pod_vec` (`stream.rs:350`) and the header mirror
  `read_pod_vec_from_cursor` (`header.rs:360`) both keep the `count.checked_mul(size_of)` overflow
  guard; `read_pod_vec` routes through `check_alloc`; the `AnyBitPattern` sealed trait
  (`stream.rs:47`, impls at `stream.rs:63` + `bs_geometry.rs:338-340`) still blocks
  `read_pod_vec::<bool>`. PASS. (Note: the header mirror relies on the *caller* to bound-check
  — see SAFE-D2-01 for the residual gap in the sibling string reader, not the POD reader.)
- **Dimension 2 — pex `OpCode::from_u8` transmute.** `opcode.rs:130-137`: `#[repr(u8)]`,
  discriminants contiguous `0..51` (only `Nop = 0` explicit, rest implicit), `byte >= MAX_OPCODE`
  (=51) rejected before the transmute. Guard + contiguity both hold. PASS.
- **Dimension 3 — AllocatorResource removed before VulkanContext drop (#1406/#1640).**
  `main.rs:461-467` (App::drop) and `main.rs:2193-2199` (CloseRequested) both call
  `remove_resource::<AllocatorResource>()` before the renderer drops; idempotent across both
  paths. PASS.
- **Dimension 3 — deferred-destroy tick after fence wait (#418/#732/#1782).** `context/draw.rs`
  runs `tick_deferred_destroy` (2261-2265) AFTER `wait_for_fences` (2149); an in-source test
  (draw.rs:4271-4287) anchors the ordering. Retired BLAS scratch routes through
  `pending_destroy_scratch` deferred-free (`cell_loader/unload.rs:139-143`). PASS.
- **Dimension 3 — Rapier bodies released on cell unload (#1520).** `rapier_release_tests.rs`
  asserts `body_count`/`colliders.len()` drop to the surviving cell's count post-unload. PASS.
- **Dimension 5 — TLAS resize wait (#1390).** `acceleration/tlas.rs:342`
  `device.device_wait_idle()` precedes the old-allocation free in the resize branch. PASS.
- **Dimension 6 — GpuMaterial 300-byte pin.** `material.rs:39-42,273` document + assert the
  300 B size (`anisotropic` at offset 296 → total 300), pinned by `gpu_material_size_is_300_bytes`.
  Prose and asserted size agree. PASS.

## Premises Checked and Disproved (no finding)

- **"Renderer has a large SAFETY-comment deficit (596 unsafe vs 403 SAFETY)."** Disproved as
  stated — the deficit is inflated by inline-first-line SAFETY comments (missed by a
  preceding-line scan, e.g. `buffer.rs:233`) and by `unsafe fn` declarations. The real
  undocumented-block count is ~134 (folded into SAFE-D4-01), not ~190.
- **"cxx bridge string lifetime / dangling pointer across FFI."** Disproved — the SKILL warns
  this surface does not exist; confirmed the bridge still exchanges no borrowed pointers.
- **Vulkan barrier/layout spec claims.** Per the repo's No-Speculative-Vulkan-Fixes guardrail,
  no barrier/layout/sync finding is asserted: the volumetrics dispatch gate, caustic
  CLEAR-before-COMPUTE, and per-frame GENERAL-layout invariants are guarded by existing tests /
  regression fixes and I traced no concrete live hazard. Any future claim here needs
  validation-layer or RenderDoc evidence, not static reasoning.

---

*Report generated by `/audit-safety`. To file findings as issues:*
```
/audit-publish docs/audits/AUDIT_SAFETY_2026-07-06.md
```
