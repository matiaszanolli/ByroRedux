# Dependency-update follow-ups — action plan, 2026-10-10

**As of**: HEAD `51feb00a4`. The 2026-10 crate update landed in `317d4b7a9`..`52de1bc0a`
(rapier 0.22 → 0.36, glam 0.29 → 0.33, egui 0.33 → 0.36, kira 0.10 → 0.12,
wasmtime 47 → 49, Ruffle → nightly-2026-10-10 with wgpu 27 → 30, plus smaller bumps).
See [dependencies.md](dependencies.md) for the current pins.

**Scope**: what the new versions let us fix, simplify or build, found by reading each
upstream changelog against our code. Every claim below was checked against both our
source and the upstream crate source in `~/.cargo/registry`. Nothing in this plan has
been implemented yet.

Waves are ordered by what they need, not by size:

- Wave 1 is code-only and can run any time.
- Wave 2 needs the engine running, so only run it when no other engine instance is up.
- Wave 3 is small optional cleanup.
- Wave 4 lists feature tracks the update makes possible. They need design before they
  can be scheduled.

---

## Wave 1 — Code-only fixes

### 1.1 Combine friction and restitution like Havok

**Why**: Havok combines both coefficients as a geometric mean, `sqrt(a * b)`.
Both SDKs we have agree:

- `havok-20070919/Source/Physics/Dynamics/Common/hkpMaterial.inl:38-46`
- `havok-2013/Physics2012/Dynamics/Common/hkpMaterial.inl:49-57`

rapier 0.35 added `CoefficientCombineRule::GeometricMean`. We never set a rule, so we
get rapier's default, `Average`. The visible difference: an object authored with
restitution 0 still bounces off a bouncy surface under `Average`, and never does in
Havok. The same goes for friction-0 surfaces.

**Steps**:

- Set `friction_combine_rule` and `restitution_combine_rule` to `GeometricMean` at
  the two collider builders:
  - `crates/physics/src/sync.rs:1128` (every streamed or imported collider,
    including the synthesized terrain and architecture trimeshes)
  - `crates/physics/src/ragdoll.rs:438` (ragdoll bones)
- Define the rule once (a `ContactConfig` field or a crate constant) and use it at
  both sites, so they can't drift apart.
- `GeometricMean` has the highest rule priority
  (`rapier3d-0.36.1/src/dynamics/coefficient_combine_rule.rs:62-66`). So any pair
  with one Havok-derived collider uses it, whatever the other collider asks for.

**Done when**:

- Both builders use the shared rule.
- A physics test drops a restitution-0 ball onto a restitution-0.8 floor and asserts
  it doesn't bounce. Check that the test fails with the rule removed.
- `docs/engine/physics.md` records the rule and the Havok citation.

**Estimate**: S.

### 1.2 Stale-comment sweep

**Why**: the update changed the facts several comments rely on, and the docs pass
during the update missed these. They fall into two groups.

**Comments that say a guard prevents the old broad-phase panic.** The rapier 0.22
multi-SAP broad phase is gone, and so is the panic. The guards should stay, since they
still catch corrupt data, but their stated reason has to change:

- `byroredux/src/ragdoll.rs:317`
- `byroredux/src/commands/physics.rs:204`
- `crates/physics/src/world/mod.rs:328`, `:1567`, `:1638`, `:2229`
- `crates/physics/src/ragdoll.rs:41`
- `crates/physics/src/sync.rs:1252`

Comments that already describe rapier 0.22 in the past tense (e.g.
`world/mod.rs:100`, `recovery.rs:41`) are fine and should be left alone.

**Version references that are now wrong:**

- `Cargo.toml:81`: the gpu-allocator comment cites egui-ash-renderer 0.11.0. The
  reason still holds (0.13 still wants gpu-allocator 0.28); only the version number
  is wrong.
- `Cargo.toml:242-247`: says egui 0.33 is the newest version egui-ash-renderer
  allows.
