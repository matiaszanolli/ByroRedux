# #7380 — REN-D5-2026-11-23-03: a floor is stamped with the archive generation at queue time, but the exterior streaming provider that read its bytes is never rebuilt, so the stamp can name a newer file than the bytes came from

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D5-03). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (visual-only; no reachable trigger in a normal run: needs an archive replaced on disk mid-session plus a provider build elsewhere in the same exterior session)
- **Dimension**: Memory/Lifecycle (streamed-texture residency, M39)
- **Location**: `TextureRegistry::source_generation` / `PendingDdsUpload.generation` / `StreamSource.generation` / `set_promotable_generation` (`texture_registry/{mod,upload}.rs`); `push_texture_streaming` (`byroredux/src/app_frame.rs`); `WorldStreamingState::new` (`streaming/mod.rs`: `tex_provider: Arc::new(tex_provider)`, documented there as "never rebuilt in place").
- **Status**: NEW (residual of #7169, closed; #7169's suggested fix stamped "`archive_replacements()` read when the provider that reads it was built", the shipped fix reads the counter when the upload is queued)
- **Description**: the #7167/#7169 invariant is "a floor promotes only while its stamp equals the generation the streamer's provider reads from". The stamp is the counter value when the floor is queued (`enqueue` -> `self.source_generation()`), and the test models "a cell load's provider build notices a replaced archive, then floors that cell's texture". That holds for interior/boot paths (`app_step.rs` builds a provider per load). It does not hold for exterior streaming: floors are read through `state.tex_provider`, built once for the `WorldStreamingState` and not refreshed. If the counter moves while that state lives (any other `build_texture_provider` call that re-opens a replaced archive: the player-body boot spawn, a loading-cover build, a debug load), floors queued afterwards read their bytes through the old file table yet carry the new stamp, `refresh_provider` rebuilds the streamer's own provider onto the new file, and `apply_wanted` then promotes those floors from the new file: the mixed-mip case #7167 closed, back for exterior-streamed textures.
- **Evidence**: `rg "tex_provider" byroredux/src/streaming/mod.rs` (constructor only, no refresh; comment "an `Arc` cloned into the worker, never rebuilt in place"); `StreamSource::generation` is written only from `upload.generation` in `record_floor_sources`; `a_floor_promotes_only_under_the_generation_it_was_read_from` stamps via `reg.source_generation()` at queue time and never models a provider older than the counter. Not run end to end.
- **Impact**: after a mid-session archive swap, some textures may promote to mips of a different file than their floor (visual blur/seam), until released. Unreachable without an archive rewritten under a running engine.
- **Related**: #7167, #7169, #7144 (closed).
- **Suggested Fix**: stamp with the generation of the provider that read the bytes (`WorldStreamingState` records `archive_replacements()` when its `tex_provider` is built and passes it into the enqueue), or rebuild the streaming state's provider on a counter move the way the streamer's is. Add a test that queues a floor after the counter moves with a provider older than the counter and expects it not to promote.
- **Trigger Conditions**: archive replaced on disk during an exterior streaming session; any second provider build before the session ends; a texture floored afterwards through the streaming provider.
- **Verification Path**: unit (registry) in the style of `the_provider_is_rebuilt_only_after_an_archive_replacement`; no GPU needed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
