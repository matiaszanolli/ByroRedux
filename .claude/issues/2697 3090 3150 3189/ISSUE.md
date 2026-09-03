# #2697 — NIFAL-D8-2026-08-12-05: `supplemental_texture_indices` is a third hand-written role walk with no lockstep test

**Severity**: LOW · **Location**: `byroredux/src/render/static_meshes.rs:561-574` vs `crates/renderer/src/vulkan/material.rs:415-430` and `crates/renderer/src/vulkan/context/mod.rs:492-504`
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (NIFAL-D8-05)

Beyond the two role walks the spec names (`map_ref`, compiler-protected; `values()`, not), there is a
third: a positional `[u32; 12]` built in `byroredux` and indexed back out through
`supplemental_texture_slot::*` constants in `byroredux_renderer`. Nothing couples the two orders.
Verified correct today (tint, inner_layer, specular, lighting, flow, wrinkle, reflectance,
emittance_gradient, decals 0-3), and the GPU side is protected by
`material_hash_matches_gpu_material_field_hash` plus the `offset_of!` pins — but the CPU-side ordering
has no test at all.

**Impact**: Inserting a constant mid-list silently shifts every following role by one — tint sampled as
specular, etc. — with no compile error and no failing test.

**Suggested Fix**: Index the constants when building the array (`arr[slot::TINT] = …`), or add an
explicit ordering test.

---

# #3090 — CONC-2026-08-16-02: a cancelled screenshot makes DebugDrainSystem skip that frame's entire command drain

**Severity**: LOW · **Location**: `crates/debug-server/src/system.rs:72-78`
**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-16.md` (Worker Threads)

The #1007 abandonment handler cancels the in-flight GPU capture, clears `pending_screenshot`, and then
`return`s from `System::run`. That `return` exits the whole system, not just the screenshot block, so
the command drain at :136-142 never runs on that frame. The three sibling arms in the same block
(:110, :124, :131) all fall through rather than returning — this arm is the odd one out.

**Trigger**: a `byro-dbg` screenshot request whose client-side 5s `recv_timeout` fires before the
engine's 10-frame ceiling (paused/GPU-stalled engine), with at least one other command already queued.

**Impact**: bounded, self-correcting (drain runs next frame), but happens exactly when a developer has
issued several commands and can't tell deferred from ignored.

**Suggested Fix**: Replace the `return` with the fall-through the sibling arms use.

---

# #3150 — ESM-2026-08-20-D4-01: three `//! TEMP scratch` audit probes committed as `crates/plugin` example targets — and 57 more sit in `crates/nif/examples/`

**Severity**: LOW · **Location**: `crates/plugin/examples/_tmp_obl_bsxrefr.rs`, `_tmp_obl_player.rs`,
`_tmp_sk_lvli.rs` (added by `19e53dd8`); plus 57 siblings in `crates/nif/examples/`
**Source**: `docs/audits/AUDIT_ESM_2026-08-20.md` (Dim 4)

Three example binaries in `crates/plugin/examples/` open with `//! TEMP scratch (audit 2026-08-16): ...`
doc comments and are committed workspace build targets (`cargo build --examples` / `cargo test` compile
them) — the exact artefact a prior ESM audit close-out claimed was removed. A larger population
(57 files) of the same pattern exists in `crates/nif/examples/`.

**Impact**: no runtime effect; build time/CI surface, audit noise (tree-wide greps for production
invariants hit these), convention drift.

**Suggested Fix**: Delete the three `crates/plugin/examples/_tmp_*.rs` files. Triage the 57 in
`crates/nif/examples/`: keep-worthy probes lose the `_tmp_` prefix and get a real doc comment (as
`watr_wind_census.rs`, `esm_dim8_bench.rs`, `sf_smoke.rs` already do); the rest are deleted.

---

# #3189 — AUD-2026-08-20-D7-02: try_load_default_water_splash duplicates the --sounds-bsa scan and re-opens the same archive a second time at boot; both loaders bypass SoundCache

**Severity**: LOW · **Location**: `byroredux/src/asset_provider/texture.rs`
(`try_load_default_footstep`, `try_load_default_water_splash`), `byroredux/src/boot.rs`
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-20.md` (AUD-2026-08-20-D7-02)

`948f104a` added a second boot-time sound loader that is a structural copy of the first. Both are
invoked back-to-back with the same `args`, both scan for `--sounds-bsa` with two different idioms
(hand-rolled `while i < args.len()` vs `args.windows(2).find(..)`), and both call `Archive::open(path)`
independently on the same file — the BSA header + full folder/file record tables are parsed twice per
boot for one archive. Neither loader routes through `SoundCache`.

**Impact**: boot-time only, no runtime cost, no correctness issue (engine boots correctly with the
archive absent). Maintenance cost: two divergent arg parsers for one flag.

**Suggested Fix**: Fold both into one `try_load_default_sounds(world, args)` that resolves
`--sounds-bsa` once, opens the archive once, and populates both `FootstepConfig.default_sound` and
`WaterAudioConfig.splash_sound` from that single handle.
