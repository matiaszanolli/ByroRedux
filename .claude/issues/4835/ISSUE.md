# #4835 — REN-D5-2026-09-24-02: the 16/24-bpp DDS expand arm makes the payload-length check vacuous — a 128-byte header drives up to ~358 MB (2D) or ~2.1 GB (legacy cubemap) of host expansion and defeats the #4197 staging bound

**Labels**: bug,renderer,medium,memory,safety
**Filed from**: docs/audits/AUDIT_RENDERER_2026-09-24.md (audited `main` @ `6c5555c70`)

- **Severity**: MEDIUM. The #4511 per-dimension 8192 cap bounds it, so the "allocation with no cap" HIGH row is not met literally. If the merger counts the cubemap variant (~2.1 GB declared, in each of three places) as an OOM bomb, HIGH is arguable.
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/dds.rs` — `expand_uncompressed_rgb` (`data.get(..).copied().unwrap_or(0)`, `Vec::with_capacity(total_pixels * 4)`); `crates/renderer/src/vulkan/texture.rs` — `record_dds_upload` (`pixel_data.len() >= total_size`); `crates/renderer/src/texture_registry/upload.rs` — `flush_pending_uploads` (`sizes` built from `dds_bytes.len()`).
- **Status**: NEW. Sibling of closed #4511, whose stated failure was "a 128-byte header-only file passes … allocates a 4 GiB image sized from on-disk fields".
- **Description**: For BC/DXGI/32-bpp payloads `record_dds_upload` rejects a truncated file because `pixel_data` is the raw file tail. For the expand arm `upload_pixels` synthesises the buffer, missing source bytes read as 0, and the expanded length is exactly `total_data_size(meta)`, so the truncation check passes by construction. Consequences: (a) a header-only file is accepted and yields an all-black image of the declared size instead of the checkerboard fallback; (b) the `Vec`, the `CpuToGpu` staging buffer and the `GpuOnly` image are all sized from header fields; (c) `flush_pending_uploads` weights each queued upload by `dds_bytes.len()` (128 B here), so #4197's "one submit never stages more than `MAX_UPLOAD_BATCH_BYTES`" does not hold for expanded uploads.
- **Evidence** (scratch crate): `parse_dds` accepts an 8192×8192, 14-mip, 24-bpp `DDPF_RGB` header of exactly 128 bytes; `total_data_size` = 357,913,940 B; `upload_pixels` returns that `Cow::Owned` in 6.3 s (dev) / 0.52 s (release). With `caps2` set to a legacy six-face cubemap, `array_layers = 6` and `total_data_size` = 2,147,483,640 B (arithmetic only; not allocated).
- **Impact**: One hostile mod texture stalls the loader for seconds and takes ~1.1 GB (2D) or ~6.4 GB (cube: Vec + staging + image) of host/VRAM, and a truncated legitimate 16/24-bpp DDS silently becomes a black texture. Vanilla content is unaffected (font atlases are tiny).
- **Related**: #4511 (closed), #4197 (closed), REN-D5-2026-09-24-01.
- **Suggested Fix**: In `parse_dds`'s expand arm compute the required source bytes with checked math and `ensure!(data.len() >= data_offset + needed)`; weight `upload_batch_ranges` by `total_data_size(meta)` rather than `dds_bytes.len()` (or cap expanded size per texture).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
