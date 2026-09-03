# #3558 — RT-12: FO4 counts one asset twice under two path spellings — two cache keys for one texture

**Severity**: LOW · **Dimension**: Runtime (renderer)
**Location**: `byroredux/src/commands/assets.rs`, `crates/renderer/src/texture_registry.rs`

## Investigation — the issue's premise was stale

Traced the actual cache-key derivation before implementing the issue's
suggested fix ("normalize separator and the `textures\` prefix before
the cache key is taken, not just before the archive lookup"). Found
that half of the claim was already false against current code:

`TextureRegistry`'s `path_map` cache key is **already** fully
self-normalizing. Every accessor (`load_dds_with_clamp_and_color_space`,
`queue_or_hit_for_view`, `get_by_path_with_clamp`,
`acquire_by_path_for_view`) routes through `texture_keyed_path`/
`texture_keyed_path_with_color_space`, which unconditionally call
`normalize_path` (lowercase + backslash-to-forward-slash fold +
guaranteed `textures/` prefix) — this is issue **#522**'s own fix,
landed 2026-04-21, well before this audit ran (2026-08-30), and already
covered by an existing regression test
(`normalize_prefix_variants_collapse_to_one_key`,
`crates/renderer/src/texture_registry_tests.rs`) that asserts the exact
scenario the issue describes (`landscape\dirt02.dds` /
`textures\landscape\dirt02.dds` / `Textures/LANDSCAPE/dirt02.DDS` /
`landscape/dirt02.dds` all collapse to one key). The archive-side
lookup (`byroredux/src/asset_provider/archive.rs::canonical_texture_key`)
is a *second*, independent normalization applied at the call site
specifically so the two agree — also already landed (#3334).

So the issue's more severe claimed impact — "a second archive lookup and
potentially a second resident copy in VRAM" — does not reproduce. The
real cache and the real archive lookup were already correct.

## What actually reproduces: the `tex.missing`/`tex.loaded` diagnostic
commands themselves

Traced why `tex.missing`'s live dump still showed two spellings despite
the cache being correct: `TexMissingCommand`/`TexLoadedCommand` bucket
by the **raw** `Material::texture_path` string (as authored, verbatim)
for their own reporting aggregation — a separate, un-normalized
`HashMap<String, ...>` local to the diagnostic command, never touching
`TextureRegistry::path_map` at all. Two entities whose `Material`
legitimately carries different spellings of the same underlying texture
(inconsistent authoring across meshes, not a cache bug) therefore report
as two distinct "missing"/"loaded" buckets — inflating the reported
count, exactly the issue's own *first*, lower-severity impact claim
("Inflates `tex_missing_unique_paths` — a purely cosmetic metric
problem"), which turned out to be the half that was actually real.

## Fix

- `crates/renderer/src/texture_registry.rs::normalize_path` — widened
  from module-private to `pub` so the diagnostic commands (a different
  crate) can reuse the exact same canonicalization the real cache uses,
  rather than risking a second, independently-maintained normalizer
  drifting out of sync with it later.
- `byroredux/src/commands/assets.rs::TexMissingCommand` (both bucketing
  loops — the base-color `TextureHandle` walk and the 26-role
  `MaterialTextureHandles` walk) and `TexLoadedCommand` (the sibling
  aggregator, same un-normalized-key shape, found and fixed alongside
  it) now bucket by `normalize_path(path)` instead of the raw authored
  string.

## SIBLING (issue's own checklist item — "every path-keyed cache in the
asset provider checked, not just the texture one")

- **Material** (`byroredux/src/asset_provider/material.rs`) — already
  safe: `resolve_bgsm`/`resolve_bgem` normalize+lowercase before
  touching `bgsm_cache`/`bgem_cache`, same pattern as the texture
  registry.
- **Mesh** (`crates/renderer/src/mesh.rs::MeshRegistry::mesh_cache`) —
  genuinely NOT self-normalizing (`acquire_cached`/
  `register_scene_mesh_keyed` key directly off the raw `model_path`
  string, trusting the caller). No live bug — every current caller
  (`references/synth_child.rs` via `canonical_model_path_key`,
  `precombined.rs` via an engine-generated deterministic path) already
  pre-normalizes — but the registry itself has nothing to catch a future
  caller that doesn't. `mesh.rs` is a per-REFR hot path, so hardening it
  needs its own perf-aware investigation rather than a blind copy of
  this fix. Filed separately: **#3818**.
- **`tex.missing`/`tex.loaded` diagnostic aggregation** — the actual
  live bug (see above), fixed here.

## TESTS (issue's own checklist item — "a regression test pins that
`textures\a\b.dds` and `a/b.dds` produce one cache entry")

The real cache already had this pin (`normalize_prefix_variants_collapse_to_one_key`,
pre-existing, #522). Added the missing half — the diagnostic commands'
own aggregation:

- `tex_missing_tests::spelling_variants_of_the_same_texture_collapse_to_one_bucket`
  — the issue's own exact evidence
  (`textures\setdressing\wallconsoles\wallconsole01_sm_d_n.dds` vs.
  `setdressing/wallconsoles/wallconsole01_sm_d_n.dds`) must report as
  ONE bucket with count 2, not two buckets of 1.
- `tex_loaded_tests::spelling_variants_of_the_same_texture_collapse_to_one_bucket`
  — same proof for the sibling `TexLoadedCommand`, newly given its own
  test module (none existed before).

**Reintroduce-and-revert verification** (both sites independently):
temporarily reverted each bucketing site back to the raw (un-normalized)
path string — confirmed each corresponding test failed showing the
exact two-spellings-two-buckets output the issue describes. Restored the
fix after each and reran — all 5 tests in `commands::assets::` pass
again.

## Verification

- `cargo check -p byroredux-renderer -p byroredux --tests`: clean, zero
  warnings.
- `cargo test -q -p byroredux --bin byroredux commands::assets::`: 5
  passing, 0 failing (+2 new).
- `cargo test -q -p byroredux-renderer`: 827 passing, 0 failing
  (visibility-only change to `normalize_path`, unaffected otherwise).
- `cargo test -q -p byroredux --bin byroredux`: 1891 passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7173 passing, 0
  failing**.
