# #4796: PERF-D7-2026-09-23b-02: `read_pod_vec_from` now zero-fills a scratch buffer and copies every bulk NIF array twice, the opposite of its "#3062 win kept" claim (regression of #3062)

**Severity**: LOW
**Labels**: low, performance, nif-parser, nif, test-gap, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D7-2026-09-23b-02)

- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/stream.rs:814-861`; stale comment `:805-813`
- **Status**: Regression of #3062 (introduced by `3dc127d0b`, the #4594 soundness fix)
- **Description**:
  - Every bulk array (points, vec2/3/4, u16/u32/i16/f32, triangles, colours, BSGeometry weights and meshlets, header block tables) now goes through four steps: `vec![0u8; byte_count]`, a `read` into it, a `copy_nonoverlapping` into `out`, then a free.
  - Compared with #3062's single copy, that is one more allocation, one more zero-fill and one more memcpy, and transient peak bytes double.
  - The comment says the fix copies "from that slice directly". It doesn't.
- **Evidence**: orchestrator-verified at `stream.rs:824-860`.
- **Impact**: *est.* tens of µs per 100 KB of geometry on the stream worker, and on the main thread for the NPC hook-path parses (PERF-D7-2026-09-23b-01). The dhat bounds assert peak bytes on tiny fixtures and are blind to it.
- **Related**: #3062, #4594
- **Suggested Fix**:
  - Both instantiations are `Cursor<&[u8]>`, so take that type directly: bounds-check `get_ref()[pos..pos+byte_count]`, `copy_nonoverlapping` into spare capacity, `set_len`, `set_position`. This is sound with one copy. `bytemuck::pod_collect_to_vec` is an alternative.
  - Add an allocation-count bound.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
