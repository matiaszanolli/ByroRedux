# #5898: PAR-D5-2026-10-15-02: Object-graph loading discards the whole graph, and caches the failure, when any one clip is missing or does not decode

- **Labels**: medium,bug,animation
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: Object-graph loading discards the whole graph, and caches the failure, when any one clip is missing or does not decode. Actor graphs drop a skeleton decode error with no log at all and then report every clip as "not found".

- **Severity**: MEDIUM
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `byroredux/src/asset_provider/object_graph.rs:99-103`: the `?` on `fetch(...).and_then(decode)` returns `None` from `load_graph`. `:178` caches `None` per project, so the prop never animates.
  - `:8-10`: the module doc still says "Skyrim Special Edition's 64-bit Havok 2010 packfiles only". v11 has decoded since 60082cc55.
  - `byroredux/src/asset_provider/actor_graph.rs:121`: `decode_skeleton(&bytes).ok()` with no log. `:271` (`job.skeleton.as_ref()?`) then yields `None` for every clip, and `:299` logs it as "not found".
  - Behavior and clip decode errors throughout are `log::debug!` only (`actor_graph.rs:100`, `:273`; `object_graph.rs:58-67`, `:101`; `archetype_idle.rs:148/153/174`).
- **Status**: NEW. The `resolve_wanted` restart bug at `actor_graph.rs:307-318` is #5861 and is not re-filed.
- **Description**:
  - Contract: a bad asset costs that asset, and a partial result is signalled.
  - In the object path, one missing or unsupported clip costs every state of the graph, including clips that decoded. Given D5-01, that includes any FO4 placed object whose graph names one `hkaLosslessCompressedAnimation` clip.
  - In the actor path, a skeleton that fails to decode, such as a mod rig above `MAX_SKELETON_BONES` or one with a non-finite pose, leaves no trace at the default log level. The symptom presents as missing files.
- **Suggested Fix**:
  - Register the clips that decode and skip the rest with a `warn!` naming project, state and clip.
  - Warn on skeleton decode failure (as `archetype_idle.rs:148` does at debug, but at warn level).
  - Raise the decode-failure logs to `warn!` once per path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
