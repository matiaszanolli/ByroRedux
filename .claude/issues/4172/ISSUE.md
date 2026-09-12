# D7-2026-09-11-01: parse_mgef's remapped associated_item sentinel (0xFFFFFFFF) triggers a false-positive warning on every multi-master load

URL: https://github.com/matiaszanolli/ByroRedux/issues/4172
Labels: bug, medium, esm-plugin

---

**Severity**: MEDIUM
**Dimension**: ESM→ECS Handoff
**Record / Sub-record**: `MGEF` / `DATA` (`associated_item` @8)
**Location**: `crates/plugin/src/esm/records/misc/magic.rs:681` (remap call), `:649` (sentinel doc); `crates/plugin/src/esm/reader.rs:449-491` (`FormIdRemap::remap`, out-of-range branch)
**Status**: NEW — residual of the #4070/D7-02 fix, which correctly remapped the field but did not add the sentinel guard its own suggested fix explicitly asked for.

**Description**: `#4070` correctly remapped `associated_item` and `effect_shader_id`. But `associated_item`'s documented "no item" sentinel is `0xFFFF_FFFF`, and `remap_fid` only special-cases `raw == 0` before delegating to `FormIdRemap::remap`. For `raw = 0xFFFF_FFFF`, `mod_index = 255` — on any real multi-master load this is neither a self-reference, an in-range master, nor (with masters present) the empty-master-list arm, so it falls to the final `else` arm, written for "genuinely suspicious" malformed input, which unconditionally `log::warn!`s. The value round-trips correctly (no corruption), but every MGEF record with no associated item — common, well-documented, authored data — now logs a warning misclassified as suspicious.

**Evidence**:
```rust
// misc/magic.rs:678-681
out.associated_item = remap_fid(header.associated_item, remap);
...
out.light_form_id = remap_fid(header.light_form_id, remap);
```
```rust
// reader.rs:483-490
} else {
    // Multi-plugin load with an out-of-range index — genuinely
    // suspicious (malformed file or an in-memory injected form).
    log::warn!("FormID {raw:08x} has mod_index {mod_index} but plugin has {} masters", ...);
    return raw;
}
```
No test exercises the real `0xFFFF_FFFF` sentinel under a non-identity remap (the existing test uses `associated_item = 0`, which short-circuits earlier).

**Impact**: Log noise, not data corruption — but a normal multi-master DLC load produces one `warn!` per no-item MGEF record at parse time, drowning out the rare genuine malformed-FormID case the branch exists to catch.

**Related**: #4070 (ESM-2026-09-09-D7-02) — this is a residual of that fix, which named this exact gap in its own suggested fix.

**Suggested Fix**: Guard the sentinel before calling `remap_fid` (`if header.associated_item == 0xFFFF_FFFF { 0xFFFF_FFFF } else { remap_fid(...) }`), or add a documented second early-out to `remap_fid` itself. Add a positive test with the real sentinel under a non-identity `FormIdRemap`.

## Completeness Checks
- [ ] **TESTS**: A positive test with `associated_item = 0xFFFF_FFFF` under a non-identity multi-master `FormIdRemap` pins the corrected no-warning behavior

