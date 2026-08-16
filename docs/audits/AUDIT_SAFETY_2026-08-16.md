# Safety Audit — 2026-08-16

Run as part of the `comprehensive` `/audit-suite` sweep.
Protocol: `.claude/commands/_audit-common.md` · severity scale:
`.claude/commands/_audit-severity.md` · dimensions:
`.claude/commands/audit-safety/SKILL.md` (all 11).

## Scope line

All 11 dimensions executed. Coverage notes:

- **Dimension 11 (`crates/mod-runtime`) is the first time this dimension has
  ever run** — it was added to the skill on 2026-08-13, one day *after* the
  previous safety audit (`AUDIT_SAFETY_2026-08-12.md`, which stops at Dimension
  10). Audited as a **contract** (Principal / CapabilitySet / SandboxConfig
  trust boundary), not as a live path: the crate still has no consumer in the
  engine, and "unused" is not reported as a finding.
- **`crates/fsr3-sys`** audited as the workspace's only live FFI crossing and an
  engine-default render-path component. **`crates/cxx-bridge` (36 LOC) is NOT
  cited as a live FFI boundary** — only as a scope guard, which holds.
- **`crates/hkx` and `crates/facegen`** swept for panics / OOB / unchecked length
  arithmetic on untrusted archive input, per the extra-scope instruction. Both
  lack a parser-discipline dimension of their own.
- **Dimension 5 was exercised through the sound evidence channel** — the engine
  was run under `BYRO_VALIDATION=1` on two scenes (Cornell + a real FO4
  interior). No Vulkan claim in this report is speculative; none was needed.
