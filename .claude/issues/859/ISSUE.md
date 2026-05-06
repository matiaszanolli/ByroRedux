## Source Audit
`docs/audits/AUDIT_SAFETY_2026-05-05.md`

## Severity / Dimension
LOW / Memory Safety (dormant API) / Documentation accuracy

## Location
- `crates/audio/src/lib.rs:951-1041` — `SoundCache` definition with documented "Eviction strategy: **none today**" policy.
- Zero hits for `SoundCache` outside `crates/audio/src/lib.rs` (`grep -rn 'SoundCache' /mnt/data/src/gamebyro-redux/byroredux/ /mnt/data/src/gamebyro-redux/crates/ | grep -v 'crates/audio/src/lib.rs'`).

## Description
The audit-audio dispatch flagged `SoundCache` unbounded growth as a watchpoint. Investigation: `SoundCache` is dead code in the current tree. Nothing in the engine binary calls `SoundCache::new()`, `insert()`, `get()`, or `get_or_load()`. The current footstep dispatch path at `byroredux/src/asset_provider.rs:251-252` writes directly into `FootstepConfig.default_sound: Option<Arc<Sound>>` — bypassing the cache entirely. The decoded Arc is held by exactly one `Resource` (FootstepConfig) for the engine lifetime; multi-sound SFX paths haven't landed yet (FOOT records, REGN ambient) so there's no second consumer to drive cache lookups.

The "no eviction → unbounded growth" concern from the prompt does not produce a live leak today because the cache never grows past `len() == 0`. The risk surfaces the moment a future commit wires a real consumer — the dormant API ships with explicit "no eviction" semantics and no telemetry beyond `len()`.

## Evidence
Zero non-test references; the `Default` impl, `is_empty`, `len`, `insert`, `get`, and `get_or_load` are exercised only by `#[cfg(test)] mod tests` (lines 1124-1185).

## Impact
No live impact today. Future wiring (planned Phase 3.5b: FOOT records → per-material sound lookup) will produce cache growth proportional to the unique-sound count. Vanilla FNV `Fallout - Sound.bsa` is 6,465 entries (~620 MB on disk); 100% load with the typical 5–10× decompression ratio gives 3–6 GB of decoded PCM in worst case. That's a real memory footprint, but the docstring's "few hundred MB" estimate is conservative for FNV (sit-on-it for now), and a workable answer for the future is an LRU bolted on at the cache layer.

## Suggested Fix
Leave the API as-is until a real consumer lands. When wiring FOOT-driven dispatch, add (a) a soft cap (e.g. 256 distinct sounds, ~256 MB ceiling for short SFX) and (b) LRU eviction. For now, the only deliverable from this audit is **upgrading the docstring** to flag dormancy and pin the future cap discussion:
```rust
/// **Status (2026-05-05)**: defined but unused — no consumer in the
/// engine binary today. The "no eviction" docstring is aspirational.
/// When the first real consumer lands (FOOT records, REGN ambient),
/// add an LRU cap before exterior streaming wires up.
```

## Related
- `feedback_no_guessing.md` — the "few hundred MB" estimate was speculative when written; verify actual decoded size when FOOT lands.
- The `pending_oneshots: Vec` cap at 256 entries (`lib.rs:327`) is the right precedent for what `SoundCache` should look like once it has a consumer.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
