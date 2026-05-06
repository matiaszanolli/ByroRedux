## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
MEDIUM / Reverb Send & Routing

## Location
cross-cut: `byroredux/src/cell_loader.rs`, `byroredux/src/streaming.rs`, `crates/audio/src/lib.rs:427-429` (the setter exists but no caller toggles it)

## Description
The crate docstring (lib.rs:99-105) and Phase 6 promise an "interior detector that runs after `cell_loader` finishes" to flip `set_reverb_send_db(-12.0)` for interiors and back to `f32::NEG_INFINITY` for exteriors. `set_reverb_send_db` exists, but no call site invokes it: `grep set_reverb_send_db /mnt/data/src/gamebyro-redux/{byroredux,crates}/**/*.rs` returns only the definition + tests.

## Evidence
```
$ grep -rn "set_reverb_send_db" /mnt/data/src/gamebyro-redux/byroredux/ /mnt/data/src/gamebyro-redux/crates/audio/
crates/audio/src/lib.rs:427:    pub fn set_reverb_send_db(&mut self, db: f32) {
crates/audio/src/lib.rs:1305:        world.set_reverb_send_db(-12.0);
crates/audio/src/lib.rs:1307:        world.set_reverb_send_db(f32::NEG_INFINITY);
# only the setter definition + 2 unit-test call sites
```

## Impact
Every cell sounds dry (no audible reverb) regardless of interior/exterior. The Phase 6 "Better-than-Bethesda axis" claim about reverb zones is unrealised in M44. Functional but lacks the promised interior bloom.

## Suggested Fix
Add a system in `Stage::Late` (or an exclusive cell-load callback in `byroredux/src/streaming.rs`) that observes the active cell's interior/exterior bit and calls `audio_world.set_reverb_send_db(-12.0)` or `NEG_INFINITY`. The CELL record's flag bit 0 distinguishes interior vs exterior; that's already plumbed through `cell_loader.rs`. Hook there.

## Related
Phase 6 future (cell-acoustic-driven reverb zones). AUD-D5-NEW-06 (the bigger gap: per-cell acoustic data).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
