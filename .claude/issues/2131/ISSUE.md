**Severity**: HIGH · **Dimension**: Renderer / Vulkan safety hygiene
**Source**: `docs/audits/AUDIT_REGRESSION_2026-07-25.md` (REG-2026-07-25-01), corroborated by `docs/audits/AUDIT_SAFETY_2026-07-25.md` (SAFE-2026-07-25-01)
**Status**: Regression of closed #1904 ("document every renderer FFI unsafe block with a SAFETY comment")

## Description
#1904 (closed 2026-07-14) swept every `unsafe {}` block in `crates/renderer/src/vulkan/` with a `// SAFETY: …` comment and locked the invariant in permanently via a crate-root `#![deny(clippy::undocumented_unsafe_blocks)]` (`crates/renderer/src/lib.rs:21`) — specifically so a *future* undocumented block would fail the build, not just look untidy.

Commit `33d6a18e` ("Add presentation pass for output-resolution HDR and FSR integration", 2026-07-23) added a new module (`presentation.rs`) and substantially reworked two existing ones (`composite.rs`, `frame_upscaler.rs`) plus two call sites (`context/draw.rs`, `context/resize.rs`), introducing 30 raw `ash` FFI calls (`create_image`, `get_image_memory_requirements`, `bind_image_memory`, `create_image_view`, `destroy_framebuffer`, `destroy_pipeline`, `destroy_shader_module`, `destroy_pipeline_layout`, `destroy_render_pass`, `destroy_descriptor_pool`, `destroy_descriptor_set_layout`, `destroy_sampler`, etc.) with **none** of them carrying the required per-block `// SAFETY:` comment.

`Presentation::destroy` does carry a `# Safety` doc-comment on the *outer* function ("No in-flight command buffer may reference this pipeline"), but `clippy::undocumented_unsafe_blocks` requires a comment on each individual inner `unsafe {}` block — the outer doc-comment does not satisfy it.

This is the same underlying code-level problem the Safety audit's independent heuristic sweep flagged as SAFE-2026-07-25-01 (~19 of the 30 sites, in the same five files, from the same commit) — filed here as one issue rather than two, since both reports are measuring the identical gap by different methods (a manual "no SAFETY within 15 lines" heuristic vs. a literal `cargo clippy` run).

