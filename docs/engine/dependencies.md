# Dependencies

External dependencies live in the workspace root [`Cargo.toml`](../../Cargo.toml)
under `[workspace.dependencies]` and are consumed by individual crates via
`{ workspace = true }`. The internal crate graph is documented in
[Architecture Overview](architecture.md#crate-dependency-graph).

> Last reconciled against the tree on **2026-05-28** (Session 42 closeout).
> The workspace has grown to **19 crates under `crates/`** plus the
> `byroredux` binary and the `byro-dbg` tool since the Session 7 (2026-04-09)
> revision of this doc — the dependency surface below reflects the current
> `Cargo.toml` files, not the early-April snapshot.

## External Dependencies

Versions are the workspace-level pins in the root `Cargo.toml`. "Used by"
lists the crates that pull the dependency in (transitive consumers via the
internal graph are not repeated).

| Crate              | Version  | Used by                                       | Purpose                                                       |
|--------------------|----------|-----------------------------------------------|---------------------------------------------------------------|
| **Vulkan / GPU**   |          |                                               |                                                               |
| ash                | 0.38     | renderer, debug-ui                            | Raw Vulkan bindings                                           |
| ash-window         | 0.13     | renderer                                      | Surface creation from window handles                          |
| gpu-allocator      | 0.28 (`vulkan`) | renderer, debug-ui                     | Vulkan memory allocation (shared `SharedAllocator`)           |
| rspirv             | 0.12     | renderer                                      | SPIR-V reflection — cross-check descriptor layouts vs shader declarations (#427) |
| **Windowing**      |          |                                               |                                                               |
| winit              | 0.30     | platform, renderer, debug-ui, byroredux       | Cross-platform windowing                                      |
| raw-window-handle  | 0.6      | platform, renderer                            | Platform-agnostic window handle traits                        |
| **Math**           |          |                                               |                                                               |
| glam               | 0.29 (`mint`) | core, physics, audio                     | Linear algebra (Vec, Mat, Quat); `mint` feeds kira's spatial API |
| nalgebra           | 0.33     | nif, physics                                  | SVD for degenerate NIF rotation repair; ABI match for rapier3d |
| **Physics**        |          |                                               |                                                               |
| rapier3d           | 0.22 (`simd-stable`) | physics                            | Rigid-body / collision sim (kinematic character controller, M28.5) |
| **Audio**          |          |                                               |                                                               |
| kira               | 0.10     | audio                                         | 3D spatial audio — SpatialScene, reverb, streaming (M44)      |
| **Parallelism**    |          |                                               |                                                               |
| rayon              | 1        | core (`parallel-scheduler`), byroredux        | Parallel system dispatch (M27) + draw-command sort            |
| **Strings**        |          |                                               |                                                               |
| string-interner    | 0.17     | core                                          | O(1) string equality via interning                           |
| **Identity**       |          |                                               |                                                               |
| uuid               | 1 (`v5`, `serde`) | core, plugin                         | Plugin identity (`v5` content hashing)                       |
| semver             | 1 (`serde`) | plugin                                     | Plugin version constraints                                    |
| **Serialization**  |          |                                               |                                                               |
| serde              | 1 (`derive`) | plugin, debug-protocol, debug-server, byroredux, core (`inspect`) | Manifest + debug-wire + profile serialization |
| serde_json         | 1        | debug-protocol, debug-server, byro-dbg, core (`inspect`) | Length-prefixed JSON debug protocol            |
| toml               | 0.8      | plugin, byroredux                             | Plugin manifest + game-profile format                         |
| **Compression**    |          |                                               |                                                               |
| flate2             | 1        | bsa, plugin                                   | Zlib decompression for BSA + BA2 + ESM records                |
| lz4_flex           | 0.11     | bsa                                           | LZ4 frame (BSA v105) + LZ4 block (BA2 v3 / Starfield) decompression |
| **Image**          |          |                                               |                                                               |
| image              | 0.24     | renderer, byroredux (dev — golden frames)     | PNG / non-DDS image loading; PNG decode in screenshot regression tests |
| **Profiling**      |          |                                               |                                                               |
| tracing            | 0.1      | byroredux                                     | Wall-clock span ladder for the cell-load critical path (#886) |
| tracing-subscriber | 0.3      | byroredux                                     | `fmt` + `env-filter` span output                              |
| tracing-tracy      | 0.11     | byroredux (`tracing-tracy` feature, opt-in)   | Pipe spans into a Tracy capture session                       |
| dhat               | 0.3      | nif (`dhat-heap` feature, opt-in)             | Heap-allocation regression gate for NIF-PERF / #408 pins (#1247) |
| sysinfo            | 0.30     | byroredux                                     | Host CPU/RAM sampling for the debug UI `MetricsSnapshot`      |
| **Debug UI**       |          |                                               |                                                               |
| egui               | 0.33     | renderer, debug-ui                            | CPU-side immediate-mode UI for the embedded overlay (Phase 4) |
| egui-winit         | 0.33     | debug-ui                                      | winit event → egui input bridge                               |
| egui-ash-renderer  | 0.11 (`gpu-allocator`) | renderer, debug-ui                | GPU pipeline for tessellated egui primitives; shares `SharedAllocator` |
| ratatui            | 0.28     | byro-dbg                                       | Terminal UI for `byro-dbg --tui` (Phase 3)                    |
| crossterm          | 0.28     | byro-dbg                                       | Terminal backend for ratatui                                  |
| **C++ interop**    |          |                                               |                                                               |
| cxx                | 1        | cxx-bridge                                     | Type-safe C++ FFI                                            |
| cxx-build          | 1 (build)| cxx-bridge build script                       | C++ compilation for the cxx bridge                            |
| **Parsing**        |          |                                               |                                                               |
| logos              | 0.15     | papyrus                                        | Lexer derive for the Papyrus tokenizer (M30)                  |
| bitflags           | 2        | papyrus                                        | Bitflag types in the Papyrus AST                              |
| **Logging**        |          |                                               |                                                               |
| log                | 0.4      | nearly all crates                              | Logging facade                                                |
| env_logger         | 0.11     | byroredux, examples, integration tests         | Stderr log output                                            |
| **Errors**         |          |                                               |                                                               |
| anyhow             | 1        | plugin, platform, renderer, ui, debug-ui, byroredux | Context-rich error handling                            |
| thiserror          | 2        | core, renderer, bsa, bgsm, sfmaterial, nif, spt, facegen, papyrus | Error type derives                            |

### Per-crate (non-workspace) dependencies

A few crates pin their own versions outside the workspace table, because the
dependency is only ever consumed by that one crate:

- **`byroredux-ui`** vendors the **Ruffle** Flash player from git (pinned to
  rev `0dde9813…`, nightly-2026-03-28): `ruffle_core`, `ruffle_render`,
  `ruffle_render_wgpu`, `ruffle_video_software`, and `swf`. It also pulls
  **`wgpu` 27** (re-exported by `ruffle_render_wgpu` but needed for type
  references), **`futures` 0.3** (async executor for wgpu device creation),
  and its own **`image` 0.25** (`default-features = false`, for `RgbaImage`
  from `capture_frame` readback). This is the one place the workspace runs a
  second `image` major series and a wgpu-backed render path — it stays inside
  the UI crate so the main Vulkan renderer never links wgpu.
- **`byroredux-debug-protocol`** pins `serde_json = "1"` directly rather than
  via the workspace alias; everywhere else `serde_json` is workspace-managed.
- **`byroredux`** (binary) carries a **dev-dependency on `tempfile` 3** for
  the game-profile loader's merge-shipped-with-user round-trip test
  (debug-UI Phase 5).

### Notable version notes (carried from the root `Cargo.toml` comments)

- **gpu-allocator 0.27 → 0.28** was bumped to align with
  `egui-ash-renderer 0.11`'s transitive pin so the dep graph holds one
  allocator copy, not two. The minor API bump (`Allocation::is_null`
  removed, `Allocator::report` → `generate_report`) is source-compatible —
  the renderer already used `generate_report`.
- **egui / egui-winit pinned at 0.33** (not 0.34) because 0.33 is the latest
  series `egui-ash-renderer 0.11.0` pins transitively; bumping the direct dep
  to 0.34 would put two copies of `egui` in the graph and break the
  `FullOutput` pass-through.
- **sysinfo pinned at 0.30** (last stable pre-0.31 API churn) to dodge the
  per-release `global_cpu_info` / `refresh_processes_specifics` rename
  treadmill.
- **rapier3d / nalgebra ABI match**: rapier3d 0.22 is selected so its
  internal nalgebra matches the workspace `nalgebra 0.33` pin (one copy).

## Internal Crate Dependencies

| Crate                    | Depends on                                                              |
|--------------------------|-------------------------------------------------------------------------|
| byroredux-core           | (none — leaf; optional `rayon` under `parallel-scheduler`, `serde`/`serde_json` under `inspect`) |
| byroredux-bsa            | (none — leaf)                                                           |
| byroredux-bgsm           | (none — leaf; `byroredux-bsa` dev-only)                                 |
| byroredux-sfmaterial     | (none — leaf; `byroredux-bsa` dev-only)                                 |
| byroredux-facegen        | (none — leaf; `byroredux-bsa` dev-only)                                 |
| byroredux-papyrus        | (none — leaf)                                                           |
| byroredux-cxx-bridge     | (none — leaf)                                                           |
| byroredux-debug-protocol | (none — leaf)                                                           |
| byroredux-platform       | byroredux-core                                                          |
| byroredux-nif            | byroredux-core                                                          |
| byroredux-spt            | byroredux-core, byroredux-nif                                           |
| byroredux-plugin         | byroredux-core                                                          |
| byroredux-physics        | byroredux-core                                                          |
| byroredux-audio          | byroredux-core                                                          |
| byroredux-scripting      | byroredux-core, byroredux-plugin                                        |
| byroredux-ui             | byroredux-core (+ vendored Ruffle/wgpu)                                 |
| byroredux-renderer       | byroredux-core, byroredux-platform                                      |
| byroredux-debug-server   | byroredux-core (`inspect`), byroredux-papyrus, byroredux-debug-protocol |
| byroredux-debug-ui       | byroredux-core, byroredux-renderer                                      |
| byroredux (binary)       | all engine crates above (debug-server is optional, on by default)       |
| byro-dbg (tool)          | byroredux-debug-protocol                                                |

`byroredux-spt` (SpeedTree) depends on `byroredux-nif` so its importer emits
the same `Imported*` scene types the cell loader spawns through one canonical
path. `byroredux-scripting` depends on `byroredux-plugin` for the
`Condition` / `ConditionList` types (M47.1) — no cycle, since plugin parses
ESM bytes and knows nothing about runtime state.

The integration tests in `crates/nif/tests/parse_real_nifs.rs` (and the
shared helper `crates/nif/tests/common/mod.rs`) add a **dev**-dependency on
`byroredux-bsa` so a single test binary can walk both BSA and BA2 archives
through a unified `MeshArchive` enum (`MeshArchive::Bsa` / `MeshArchive::Ba2`,
defined in `common/mod.rs`) without introducing a runtime dependency. Several
leaf parser crates (`bgsm`, `sfmaterial`, `facegen`, `spt`) do the same for
their corpus-parse regression tests.

## Dependency Philosophy

- **Minimal.** No frameworks. Single-purpose focused crates.
- **Workspace-managed.** Versions live in one place; no per-crate version
  drift. Adding a new crate means one entry in the root `Cargo.toml` and
  one `{ workspace = true }` reference. The handful of per-crate pins
  (Ruffle/wgpu/futures in `ui`, `tempfile` dev-dep in `byroredux`) are
  deliberate exceptions where the dependency is single-consumer.
- **Mostly no duplicates.** Each capability is generally covered by exactly
  one crate (one math library, one log facade, one allocator). The sole
  intentional split is `image`: `0.24` workspace-wide for the Vulkan renderer
  and golden-frame tests, `0.25` inside `byroredux-ui` to match Ruffle's
  expectations.
- **Leaf crates first.** `byroredux-core`, `byroredux-bsa`, `byroredux-bgsm`,
  `byroredux-sfmaterial`, `byroredux-facegen`, `byroredux-papyrus`,
  `byroredux-cxx-bridge`, and `byroredux-debug-protocol` have no engine
  dependencies. They're testable in isolation and would be the first
  candidates for extracting into separate workspaces if the project ever
  splits.
- **Parallelism is opt-in and structured.** `rayon` powers the M27 stage-based
  parallel system scheduler (`core`'s default `parallel-scheduler` feature)
  and the parallel draw-command sort, not free-floating `std::thread` fan-out.
- **Don't pull in async runtimes.** No `tokio` or `async-std`. The one
  `futures` dependency is confined to `byroredux-ui` (wgpu device creation
  for Ruffle); engine concurrency is rayon + Vulkan command-buffer
  parallelism + per-frame double-buffering, not an async runtime.

## Build-time deps

Beyond `cxx-build`, the renderer carries a `build.rs` that does two things:

1. **Shader↔Rust constant codegen** (#1038 / TD4-003…020) — generates the
   GLSL header `shaders/include/shader_constants.glsl` from the single source
   of truth `src/shader_constants_data.rs` (Rust → GLSL; the `.glsl` file is
   the only output written) so the `struct GpuInstance` / flag / material-kind
   values can't silently drift between GLSL and Rust.
2. The renderer's GLSL shaders are still pre-compiled to SPIR-V with
   `glslangValidator` and embedded with `include_bytes!`. The build script
   does **not** run `glslangValidator` automatically — recompiling shaders is
   a manual step (see [README](../../README.md) for the command). This keeps
   the build dependency surface small and avoids requiring the Vulkan SDK on
   `PATH` for crate consumers. SPIR-V reflection (`rspirv`) cross-checks the
   compiled shaders' descriptor layouts against the Rust declarations at
   runtime (#427).

## Build profile tuning

The root `Cargo.toml` overrides the default debug profile (Bevy / Macroquad
pattern, Phases 12 + 16):

```toml
[profile.dev]
opt-level = 1                      # whole-workspace inlining in debug

[profile.dev.package.rapier3d]   opt-level = 3
[profile.dev.package.parry3d]    opt-level = 3   # rapier transitive
[profile.dev.package.nalgebra]   opt-level = 3
[profile.dev.package.glam]       opt-level = 3
```

Diagnosis (debug-UI Metrics panel, Sleeping Giant Inn, 3211 ECS entities):
`physics_sync_system` measured **414 ms/frame** with the math crates at
`opt-level = 0`, and `build_render_data`'s pre-draw phase was pinned at a
deterministic ~47.4 ms. Lifting the math crates (and `glam`, which sits on
the hot transform path) to `opt-level = 3` while keeping the rest of the
workspace at `1` retains full debug symbols without regressing physics.
`--release` builds are unaffected.