- `Cargo.toml:249-250`: the eframe comment says "wgpu 27"; it is 30 now.
- `Cargo.toml:222`: sysinfo "Pinned to 0.30".
- `tools/byro-launcher/Cargo.toml:11`: "pinned `egui 0.33`".
- `crates/renderer/src/vulkan/sync.rs:101-102` (rider 14): names the deprecated
  `free_textures` / `set_textures`. The calls are now `free_texture` /
  `set_texture`. The ordering argument itself is unchanged.
- `crates/ui/src/prepare.rs:44-58`: cites the old Ruffle rev `0dde9813` and hopes a
  re-pin will fix it. At `2edfd97d`, `swf/src/read.rs:1929-1939` still rejects
  PlaceObject3 records with neither MOVE nor HAS_CHARACTER, so the shim is still
  needed. Update the citation and say so.
- `crates/mod-runtime/src/limits.rs:25`, `:68`: cite wasmtime 47.0.3. Both claims
  still hold on 49.0.2.
- `crates/mod-runtime/src/limits.rs:29`: says "no engine consumer yet".
  `byroredux/src/extensions/systems.rs:20` builds the host now, so the
  dedicated-thread TODO next to it can be acted on.
- `crates/audio/src/lib.rs:177`, `:192`, `:211`, `:859`, `:1150`: cite kira 0.10.x.
  The quoted values and limitations are unchanged in 0.12; only the version numbers
  are wrong.
- `crates/audio/src/lib.rs:1619`: says `from_cursor` needs `'static`. kira 0.12.4
  relaxed that. No caller benefits, but the comment is wrong.
- `crates/bsa/src/ba2.rs:846`, `:2193`, `:2312`: cite lz4_flex 0.11.6. Re-check each
  claim against 0.14 and update it. The `catch_unwind` at `ba2.rs:873` stays, because
  0.14 still documents that it "may panic".

**Done when**: none of the listed lines asserts a fact the current pins contradict.
Running `grep -rn -i 'multi-sap'` turns up only past-tense mentions.

**Estimate**: S (doc only). Keep it to one commit so it reviews in one go.

### 1.3 Trim `image` 0.25's default features

**Why**: 0.25's default formats include `avif`, which pulls in the rav1e AV1 encoder
(`cargo tree -i rav1e` shows `ravif → image`). Our workspace line `image = "0.25"` is
the only thing turning the defaults on: eframe, arboard and Ruffle all use
`default-features = false`. That means extra compile time and target-dir size for a
format we never touch.

**What we use**:

- PNG encode in `crates/renderer/src/vulkan/context/screenshot.rs` and
  `byroredux/src/commands/assets.rs`.
- PNG decode in the byroredux integration tests (`golden_frames.rs`,
  `renderer_anchor.rs`, `cornell_rt_oracle.rs`, `upscaler_quality.rs`).
- In `tools/texture-upscale/src/pipeline.rs:217`, `:421`, decoding of the upscaler
  output and of source textures (DDS, PNG, TGA).

**Steps**:

- Workspace: `image = { version = "0.25", default-features = false, features = ["png"] }`.
- `tools/texture-upscale/Cargo.toml`: add `features = ["dds", "tga"]`. Check its
  real input formats first.

**Done when**:

- `cargo tree -i rav1e` is empty.
- The workspace builds and its tests pass, including the screenshot, `tex.dump` and
  golden-frame tests.
- texture-upscale still decodes a DDS, a PNG and a TGA.

**Estimate**: S.

### 1.4 Regression tests for wasmtime's fuel changes

**Why**: two upstream changes touch the mod sandbox's CPU guard.

