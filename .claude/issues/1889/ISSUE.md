# LC0705-01: VWD full-model-cull consumer untracked

**Issue**: #1889
**Filed from**: docs/audits/AUDIT_LEGACY_COMPAT_2026-07-05.md (LC0705-01, carryover of LC0703-01)
**Severity**: LOW · **Labels**: low, legacy-compat, enhancement
**Dimension**: EXAL — LOD distance rendering

## Description
#1731 closed the VWD / "Has Distant LOD" record-header flag at parse scope only.
The flag (`0x00010000`, `RecordHeader::is_visible_when_distant`,
`crates/plugin/src/esm/reader.rs`) is parsed and test-pinned, but the runtime
consumer — culling the full model once its `.bto` / `_far.nif` LOD proxy is active
— was deferred with no follow-up issue. LC0703-01 recommended a tracking issue;
it was never filed. #1889 fills that gap.

## Deferred consumers
- `byroredux/src/cell_loader/object_lod.rs` (.bto quads)
- `byroredux/src/cell_loader/placement_lod.rs` (_far.nif)
Both conservatively load distant geometry only outside the full-detail ring rather
than reading the flag.

## Suggested Fix
Wire `is_visible_when_distant()` into the LOD spawn path: suppress the full REFR
beyond the full-detail radius when its quad's LOD proxy is active. No urgent code
change — parse is correct, conservative ring rule is a valid interim.

## Related
#1731 (CLOSED, parse) · #1849 (OPEN, NAM3/NAM4 sibling) · EXAL §5.2/§5.4
