# #2734: NavigatorBackend::fetch is async in signature only; archive I/O runs inline on the main loop

- **Severity**: LOW
- **Dimension**: 7 — Worker Threads (pump semantics)
- **Location**: `crates/ui/src/navigator.rs:146-230`
- **Status**: NEW
- **Description**: `fetch` returns `OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse>`, but every
  code path performs its work **eagerly** and then wraps an already-computed value in
  `Box::pin(async move { Ok(response) })`. The synchronous work includes the full
  `provider.load(&archive_path)` archive extract (zlib/LZ4 decompress for BSA/BA2), plus for import
  assets a `swf::decompress_swf` + `swf::parse_swf` + tag-record rewrite in
  `prepare_import_asset_swf`. `fetch` is called by Ruffle from inside `player.preload()` /
  `player.tick()`, i.e. inside the engine's main loop, with no opportunity for the local executor to
  interleave.
- **Impact**: archive decompression and SWF reparse cost lands as a main-loop stall rather than
  amortised pump work; `MAX_ARCHIVE_PRELOAD_PASSES = 64` bounds the pass count but not the per-pass
  cost. Correctness is unaffected — and it is what makes the pump trivially single-threaded, which
  is why §3.2/§3.5 come out clean. Reachable only through the navigator path (test-only today, per
  CONC-D7-UI-04).
- **Suggested Fix**: no action while the path is dev-only. If archive menus ship, move the extract
  behind a real future serviced by the existing streaming worker's `Arc`-shared provider
  (`byroredux/src/streaming.rs`), whose `BsaArchive`/`Ba2Archive` already serialise `File` access via
  Mutex — note this would require replacing the navigator's `Rc<dyn ScaleformResourceProvider>` with
  an `Arc`-based one, which is a real design change, not a `s/Rc/Arc/`.

---

---
**Source**: `docs/audits/AUDIT_CONCURRENCY_UI_2026-08-12.md` (finding `CONC-D7-UI-06`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