- **wasmtime 48 (#13931)**: variable-length operations are now charged by size. By
  default `memory.fill`, `memory.copy` and `memory.init` cost one fuel unit per byte
  (`wasmtime-environ-49.0.2/src/tunables.rs:520-560`). On 47, a 64 MiB fill cost
  one unit. That closes a CPU-exhaustion hole for free. But it also means
  `fuel_per_entry` (10M by default, `crates/mod-runtime/src/limits.rs:104`) now
  limits bytes moved too, including passive data-segment init during instantiation
  (fuel is set at `crates/mod-runtime/src/runtime/sandbox.rs:396`).
- **wasmtime 48.0.3 / 49.0.1 (GHSA-m63x-6p34-q65x)**: "Do not drop fuel-spend
  accrued by callees of `call_ref`". Function references are on by default in our
  engine config (`sandbox.rs:178-181`), so before this update a guest could get round
  the fuel guard.

**Steps**: add two WAT cases next to `fuel_exhaustion_quarantines_runaway_guest`
(`crates/mod-runtime/src/tests.rs:2067`):

1. A `memory.fill` of more bytes than `fuel_per_entry`, in a single instruction.
2. A `call_ref` loop.

Both must end quarantined. Update the fuel doc at `limits.rs:49-58` so it says fuel
now covers bytes moved.

**Done when**: both tests pass on 49.0.2 and the doc names the per-byte cost.

**Estimate**: S.

### 1.5 Re-check the character-grounding workaround (#3799) against rapier 0.36

**Why**: `resolve_ground_contact` (`byroredux/src/systems/character.rs:1599`) ORs
our own downward probe into the KCC's `grounded` flag. It does that because rapier
0.22 returned `grounded = false` for a capsule at rest: "a zero-length sweep is
precisely the input for which Rapier's KCC cannot observe contact". rapier 0.36
changed exactly this case:

- `control/character_controller.rs:429-440` now calls the grounded check explicitly
  when the move is below 1e-5 ("When not moving … we call it explicitly here"). 0.22
  had no such branch.
- Since 0.35, snap-to-ground fires on any movement that isn't upward.

The probe also does two other jobs: it corrects drift on sloped trimeshes, and it
screens out unwalkable slopes (#3971). So it stays either way. What has changed is
only the reason for the OR.

**Steps**:

- Add a physics-crate test that drives `PhysicsWorld::move_character` for a capsule
  resting at `kcc_offset_bu` above a floor, using production settings (offset 4 BU,
  snap-to-ground and autostep on). Request a desired translation of about zero for
  60 frames and assert `grounded` on every frame.
  - The existing #3799 tests feed the KCC verdict in as a literal (`kcc_grounded =
    !grounded`). None of them exercises real rapier.
- If it passes, rewrite the rationale at `character.rs:1599-1632`. The OR stays as a
  defence, but it is no longer a workaround for a rapier bug.
- If it fails, record why. The case could be the tiny non-zero correction the
  grounded branch requests rather than an exact zero.

**Done when**: rapier's behaviour is pinned by a test, and the doc states which case
the OR still covers.

**Estimate**: S–M.

### 1.6 Re-scope #5530

**Why**: #5530 (an unchecked translation at newcomer registration and
`set_kinematic_translation`) is filed as "the same panic class" as the multi-SAP
crash. With the BVH broad phase, a finite but very large coordinate can't panic. What
is left is a correctness problem: a corrupt save or placement teleports the player or
body far away.

**Steps**:

- Comment on the issue with the rapier 0.36 reasoning.
- Either downgrade it to "corrupt input accepted silently" and keep the suggested
  bound, or close it as moot. That decision belongs to the issue owner.

**Done when**: the issue no longer describes a crash that can't happen.

**Estimate**: XS. This is a tracker action, so confirm before posting.

---

## Wave 2 — Measurements and live checks (engine required)

Run only when no other `byroredux` instance is up (see the project memory note on
parallel launches). Release builds need `BYRO_DEBUG_SERVER=1` for byro-dbg.

### 2.1 Re-test #4772 (FO3 restore: first ragdoll solve jumps ~1e12 BU)

**Why**: #4772 asks for the root cause of the FO3 copied-save restore, where the
first ragdoll solve throws the root about 1e12 BU and the recovery code hides it.
Two rapier 0.35 changes match that symptom:

- **0.35.2**: guards multibody solving when the mass matrix is nearly singular. "The
  free-velocity update is re-solved with the plain mass matrix whenever it injects
  more energy than the applied forces' work."
- **0.35**: joint-limit rows now cap their position-correction bias, so "a deep limit
  violation recovers over a few steps instead of catapulting the bodies". A restored
  pose that seeds a joint past its limit is exactly that input.

**Steps**:

1. Run `docs/smoke-tests/p2-melee-core.sh fo3`.
2. Read the `phys.stats` recovery counters and `ragdoll.status` after the restore leg.
3. If recoveries are zero and the joints are intact, close #4772 and make the FO3
   gate assert zero recoveries (the #4683 follow-up).
4. If not, the jump is ours to root-cause, and the candidates in the issue body still
   apply.

**Done when**: #4772 is either closed with the gate asserting zero, or updated with
the 0.36 measurement.

**Estimate**: S to measure; M if the gate needs changing.

### 2.2 Re-measure the FO4 grid-cross broad-phase cost

**Why**: the 2026-09-28 streaming measurements left a 52–60 ms rapier MultiSAP
broad-phase hitch every time a few thousand proxies were added or removed. rapier
0.35 reworked the broad phase for large, mostly static worlds ("scenes with very
large static collider counts now pay near-zero per-step broad-phase cost"). It is
likely gone, but that is unmeasured.

**Steps**:

1. Bench the FO4 Commonwealth `(0,0)` radius 1 grid-cross with `BYRO_PROFILE=1` and
   compare the SLOW FRAME lines and physics share against the 2026-09-28 numbers
   (readiness about 1.27 s, physics 54 ms on the first presented frame).
2. Use the quiet-window bench procedure (separate worktree and target dir, no
   compiling during the bench).

**Done when**: the hitch is measured as gone, or re-attributed with numbers. Update
the project memory note on archive-I/O parallelism either way.

**Estimate**: S.

### 2.3 Tab never opens the inventory during normal play (unrelated to the update)

**Why**: traced in code, not yet seen live.

- egui-winit always reports Tab as consumed: "When pressing the Tab key, egui focuses
  the first focusable element, hence Tab always consumes" (`egui-winit-0.36.2/src/lib.rs:416-419`).
  0.33 did the same.
- `DebugUiState` exists for the whole session, so with no menu open,
  `byroredux/src/app_events.rs:457` returns on `egui_consumed`. That happens before
  the inventory branch at `app_events.rs:537`.
- So the default Tab → Inventory binding (`byroredux/src/interaction.rs:167`) never
  fires.

**Steps**:

1. Confirm live: press Tab in a loaded cell with no overlay open.
2. If it reproduces, honour `egui_consumed` for key presses only while an egui
   surface is actually showing. Check every egui surface (debug overlay, game menu,
   Studio, console) before choosing the predicate.
3. Add a test on the routing predicate.

**Done when**: Tab opens the inventory with no overlay up, and still goes to egui when
an egui text field has focus.

**Estimate**: S.

---

## Wave 3 — Small optional cleanups

Batch these as convenient. Each is S effort with low risk.

- **egui frame discards**: replace `output.textures_delta.clear()` with egui 0.36's
  `FullOutput::drop_without_applying_deltas()` at `byroredux/src/app_frame.rs:847`,
  `crates/renderer/src/vulkan/context/mod.rs:1456` and
  `crates/renderer/src/vulkan/context/teardown.rs:273`. Keep `clear()` in the
  `panels.rs:1782` test helper, which returns the output afterwards.
- **Undrained egui drops**: `crates/debug-ui/src/lib.rs:236`, `:258`, `:288` set
  `last_output = None` without draining texture deltas. That would trip egui 0.36's
  debug assert if it ever ran with a frame pending. It doesn't today, because
  `app_frame.rs` takes the output right after `run()`. Drain first, or delete the
  lines if they are dead.
- **rfd**:
  - Raise the minimum to `"0.17.1"` (`Cargo.toml:253`). 0.17.0 returns
    percent-encoded portal paths, which breaks folders with spaces. Cargo.lock is
    already on 0.17.2, so this only protects a fresh resolve.
  - Optionally parent the launcher's folder picker to its window with
    `.set_parent(frame)` in `tools/byro-launcher/src/app.rs:132`. On Wayland that
    needs rfd's `wayland` feature.
- **glam**:
  - Collapse the three identical `[f32; 3]` lerps into one: `byroredux/src/components.rs:1241`,
    `byroredux/src/systems/weather.rs:264`, `byroredux/src/render/water.rs:49`.
  - Optional drop-ins: `FloatExt::smoothstep` for `render/fog_volumes.rs:284` and
    `volumetrics/noise.rs:142`; `Vec3::rotate_y` for `bench_camera.rs:391`;
    `Quat::from_rotation_axes` for the `from_mat3` step in `crates/physics/src/ragdoll.rs`.
  - Do NOT swap the local scalar lerps for `FloatExt::lerp`: glam 0.33.11 changed its
    formula to `a*(1-t) + b*t`.
  - Leave the Gamebryo-parity math alone (the Shepperd matrix-to-quat, the
    `exp_map_rel` / `quat_log_rel` helpers, `rotation_scaled_axis`).
- **Ruffle**:
  - AVM2 `ExternalInterface.addCallback(name, null)` now removes a callback
    upstream, but our callback set (`crates/ui/src/host.rs`) keeps the name. Check
    `external_interface.get_callback(name)` before invoking.
  - Skip `ImportAssets` tags with an empty URL in `navigator.rs`
    `import_asset_paths_from_tags`, matching upstream.
  - Optionally use `PlayerBuilder::with_locale(DeterministicLocaleBackend)` for
    reproducible test captures.
- **phys.stats**: show the contact-pair, contact and constraint counts that rapier
  0.36 now fills into `PhysicsPipeline::counters` (enabled by default; the timers
  still need the `profiler` feature).
- **wasmtime**:
  - Pin the guest proposal set explicitly in `SandboxRuntime::new` (49 turned
    wide-arithmetic on by default; relaxed-SIMD can give different results on
    different CPUs). Check our guest toolchains first.
  - Set `Store::set_hostcall_fuel` so `log(message)` can't copy up to 64 MiB of guest
    memory before the 16 KiB check (`runtime/host/logging.rs:13`).
- **toml_edit**: bump game-detect to 0.25, which Cargo.lock already carries through
  proc-macro-crate. That removes the duplicate and puts the validate-then-edit path in
  `crates/game-detect/src/overrides.rs:113-122` on a single TOML grammar.
- **kira**: consider `cpal-realtime` without `-dbus` (real-time audio thread priority
  on Windows; a no-op on Linux; no libdbus needed).
- **sysinfo**: `tools/texture-upscale/src/space.rs:89` refreshes every disk counter.
  Use `DiskRefreshKind::nothing().with_storage()`, and `Disk::is_read_only()` to
  refuse a read-only output disk up front.

---

## Wave 4 — Feature tracks the update makes possible (need design first)

These are larger. Each needs its own plan and has a prerequisite on our side.

- **Surface-aware footsteps and impact sounds.**
  - parry 0.31 reports which sub-shape a query hit (for a trimesh, the triangle).
  - rapier 0.35 added `ContactForceEvent::started`, which is true on the step a
    pair's force first crosses its threshold. That is the "impact" signal Havok's
    collision listeners give.
  - Prerequisites:
    - Havok materials are dropped at the NIF → ECS boundary. The core
      `CollisionShape` has no material field, and the NIF collision import never
      carries one.
    - `footstep_system` plays a single default sound.
    - The IPDS/IPCT impact records are parsed but unused.
  - Order: carry the Havok material through import, map it per triangle, then wire
    footsteps and clutter impacts.
- **Cloth (rapier 0.36 soft bodies).** Ropes, cloth and deformable trimeshes now
  simulate alongside rigid bodies. Skyrim and FO4 capes and banners author their cloth
  in `BSClothExtraData`, which we parse as an opaque Havok blob
  (`crates/nif/src/blocks/extra_data.rs`). This is blocked on decoding the Havok cloth
  data in that blob, the same blob problem PHYSAL already notes for FO4+.
- **Collider debug view.** rapier's `DebugRenderPipeline` (it now colours bodies that
  are eligible to sleep but still awake, and draws trimesh pseudo-normals) could drive
  a wireframe collider overlay. We have no physics debug draw today.
- **Zero-copy Scaleform frames.** wgpu 30 adds `vulkan::Queue::add_wait_semaphore`
  and lets `create_texture_from_hal` keep a texture's initial state. Together those
  make cross-device image sharing with our ash device possible without a CPU fence.
  - It is L effort and high risk: it needs the external-semaphore extension,
    queue-family ownership barriers wgpu won't emit, premultiplied UNORM input, and a
    replacement for the byte compare the pacer uses to detect idle frames.
  - It can't be verified without RenderDoc. Only worth it if the measured 5–8 ms per
    pass (AUDIT_UI_2026-09-21) still hurts after the #4717 pacing work.

---

## Checked and ruled out — don't redo

- **Ragdoll `additional_solver_iterations(12)`** (`crates/physics/src/ragdoll.rs:393`):
  it looked as if its meaning changed in 0.35. It didn't: rapier 0.22 already
  divided `dt` by `num_solver_iterations + additional`
  (`rapier3d-0.22.0/src/dynamics/solver/island_solver.rs:45-49`). Ragdolls ran 16
  substeps before and still do.
- **Ragdoll DOF clamp**: still needed. rapier's per-substep speed cap skips
  multibodies, which are integrated separately
  (`staged_island_solver/worker.rs:640-700`).
- **Ragdoll cone limits**: rapier 0.36 still has no cone or swing joint limit, so the
  per-axis approximation stays.
- **egui-ash-renderer #4986 workaround**: 0.13 still starts partial updates from
  `UNDEFINED`, so `promote_partial_deltas` stays. `image_mirrors` stays regardless,
  because the #5072 re-seed needs it.
- **kira 0.11–0.12.5**: maintenance only. The spatial, filter, reverb and send code
  is unchanged.
- **Ruffle shims**: none became redundant at `2edfd97d`. That covers the synthetic
  ShowFrame, the `record_degraded` placeholder, the AVM2 adapter, the max_stack
  headroom and the PlaceObject3 rewrite.
- **Terrain heightfield fixes (parry)**: terrain is deliberately a trimesh
  (`byroredux/src/cell_loader/spawn.rs`, one collider path for all static geometry),
  so they don't apply.
- **Sleep workarounds**: none exist to remove. rapier 0.35 honours `sleep()` right
  after a reposition, and we never worked around the old behaviour.
- **Kinematic label in `phys.stats`**: still correct. Kinematic bodies never sleep in
  rapier 0.36 (`island_manager/sleep.rs:30`).
- **logos 0.16, lz4_flex 0.14, string-interner 0.20, nvml-wrapper 0.13,
  tracing-tracy 0.12**: nothing to adopt.
  - Keep `fixed_string_serde`: string-interner's own symbol serde writes a usize
    and reads it back as a u32.
  - Keep the lz4 `catch_unwind`.

---

## Suggested order

1. Wave 1.1–1.5 as one sitting. These are all code-only and touch separate files, so
   each can be its own commit.
2. Wave 1.6, posted after confirmation.
3. Wave 2, next time the engine can be launched without clashing with another
   instance. Do 2.1 first, because its result decides whether a gate changes.
4. Wave 3 opportunistically, as filler between larger work.
5. Wave 4 goes to the roadmap as candidate tracks. Surface materials is the most
   self-contained, since its prerequisite (carrying the Havok material through import)
   is ours alone.