## Evidence
Reproduced directly against current `main` (2026-07-25):
```
$ cargo clippy -p byroredux-renderer --lib -- -D warnings
error: unsafe block missing a safety comment
   --> crates/renderer/src/vulkan/presentation.rs:448:13
    |
448 |             unsafe { device.destroy_sampler(self.sampler, None) };
    |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: consider adding a safety comment on the preceding line
...
error: could not compile `byroredux-renderer` (lib) due to 30 previous errors
```
`git log --oneline -1 -- crates/renderer/src/vulkan/presentation.rs` → `33d6a18e Add presentation pass for output-resolution HDR and FSR integration` (2026-07-23, postdates #1904's 2026-07-14 close by 9 days).

`cargo clippy --workspace -- -D warnings` — the exact command `.github/workflows/ci.yml`'s `cargo-test` job runs — reproduces the same 30 errors and halts before any downstream crate (including the `byroredux` binary itself) gets clippy-checked, since `byroredux-renderer` fails to compile under clippy first.

Full site list (30 total):
- `presentation.rs:417,420,424,428,432,436,440,444,448` (9 sites, `Presentation::destroy`)
- `composite.rs:381,394,400,407,1052,1056,1144,1151,1157,1162,1297,1301,1478,1482` (14 sites — includes the "composed scene" image/view creation sequence: `create_image` / `get_image_memory_requirements` / `bind_image_memory` / `create_image_view`, same shape as `exposure.rs`'s already-commented version)
- `frame_upscaler.rs:346,360,470,791,794` (5 sites)
- `context/draw.rs:893` (1 site — delegating call `unsafe { presentation.destroy(&self.device) }`)
- `context/resize.rs:827` (1 site — same delegating pattern)

SAFE-2026-07-25-01's corroborating (narrower, heuristic) subset covered the same five files: `presentation.rs:417-448` (its "six `device.destroy_*` calls" description), `composite.rs:381-407,1144-1162`, `frame_upscaler.rs:360,470`, `context/draw.rs:893`, `context/resize.rs:827` — confirming this is one bug, not two.

`crates/fsr3-sys/examples/vulkan_context_smoke.rs` (17 further sites) is a standalone `cargo run --example` smoke test, not part of the shipped engine binary or the `byroredux-renderer` crate the `#![deny(...)]` lint gates — out of scope for this issue but worth a defensive top-of-file comment per SAFE-2026-07-25-01's suggestion.

## Impact
`cargo clippy --workspace -- -D warnings` — the exact CI gate — currently fails on `main`. Every one of the 30 sites is a real Vulkan object-lifetime call (image/view creation, GPU allocator binding, pipeline/shader-module/render-pass/descriptor-pool/sampler teardown in the new FSR presentation pass) with its actual precondition ("device still valid," "not referenced by an in-flight command buffer," "handle was created by this device," "count/pointer arguments valid") now completely unstated and thus unreviewed — exactly the class of omission #1904 was written to make structurally impossible.

`cargo test --workspace` stays green (3858 passed / 0 failed) because `clippy::undocumented_unsafe_blocks` is a Clippy-only lint that plain `rustc`/`cargo build`/`cargo test` silently ignore — so the break is invisible to every gate except the one it was purpose-built to trip. No live memory-safety violation was found in a static read of any of the 30 sites (Safety audit's independent review of the same code found no invariant actually violated) — this is a hygiene/process regression, not a live bug, but it is a real CI-breaking regression of a closed issue.

## Suggested Fix
Add a `// SAFETY: …` comment immediately above each of the 30 flagged `unsafe {}` blocks, stating the concrete precondition each call relies on (mirroring the phrasing #1904 already established for the rest of the crate — e.g. "device is valid for the lifetime of this call; handle was created by this device; not referenced by any in-flight command buffer"). Port the existing convention from `exposure.rs`/`svgf.rs` onto `composite.rs`'s new composed-scene block and `presentation.rs::destroy()`'s per-call sites specifically, since those are the exact same call shapes already commented elsewhere in the crate. Then re-run `cargo clippy -p byroredux-renderer --lib -- -D warnings` locally before the next push — this file class (new Vulkan modules landing without clippy having been run locally first) is exactly what the crate-root `deny` is supposed to catch pre-merge, so treat a clean local `cargo clippy` as a hard prerequisite for any PR touching `crates/renderer/src/vulkan/`.

## Related
Regression of #1904 (the original fix) · commit `33d6a18e` (the regressing commit) · SAFE-2026-07-25-01 (corroborating independent finding, same bug, folded in here) · the four preceding FSR-integration commits (`e153b50c`, `5c7acfe2`, `443e55b0`, `227b331b`) did not touch these files and are not implicated.

## Completeness Checks
- [ ] **UNSAFE**: Each of the 30 blocks gets its own `// SAFETY:` comment stating the upheld invariant (not just a comment on the enclosing fn)
- [ ] **SIBLING**: Confirm no further undocumented blocks exist elsewhere in the FSR integration once these 30 are fixed — re-run `cargo clippy -p byroredux-renderer --lib -- -D warnings` and expect zero errors
- [ ] **DROP**: `Presentation::destroy()` / `FrameUpscaler` teardown ordering stays reverse-order correct once comments are added (no logic change intended, comments only)
- [ ] **FFI**: N/A — these are ash/Vulkan calls, not the `cxx-bridge` or `fsr3-sys` boundary (the `fsr3-sys` crate itself already fully documents its own `unsafe fn`s per the Safety audit's PASS list)
- [ ] **TESTS**: `cargo clippy -p byroredux-renderer --lib -- -D warnings` passes clean (this is itself the regression test — no new unit test needed, the lint is the guard)