- Not covered (out of this skill's scope, named for honesty): the P2 gameplay
  slice's *gameplay* invariants (damage/equip/activation) — only its
  leak/growth/NaN surface was checked; `crates/debug-server`'s command surface.

Dedup performed against `/tmp/audit/issues.json` (269 issues) and
`docs/audits/` (25 prior safety reports). Per-dimension scratch notes:
`/tmp/audit/safety/dim_{01,02_03,04,05,06_10,11}.md`.

## Summary

**5 findings**: 0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW.

This is a low-yield run *because the engine's safety surface is in good shape*,
not because the sweep was shallow. Nine of the ten previously-reported items
that were not filed as issues have been fixed since 2026-08-12 (SAFE-D1-01,
SAFE-D4-03, SAFE-D10-01, SAFE-2026-08-07-02, SAFE-2026-08-07-04 all verified
fixed in the live tree), 683 of 683 `unsafe` blocks carry a SAFETY comment, and
two validation-layer runs over 85 frames emitted **zero VUIDs and zero errors**.

The two MEDIUMs are both in code the *prior* audits could not have found: one in
a crate that had never been audited (`mod-runtime`), one in a crate with no
owner audit skill (`facegen`).

### The finding that matters most

**SAFE-2026-08-16-01** is the only one on a live, default-reachable path. The
FaceGen morph evaluator was written specifically to stop non-finite vertex data
reaching the GPU — it has a module-level "NaN guard" section, three
`is_finite()` gates and two dedicated tests. It checks every *input* and no
*output*, so the one way a finite input set can still produce a non-finite
vertex — floating-point overflow in the multiply — walks straight through it
into the vertex SSBO and the BLAS build, where nothing downstream checks
finiteness either.

---

## Findings

### MEDIUM

#### SAFE-2026-08-16-01: FaceGen morph evaluator's NaN guard checks every input and no output — finite×finite overflow puts ±inf vertex positions into the vertex SSBO and the BLAS build

- **Severity**: MEDIUM
- **Dimension**: 9 (NIFAL boundary — NaN/Inf on the GPU) + extra-scope `crates/facegen` sweep
- **Location**: `crates/facegen/src/eval.rs:62-100` (guard chain at :74, :79, :93; the unchecked product at :82 and the unchecked accumulation at :96-98). Live call site: `byroredux/src/npc_spawn/resumable.rs:971-973`.
- **Status**: NEW
- **Description**: `apply_morphs` computes `v_i' = v_i + Σ_j w_j · scale_j · delta_ji`. It rejects a non-finite weight (`:74`), a non-finite morph scale (`:79`) and a non-finite delta component (`:93`) — but never checks `coeff = w * scale` (`:82`), never checks `coeff * d[k]`, and never checks the accumulated output. Two f32 operands that are each `is_finite()` can multiply to `±inf`; so can a sum of ~80 large finite terms. Both operands come from *separate untrusted sources with no magnitude validation*:
  - `w` is `NpcFaceGenRecipe.fggs/.fgga`, decoded by `read_f32_array_into` (`crates/plugin/src/esm/records/actor/mod.rs:1091-1100`) as raw `f32::from_le_bytes` with no finiteness or range check;
  - `scale` is the per-morph EGM scalar, decoded by `read_f32_le` (`crates/facegen/src/lib.rs:118-120`) as a bare `f32::from_bits` — also unvalidated.

  The module doc at `eval.rs:22-31` states the guard's whole purpose as stopping NaN propagation "to the deformed vertex, then to the GPU", and `nan_delta_skipped` / `nan_weight_skipped` pin exactly the two cases that *are* covered. The overflow corner is untested and unguarded.
- **Evidence**:
  ```rust
  // crates/facegen/src/eval.rs:78-98
  let scale = morphs[j].scale;
  if !scale.is_finite() { continue; }
  let coeff = w * scale;              // <- both finite; product may be ±inf. Unchecked.
  ...
      if !d[0].is_finite() || !d[1].is_finite() || !d[2].is_finite() { continue; }
      out[i][0] += coeff * d[0];      // <- inf * finite = inf. Result never re-checked.
  ```
  The output is assigned straight back onto the imported mesh:
  ```rust
  // byroredux/src/npc_spawn/resumable.rs:971-973
  let after_sym = byroredux_facegen::apply_morphs(&mesh.positions, &egm.fggs_morphs, &fggs);
  mesh.positions = byroredux_facegen::apply_morphs(&after_sym, &egm.fgga_morphs, &fgga);
  ```
  `grep -n is_finite crates/renderer/src/mesh.rs crates/renderer/src/vulkan/acceleration/*.rs byroredux/src/scene/nif_loader.rs` returns **nothing** — there is no downstream finiteness filter between `mesh.positions` and the vertex SSBO / `build_blas_for_mesh`.
- **Impact**: A corrupt or hostile plugin (FGGS slider ≈ 3e38) or a corrupt `.egm` sidecar (morph scale ≈ 3e38) yields `±inf` head-vertex positions for one NPC. Non-finite vertex data in a `VkAccelerationStructureGeometryTrianglesDataKHR` build is undefined per spec; the practical range is a garbage/exploded head mesh through to a device-level fault. Blast radius is one actor's head mesh per bad record, on the FNV/FO3 runtime-FaceGen path (`has_runtime_facegen_recipe`). Not reachable from vanilla content — the vanilla non-finite *sentinel* case is precisely what the existing guard already handles.
- **Related**: #2687 (SAFE-D9-01, save-restore skips `resolve_pbr()`) and #2489 (`mat.set` PBR clamp) are the same class — a renderer-bound producer with no finiteness gate — on the material side rather than the geometry side. `crates/facegen` has **no owner audit skill** (`_audit-common` un-owned-subsystems table), which is why this survived.
- **Suggested Fix**: In `apply_morphs`, check `coeff.is_finite()` after the multiply at `:82` (`continue` on failure, matching the existing skip semantics), and gate the write at `:96-98` on the summed value being finite — or clamp the accumulated position to a sane Bethesda-unit bound. Add an `overflow_delta_skipped` test alongside `nan_delta_skipped`. Optionally reject implausible slider magnitudes at the ESM decode in `read_f32_array_into`.

---

#### SAFE-2026-08-16-02: `SandboxConfig::validate()` enforces only lower bounds — an oversized `max_wasm_stack_bytes` turns guest recursion into a host **process abort**, which wasmtime documents explicitly

- **Severity**: MEDIUM
- **Dimension**: 11 (sandboxed mod runtime — resource limits)
- **Location**: `crates/mod-runtime/src/limits.rs:38-78` (`validate`); consumed at `crates/mod-runtime/src/runtime.rs:94-96`
- **Status**: NEW (this dimension has never been audited before — added to the skill 2026-08-13, one day after the last safety audit)
- **Description**: The skill's Dimension-11 checklist asks that `validate()` reject "degenerate configs (zero/absurd fuel, zero memory)". It rejects **zero** on every field and **absurd on none** — there is no upper bound on `max_component_bytes`, `max_memory_bytes`, `fuel_per_entry`, or `max_wasm_stack_bytes`. The last of those is not merely a soft limit: `max_wasm_stack` is forwarded verbatim to `wasmtime::Config`, and wasmtime 47.0.3 documents that exceeding the calling thread's remaining native stack aborts the process rather than trapping the guest. That converts the sandbox's headline property — *a hostile guest is contained* — into *a hostile guest kills the host* for any embedder that picks a large-but-plausible value.
- **Evidence**: `crates/mod-runtime/src/runtime.rs:96`
  ```rust
  engine_config.max_wasm_stack(config.max_wasm_stack_bytes);
  ```
  wasmtime 47.0.3 `Config::max_wasm_stack` doc (`~/.cargo/registry/src/index.crates.io-*/wasmtime-47.0.3/src/config.rs:804-828`), verbatim:
  > - Let's assume this option is set to 2 MiB and then a thread that has a stack with 512 KiB left. **If wasm code consumes more than 512 KiB then the process will be aborted.**

  `validate()`'s only clause for this field is `if self.max_wasm_stack_bytes == 0`. `SandboxConfig { max_wasm_stack_bytes: 64 * 1024 * 1024, ..Default::default() }` validates clean.
- **Impact**: Latent today — `Default` is 512 KiB (wasmtime's own default) and the crate has no engine consumer, so nothing is broken right now. The moment a consumer lands and someone raises the stack ceiling to accommodate a deep-recursion guest — or instantiates the runtime on a worker thread with a smaller stack than the main thread — an untrusted mod gets an unrecoverable `SIGSEGV`/abort of the whole engine instead of a `FaultInfo` + quarantine. This is exactly the window a contract audit exists to close: the guarantee must be real *before* the first consumer makes it load-bearing.
- **Related**: `crates/mod-runtime` has **no owner audit skill** — `/audit-safety` Dim 11 is its only coverage (`_audit-common` un-owned-subsystems table). Everything else in the limits contract checks out: fuel, memory ceiling, table/instance/memory counts, `trap_on_grow_failure(true)`, and the log budget are all enforced and guard-tested.
- **Suggested Fix**: Add upper-bound clauses to `validate()` — most importantly `max_wasm_stack_bytes` against a conservative ceiling well under the smallest thread stack the engine creates (e.g. 1 MiB), and a documented note that the runtime must be instantiated on a thread whose stack exceeds it. Bounding `max_memory_bytes` and `fuel_per_entry` at the same time costs nothing and makes the whole config self-describing.

---

### LOW

#### SAFE-2026-08-16-03: mod-runtime's log budget is a lifetime-total cap with no drain, so exceeding it permanently quarantines an otherwise-healthy guest

- **Severity**: LOW
- **Dimension**: 11 (sandboxed mod runtime — resource limits / lifecycle)
- **Location**: `crates/mod-runtime/src/runtime.rs:263-301` (the `log` host fn), `:187-189` (`logs()`), `:229-249` (`enter`/`quarantine`)
- **Status**: NEW
- **Description**: The skill asks whether the guest-controlled `logs()` `Vec` is capped. It is — `max_log_entries`, `max_log_message_bytes` and `max_log_bytes` are all enforced, with a `checked_add` on the running byte counter. That half is a clean PASS. The nuance worth recording is what happens *at* the cap: exceeding any of the three calls `wasmtime::bail!`, which traps the guest, which `enter()` converts into `InstanceStatus::Quarantined`. And because `logs()` only lends `&[LogEntry]` with no drain/take API, the budget is a **per-instance-lifetime total**, not a rate limit. A long-running, entirely well-behaved mod that logs at a modest steady rate is therefore guaranteed to be killed eventually — 1024 entries or 1 MiB, whichever comes first — with no way for the host to reclaim headroom.
- **Evidence**:
  ```rust
  // runtime.rs:278-287
  if self.logs.len() >= self.max_log_entries {
      wasmtime::bail!("log entry limit of {} exceeded", self.max_log_entries);
  }
  ...
  if next_log_bytes > self.max_log_bytes {
      wasmtime::bail!("log byte limit of {} exceeded", self.max_log_bytes);
  }
  ```
  ```rust
  // runtime.rs:187-189 — read-only, no drain
  pub fn logs(&self) -> &[LogEntry] { &self.store.data().logs }
  ```
- **Impact**: Fail-closed, so not a security hole — but it turns a diagnostics budget into an unavoidable kill switch, and the failure will look to a mod author like a random crash rather than a quota. Zero impact today (no consumer).
- **Related**: SAFE-2026-08-16-02 (same contract, same crate).
- **Suggested Fix**: Add `take_logs(&mut self) -> Vec<LogEntry>` that drains and credits `log_bytes` back, so the host can pump diagnostics and the cap becomes a genuine backpressure bound; or drop over-cap records with a counter instead of trapping.

---

#### SAFE-2026-08-16-04: no test feeds `SandboxRuntime::compile` hostile non-wasm bytes, and there is no bound on compilation cost for an in-limit adversarial component

- **Severity**: LOW
- **Dimension**: 11 (sandboxed mod runtime — untrusted input at compile time)
- **Location**: `crates/mod-runtime/src/runtime.rs:115-126`; test module `crates/mod-runtime/src/tests.rs`
- **Status**: NEW
- **Description**: The skill's Dimension-11 checklist ends with "verify a malformed component yields `SandboxError`, not a panic — this crate's whole point is that hostile input is expected." By inspection it does: `Component::new` returns `Err`, mapped to `SandboxError::Compile`. But the test module covers oversize input, a WASI import, a fuel-runaway, a memory-ceiling breach and a capability denial — and **never once passes `compile()` a byte string that is not valid wasm**. The one input class that is pure hostile bytes is the one with no test. Separately, `fuel_per_entry` bounds guest *execution* but nothing bounds *compilation*: a 16 MiB component that passes the byte-length check is handed to Cranelift with no CPU or memory ceiling and no `catch_unwind`.
- **Evidence**: `crates/mod-runtime/src/tests.rs` fixture list — `logging_component`, `looping_component`, `oversized_memory_component`, `component_with_wasi_import` — every one is valid WAT compiled by `wat::parse_str`. `runtime.rs:115-126`:
  ```rust
  pub fn compile(&self, bytes: &[u8]) -> Result<CompiledMod> {
      if bytes.len() > self.config.max_component_bytes { return Err(...); }
      let component = Component::new(&self.engine, bytes)
          .map_err(|error| SandboxError::Compile(format!("{error:#}")))?;
      Ok(CompiledMod { component })
  }
  ```
- **Impact**: None today. The gap is that the crate's central claim ("hostile input is expected") is asserted rather than pinned, so a future wasmtime bump or a `Config` change could regress panic-freedom silently.
- **Related**: SAFE-2026-08-16-02, SAFE-2026-08-16-03.
- **Suggested Fix**: Add a `compile_rejects_garbage_bytes` test over a few adversarial inputs (random bytes, a truncated valid component, a core module rather than a component). Note the unbounded compile cost in the `compile` doc comment so the first consumer knows to compile off the frame thread.

---

#### SAFE-2026-08-16-05: `/audit-safety` Dimension 7 names `REFRACT_PASSTHRU_BUDGET = 2` — a backticked symbol that exists nowhere, at a value 4× off the live cap

- **Severity**: LOW
- **Dimension**: Skill doc-rot (Dimension 7 — RT IOR-refraction regression guards)
- **Location**: `.claude/commands/audit-safety/SKILL.md` Dimension 7, first bullet. Ground truth: `crates/renderer/shaders/triangle.frag:1818`.
- **Status**: NEW (distinct from the OPEN #2686, which is about `GLASS_RAY_BUDGET` being a dead constant, and from the 2026-08-12 skill-correction table, which did not cover this symbol)
- **Description**: The skill instructs the auditor to "verify the budget is enforced" and names it `REFRACT_PASSTHRU_BUDGET = 2`. That identifier appears in **no file in the repository** — not in the shaders, not in `shader_constants_data.rs`, not in the generated `shader_constants.glsl`. The live guard is a GLSL-local `const int MAX_REFRACT_PASSTHRUS = 8;`. So the skill's backticked symbol violates the `_audit-common` path/symbol convention (backticks assert present-tense existence), *and* the quoted value is wrong by 4×. It also slips past `.claude/commands/_audit-validate.sh`: the text is written as `` `REFRACT_PASSTHRU_BUDGET = 2` `` — the value is *inside* the backticks, so the gate's symbol extractor sees a multi-token span rather than an identifier and never checks it. A clean run of the gate today therefore does not mean this class of drift is absent.
- **Evidence**:
  ```
  $ grep -rn "REFRACT_PASSTHRU_BUDGET" crates/ byroredux/
  (no matches)
  $ grep -n "MAX_REFRACT_PASSTHRUS" crates/renderer/shaders/triangle.frag
  1818:            const int MAX_REFRACT_PASSTHRUS = 8;
  1861:            for (int passthru = 0; passthru <= MAX_REFRACT_PASSTHRUS; ++passthru) {
  ```
  The safety property itself is **intact** — the loop is bounded at 9 iterations and the `materialKind == MATERIAL_KIND_GLASS` gate is present at `triangle.frag:1433`, `:1895`, `:2030`, `:3458`. Only the documentation is wrong.
- **Impact**: Every future `/audit-safety` run greps for a symbol that cannot be found and must either re-derive the guard from scratch or, worse, conclude the guard is missing. That is the same failure mode that produced the retired "SAFETY-comment gap" work item (#2692) — a skill sending auditors after a haystack with no needle.
- **Related**: OPEN #2274 (same skill, Dimension 3 leak-inventory drift); OPEN #2686 (`GLASS_RAY_BUDGET`). The 2026-08-12 report also recorded, in prose and never as a filed issue, that SKILL.md lines 21-22 still list `crates/plugin`, `crates/facegen` and `crates/ui` as carrying "one `unsafe`" each when all three have **zero** — re-verified today and still unfixed.
- **Suggested Fix**: Replace the symbol and value in Dimension 7 with `MAX_REFRACT_PASSTHRUS` / 8, or de-backtick it and describe the guard behaviourally. Fold in the still-unfixed `plugin`/`facegen`/`ui` correction at the same time and re-run `.claude/commands/_audit-validate.sh`.

---

## Verified-intact regression guards (PASS — not findings)

Recorded so a future run does not re-derive them. Per the skill's procedure
step 8, a confirmed-intact guard is a PASS, not a NEW finding.

### Dimension 1 — FFI
- **`crates/fsr3-sys`**: both `pub unsafe fn` (`Context::create`, `Context::dispatch`) carry `# Safety` sections; `create`'s now states the Vulkan-idle-before-`Drop` requirement that `Drop`'s SAFETY comment cross-references — **SAFE-D1-01 is FIXED** (`lib.rs:365-378`, cites #2692). The `get_device_proc_addr as usize` cast stores a process-lifetime loader symbol; no dangling-fn-pointer hazard.
- **Ruffle/wgpu (`crates/ui`)**: `SwfPlayer::render` (`player.rs:405-440`) takes ownership of the captured image (`into_raw()`), **copies** into the player-owned `pixel_buffer`, and returns `Option<&[u8]>` bounded by `&mut self` — no borrow escapes into the wgpu backend. Ruffle's wgpu device is private to the player and does not share `VulkanContext`'s allocator, so the Dimension-3 allocator-before-device rule does not apply.
- **cxx scope guard**: `crates/cxx-bridge/src/lib.rs` still exposes exactly `native_hello() -> String`. No `*const`, no `&[u8]`, no `Box<…>`, no fn taking a Rust reference. Still a placeholder, correctly.

### Dimension 2 — Memory corruption / UB
- **ECS cached-pointer contract (#35/#1367)**: `QueryRead`/`QueryWrite`/`ComponentRef` (`crates/core/src/ecs/query.rs:64, 135, 143, 289`) resolve the pointer once in `new()` from a guard held as the **first** struct field; `&mut self` gates `&mut *self.storage`. The custom `Drop` impls only call `lock_tracker::untrack_*` — no deref — and run before field drops, so the guard cannot be released while a pointer is live. SAFETY comments still match the field layout.
- **`#[repr(C)]` GPU structs**: `gpu_types.rs` module doc forbids `[f32; 3]`; every vector member is `[f32; 4]`/`[[f32;4];4]`/flat scalar. Pins live: `gpu_material_size_is_348_bytes`, `gpu_instance_layout_tests`, and `reflect.rs:543` asserting `size_of::<GpuCamera>()` against the SPIR-V-declared UBO.
- **NIF bulk POD reads**: `read_pod_vec` (`crates/nif/src/stream.rs:438-469`) keeps its `checked_mul` byte-count guard, the `check_alloc` cap, the `T: AnyBitPattern` bound and the big-endian compile gate, under a 17-line SAFETY comment.
- **pex opcode transmute**: `OpCode::from_u8` (`crates/pex/src/opcode.rs:130-137`) range-checks `byte < MAX_OPCODE` (51) before the transmute; discriminants are contiguous (only `Nop = 0` is explicit); `OPCODES: [(&str, u8, bool); MAX_OPCODE as usize]` makes a table/enum desync a compile error. Pinned by `discriminants_match_on_disk_order` and `from_u8_round_trips_and_rejects_oob`.
- **sfmaterial**: `BuiltinType::from_u32` is still a checked `match` with an `Err(UnsupportedBuiltin)` default; the crate contains zero `unsafe`, so the module doc's "transmute" prose remains prose.

### Dimension 3 — Leaks & drop ordering
- **Rapier release on cell unload (#1520)**: `byroredux/src/cell_loader/rapier_release_tests.rs` present; `crates/physics/src/world.rs::remove_body` live.
- **Deferred-destroy drain (#418/#732)**: `context/draw.rs:1591-1611` ticks the mesh/texture/accel queues **after** the fence wait.
- **`AllocatorResource` removal (#1406)**: `byroredux/src/app_events.rs:59` removes the resource; re-inserted on `resumed` at `:125-126`.
- **SAFE-2026-08-07-04 is FIXED**: the BLAS-scratch-shrink call in `finish_unload_batch` (`byroredux/src/cell_loader/unload.rs:275-283`) carries an explicit `// SAFETY:` block again.
- **P2 gameplay slice** (`combat.rs`, `inventory.rs`, `interaction.rs`, `settings_io.rs`, ~2.6k LOC landed 2026-08-15/16): no unbounded per-frame growth. `CombatState.last` is a single `Option`, not an accumulating trace; `InventoryCatalog.entries` is a load-time metadata cache keyed by form id (by-design per `_audit-common`); `interaction.rs`'s candidate map is per-call.

### Dimension 4 — Unsafe-block discipline
Mechanised sweep of `crates/` + `byroredux/` + `tools/`: **683 `unsafe {` blocks,
683 with a SAFETY comment.** Eight apparent misses all resolve to false positives
on manual read (the house convention places `// SAFETY:` as the first line
*inside* the block, and several post-#2683 comments now run 10-17 lines, longer
than any fixed window). Of 79 `unsafe fn`, the only one without a caller contract
is `debug.rs:44 unsafe extern "system" fn debug_callback` — a Vulkan-invoked
callback with no Rust caller, so there is no contract to state. **SAFE-D4-03 and
SAFE-2026-08-07-02 are both FIXED.**

Per-crate recount (`grep -ro unsafe <crate>/src | wc -l`, token counts):
renderer 775 · nif 11 · fsr3-sys 11 · core 6 · byroredux 2 ·
plugin/facegen/cxx-bridge/ui/pex 1 each (the first four of those are prose or
log-string matches, **not code**) ·
save/hkx/mod-runtime/physics/bsa/bgsm/sfmaterial/spt/scripting/papyrus/audio/debug-server **0**.
`hkx` and `mod-runtime`'s zeros verified directly — for both, the absence is the
safety property.

### Dimension 5 — Vulkan spec compliance
Exercised through the sound evidence channel, debug build, `BYRO_VALIDATION=1`
(sync-validation confirmed active in the log at `vulkan::instance`):

| Scene | Frames | VUIDs | ERROR lines |
|---|---:|---:|---:|
| `--cornell` (self-contained RT harness) | 45 | **0** | **0** |
| `--game fo4 --cell DmndDugoutInn01` (real content: streaming, AS build, FSR path) | 40 | **0** | **0** |

Logs retained at `/tmp/audit/safety/val_cornell.log` and
`/tmp/audit/safety/val_fo4.log`. Every WARN in the FO4 run is
`byroredux_plugin::esm::cell::support` #1620 ARMO MODL control-byte content
noise — parser, not Vulkan. **No speculative barrier / render-pass / pipeline
claim is made in this report; none was warranted.**

Named guards re-verified statically: TLAS resize still calls
`device.device_wait_idle()` before freeing the old allocation
(`acceleration/tlas.rs:1006`, #1390, now inside the post-#2929 allocate-then-swap
arm); `VOLUMETRIC_OUTPUT_CONSUMED` is `true` (`volumetrics.rs:456`) and **both**
callers gate on the constant by name (`context/draw.rs:663`,
`context/post_passes.rs:494`) rather than assuming a state; `initialize_layouts`
exists on all seven storage-image passes (bloom, gbuffer, caustic, taa,
water_caustic, svgf, volumetrics).

### Dimension 6 — R1 material table
`MAX_MATERIALS = 16384` (`scene_buffer/constants.rs:191`); `MaterialTable::intern`
caps and routes over-cap interns to the neutral default (`material.rs:1087`);
`upload_materials` (`upload.rs:646-680`) carries a **release-visible** `assert!`
plus `.min(MAX_MATERIALS)` — intern cap and upload truncation are in lockstep.
Size pin `gpu_material_size_is_348_bytes` (`material.rs:1382`) matches its
asserted value. Not re-reported: **#2688** (OPEN — the GLSL scalar *type* is not
pinned).

### Dimension 7 — RT IOR refraction
Loop cap enforced (`triangle.frag:1818` + `:1861`); `materialKind ==
MATERIAL_KIND_GLASS` gate present at `:1433`, `:1895`, `:2030`, `:3458`;
`DBG_VIZ_GLASS_PASSTHRU = 0x80` (`shader_constants_data.rs:392`) still uncollided
and mirrored correctly to `shader_constants.glsl:126`. Not re-reported:
**#2686** (OPEN — `GLASS_RAY_BUDGET` is a dead constant). The skill's own text is
SAFE-2026-08-16-05 above.

### Dimension 8 — NPC / animation spawn
FLT_MAX pose-fallback sentinel live throughout `crates/nif/src/anim/bspline.rs`
(`:178, :328, :359, :394, :427`) — #772 guard intact.
`AnimationClipRegistry` interns as `CanonKey::Owned(key.to_ascii_lowercase())`
(`registry.rs:212`) — #790 case-insensitive dedup preserved.
`SkinSlotPool` keeps its one-shot `overflow_warned` + `overflow_attempt_count`
with bind-pose fallback (`skin_slot_pool.rs:99-182`). Not re-reported:
**#2689** (OPEN — the slot vector grows monotonically).

### Dimension 9 — NaN/Inf on the GPU
`translate_material` calls `resolve_pbr()` on both exit paths
(`material_translate.rs:230`, `:308`); the `mat.*` console writes call it
(`commands/scene.rs:906`, `:913`); `Material::default()` seeds a finite
`metalness: 0.0`. Not re-reported: **#2687** (OPEN — save-restore producer skips
`resolve_pbr()`), **#2489** (OPEN — `mat.set` clamp). The geometry-side hole is
SAFE-2026-08-16-01 above.

### Dimension 10 — debug-ui / egui overlay
`EguiPass::new` now destroys `render_pass` on **every** constructor failure path
(`egui_pass.rs:101-157`) — **SAFE-D10-01 is FIXED (#2685)**. The one-frame
`pending_free` defer is intact (`:233`, `:300`) and drained again in `destroy()`
(`:316`). `VulkanContext::drop` takes and destroys `egui_pass`
(`context/mod.rs:3822`) ahead of the device teardown.

### Extra scope — `crates/hkx` and `crates/facegen` parser discipline
Both parse untrusted archive binary with no parser-discipline dimension of their
own. Swept for panics / OOB / unchecked length arithmetic:

- **`crates/hkx` (1282 LOC, zero `unsafe`) — clean, exemplary.** Section table
  validated as a monotonic chain (`packfile.rs:79-98`: `start ≤ local_fixups ≤
  global_fixups ≤ virtual_fixups ≤ exports ≤ end ≤ bytes.len()`), which is what
  makes the direct slice in `cstr` (`:229`) safe. `data_slice` uses `checked_add`
  on both ends and bounds against the section *and* the file. Every scalar read
  goes through `take`/`read_u32`/`read_f32` with `bytes.get(..)`. Dimension
  plausibility gates on the spline clip (`animation.rs:131-145`) reject zero and
  absurd counts and pin `mask_size == transform_count * 4 + float_count`, which
  is precisely what makes the unchecked-looking `&data[block_start..block_start +
  transform_count * 4]` at `:190` in-bounds by construction. Finiteness is
  enforced at the leaves (`read_f32:675-682`, `normalize_quaternion:813-830`,
  `read_qs_transform:791-812`) and zero-length quaternions are rejected. No
  finding.
- **`crates/facegen` (989 LOC, zero `unsafe`)**: `.tri`/`.egm`/`.egt` headers are
  all magic-checked, count-capped (`MAX_VERTICES = 1 << 20`, `MAX_MORPHS = 1024`,
  `MAX_TEXTURE_DIM`) and then pinned by an **exact** file-size equality
  (`egm.rs:124`, `egt.rs:123`), which is what makes the inner unchecked indexing
  safe. `egt.rs:113-120` uses `checked_mul` for `width × height`. No parse-side
  finding. The one issue is in the *evaluator*, not the parser —
  SAFE-2026-08-16-01.

---

## Report finalization

Report: `docs/audits/AUDIT_SAFETY_2026-08-16.md`
No GitHub issues were created. To file:

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-08-16.md
```
