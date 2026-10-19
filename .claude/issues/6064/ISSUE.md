# #6064 — SAFE-2026-10-19-03 (snapshot as filed)

Labels: low,safety,renderer,documentation,doc-rot
Report: docs/audits/AUDIT_SAFETY_2026-10-19.md

**Severity**: LOW (documentation of an `unsafe fn` contract). `pub(crate)` items are outside `clippy::missing_safety_doc`; no caller is mis-written today.
**Dimension**: Dim 4 (unsafe-block discipline: invariant attached to the right function)
**Source**: `docs/audits/AUDIT_SAFETY_2026-10-19.md` (SAFE-2026-10-19-03)

## Location
`crates/renderer/src/vulkan/descriptors.rs:209-266` (introduced by `31f4919d5`, #5809, 2026-10-15). A sibling splice sits in the same file's test module (`:764-773`): the #5845 doc of `groundcover_pools_carry_no_hand_typed_frame_counts` is appended to the previous test's doc, losing that test's own doc line.

## Description
`clamp_sampler` was inserted between `memory_barrier`'s doc comment and its item, so:
- `memory_barrier` lost its whole doc block, including `# Safety — cmd must be a recording command buffer owned by the caller`, and its `#[inline]`.
- `clamp_sampler` (`pub(crate) unsafe fn`) now carries that command-buffer `# Safety` paragraph and the `#[inline]`, while its real contract (device live; caller owns and later frees the returned `vk::Sampler`) appears only as an inner `// SAFETY:` comment.

## Evidence
`git blame -L 223,232`: lines 223-225 from `9c9e37d5dd`, 226-232 from `31f4919d57`. The doc block reads `… validation layers catch mismatches at runtime.` / `#[inline]` / `/// #5809 — the clamp-to-edge …` / `pub(crate) unsafe fn clamp_sampler`.

## Impact
Reviewers of ~13 `memory_barrier` call sites no longer see its safety contract; reviewers of `clamp_sampler` see a command-buffer contract on a sampler constructor. Same class as closed #5029 / #5909.

## Suggested Fix
Move the `clamp_sampler` block above the `// ── MemoryBarrier (global) helper ──` banner (or below `memory_barrier`), restore `memory_barrier`'s doc + `#[inline]`, give `clamp_sampler` its own `# Safety`, re-join the test doc. Cheap guard: a source-scan test that no `#[inline]` line is directly followed by `///`.

## Completeness Checks
- [ ] **UNSAFE**: Each `unsafe fn` here carries a `# Safety` stating its own contract
- [ ] **SIBLING**: Other `#[attr]`-after-`///` splices checked (also `cell_loader/spawn.rs:449-450`, `cell_loader/terrain.rs:588-590`)
- [ ] **TESTS**: Source-scan guard against `#[inline]` directly followed by `///`
