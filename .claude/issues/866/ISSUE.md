## FNV-D6-NEW-07: AnimationClipRegistry::get_or_insert_by_path doesn't lowercase the key — foot-gun for M42 IDLE callers

## Source Audit
`docs/audits/AUDIT_FNV_2026-05-05.md` — Dim 6 INFO Dim6-02

## Severity / Dimension
LOW (forward-looking foot-gun) / Animation registry API

## Location
`crates/core/src/animation/registry.rs:66-76`

## Description
`get_or_insert_by_path` does NOT lowercase the key; it relies on the caller to pass a canonical key.

```rust
pub fn get_or_insert_by_path<F>(&mut self, key: String, build_clip: F) -> u32
where
    F: FnOnce() -> AnimationClip,
{
    if let Some(&handle) = self.clip_handles_by_path.get(&key) {
        return handle;
    }
    let handle = self.add(build_clip());
    self.clip_handles_by_path.insert(key, handle);
    handle
}
```

The single live caller today (`byroredux/src/npc_spawn.rs:196`) passes a `&'static str` literal `r"meshes\characters\_male\locomotion\mtidle.kf"` which is already lowercase, so the contract holds. The docstring at `registry.rs:24-26` calls out "Keys are caller-normalised (typically a lowercased archive path)" but the API gives the caller no enforcement.

## Impact
**Today: none** — single caller passes a static lowercase literal.

**M42 (IDLE-record callers, AI-package idles)**: a future caller handing in a user-sourced KF path (e.g. an IDLE record's `Model` field, or a Papyrus `Debug.SendAnimationEvent` re-routed through this path) without an explicit `.to_ascii_lowercase()` would silently break dedup. Two payloads with the same path but different case would each register a separate clip handle, defeating the #790 dedup invariant and resurrecting the leak it was meant to prevent.

## Suggested Fix
Two options:

1. **Lowercase inside the registry** — simplest:
   ```rust
   pub fn get_or_insert_by_path<F>(&mut self, key: String, build_clip: F) -> u32
   where F: FnOnce() -> AnimationClip,
   {
       let key = key.to_ascii_lowercase();
       // ... existing body
   }
   ```
   Cost: one allocation per call (negligible vs the parse cost on miss; on hit it's wasted).

2. **Type the key as a `LowercasePath` newtype** before this gets footgunned in M42:
   ```rust
   pub struct LowercasePath(String);
   impl LowercasePath {
       pub fn new<S: AsRef<str>>(s: S) -> Self { Self(s.as_ref().to_ascii_lowercase()) }
   }
   pub fn get_or_insert_by_path<F>(&mut self, key: LowercasePath, ...) -> u32 { ... }
   ```
   Stronger compile-time guarantee, costs one type at the call sites.

Either lands before any M42 IDLE-record callers attach.

## Related
- #790 (clip-dedup commit `da99d15`) — the contract this finding is hardening
- `FNV-D3-NEW-04` (`AnimationClipRegistry` grow-only leak) — this finding is the same registry's adjacent foot-gun

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check every other registry that takes a path-keyed lookup — `NifImportRegistry::get` already handles case via `to_ascii_lowercase()` at the call site (`cell_loader.rs:1146-1224`)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Unit test inserting the same key in different case — assert the second call returns the first call's handle, and `clips.len() == 1`
