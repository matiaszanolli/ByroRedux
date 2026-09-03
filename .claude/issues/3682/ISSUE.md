# #3682 — PERF-D2-2026-08-30-03: the per-instance `GpuInstance` loop probes two `std::collections::HashMap`s per draw per frame, and the #3061 guard structurally cannot see them

**Severity**: LOW · **Dimension**: Draw & Instancing
**Location**: `crates/renderer/src/texture_registry.rs`

## Fix

`TextureRegistry::handle_has_alpha` / `handle_avg_rgb` resolved through
`std::collections::HashMap<TextureHandle, _>` (SipHash-1-3), read once (or
twice — unconditionally for `avg_rgb`, gated on alpha-blend for
`has_alpha`) per `DrawCommand` in the `GpuInstance` build loop — up to
~3,949 probes/frame on `fo4-InstituteBioScience`, the highest-volume site
in the #1368 → #2174 → #2923 → #3045 → #3061 hot-path-hashing cluster.

Took the issue's own first-choice suggested fix rather than its "minimum"
fallback (`FxHashMap`): `TextureHandle` is a dense index into
`textures: Vec<TextureEntry>` (`let handle = self.textures.len() as
TextureHandle` at every push site), so there is nothing to hash at all.
Moved `has_alpha: bool` and `avg_rgb: Option<[f32; 3]>` onto `TextureEntry`
itself and made both accessors a direct `Vec` index:

```rust
pub fn handle_has_alpha(&self, handle: TextureHandle) -> bool {
    self.textures
        .get(handle as usize)
        .is_some_and(|entry| entry.has_alpha)
}

pub fn handle_avg_rgb(&self, handle: TextureHandle) -> Option<[f32; 3]> {
    self.textures.get(handle as usize).and_then(|e| e.avg_rgb)
}
```

Updated all 5 `TextureEntry` push sites (`set_fallback`,
`set_neutral_fallback`, `load_dds_with_clamp`, the deferred-upload
`queue_or_hit_for_view` reservation, `create_dynamic_rgba_texture`) to
default `has_alpha: false, avg_rgb: None`, and the two places that
previously wrote through the HashMaps (`load_dds_with_clamp`'s
synchronous path, `flush_pending_uploads`'s deferred-upload closure) to
write `self.textures[handle as usize].has_alpha = ...` /
`.avg_rgb = Some(...)` directly instead. Removed both `HashMap` fields
from `TextureRegistry` entirely.

## SIBLING (issue's own checklist item)

The 3 load-time-only callers of `handle_has_alpha` named in the issue's
own evidence (`byroredux/src/cell_loader/terrain.rs`,
`byroredux/src/cell_loader/spawn/mesh_instance.rs`,
`byroredux/src/scene/nif_loader.rs`) needed no changes — they call through
the same public accessor, whose signature is unchanged.

`path_map: HashMap<String, TextureHandle>` on the same struct correctly
stays `std::collections::HashMap` — the issue's own evidence section notes
it's the one DoS-facing map here (keyed by attacker-influenced path
strings from mod content), unlike the two per-`TextureHandle` maps this
fix removed.

Per the issue's own suggested fix's second half, extended the #3061
source-scan guard (`crates/renderer/src/vulkan/context/mod.rs::
rigid_history_hasher_tests`) with
`texture_alpha_and_avg_rgb_are_not_hashed_by_texture_handle`, scanning
`texture_registry.rs` — the file every prior sweep in this lineage (#1368
through #3061) stayed inside `context/` and never covered.

## TESTS (issue's own checklist item)

Neither accessor had a direct regression test before this fix. Added two,
pinning both halves of the old `HashMap`'s "absent key → default" contract
that the new `Vec`-index version must preserve without indexing out of
bounds:
- `handle_has_alpha_reads_the_seeded_flag_and_defaults_false_out_of_range`
- `handle_avg_rgb_reads_the_seeded_value_and_is_none_out_of_range`

Extended the #3061 guard with
`texture_alpha_and_avg_rgb_are_not_hashed_by_texture_handle`, asserting
`texture_registry.rs` declares neither field as a per-`TextureHandle`
`HashMap`/`FxHashMap` and that `TextureEntry` carries both fields directly.

**Reintroduce-and-revert verification**, three separate probes:
1. Reverted `handle_has_alpha` to an unconditional `false` — confirmed the
   new accessor test failed on the seeded-value assertion.
2. Reverted `handle_avg_rgb` to an unconditional `None` — confirmed its
   accessor test failed the same way.
3. Reintroduced a commented-out
   `texture_has_alpha: HashMap<TextureHandle, bool>` field-declaration
   line into `TextureRegistry` (simulating the cluster re-growing) —
   confirmed the new guard test failed with the expected message.

Restored the fix after each probe and reran — all 44 `texture_registry`
tests and all 3 `rigid_history_hasher_tests` pass again.

## Verification

- `cargo check -p byroredux-renderer --tests`: clean, zero warnings.
- `cargo test -p byroredux-renderer --lib texture_registry`: 44 tests
  passing, 0 failing (+2 new).
- `cargo test -p byroredux-renderer --lib context::rigid_history_hasher_tests`:
  3 tests passing, 0 failing (+1 new).
- `cargo test -q -p byroredux-renderer`: 826 tests passing (+3), 0
  failing.
- `cargo check -p byroredux --tests`: clean (downstream crate).
- `cargo test -q --no-fail-fast` (full workspace): **7111 passing, 0
  failing**.
