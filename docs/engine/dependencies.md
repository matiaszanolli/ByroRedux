# Dependencies

External dependencies live in the workspace root [`Cargo.toml`](../../Cargo.toml)
and are consumed by individual crates via `{ workspace = true }`. The internal
crate graph is documented in [Architecture Overview](architecture.md#crate-dependency-graph).

## External Dependencies

| Crate              | Version  | Used by                                      | Purpose                                       |
|--------------------|----------|----------------------------------------------|-----------------------------------------------|
| **Vulkan / GPU**   |          |                                              |                                               |
| ash                | 0.38     | renderer                                     | Raw Vulkan bindings                           |
| ash-window         | 0.13     | renderer                                     | Surface creation from window handles          |
| gpu-allocator      | 0.27     | renderer                                     | Vulkan memory allocation                      |
| **Windowing**      |          |                                              |                                               |
| winit              | 0.30     | platform, renderer, byroredux                | Cross-platform windowing                      |
| raw-window-handle  | 0.6      | platform, renderer                           | Platform-agnostic window handle traits        |
| **Math**           |          |                                              |                                               |
| glam               | 0.29     | core                                         | Linear algebra (Vec, Mat, Quat)               |
| nalgebra           | 0.33     | nif                                          | SVD for degenerate NIF rotation matrix repair |
| **Strings**        |          |                                              |                                               |
| string-interner    | 0.17     | core                                         | O(1) string equality via interning            |
| **Identity**       |          |                                              |                                               |
| uuid               | 1        | plugin                                       | Plugin identity (`v5` content hashing)        |
| semver             | 1        | plugin                                       | Plugin version constraints                    |
| **Serialization**  |          |                                              |                                               |
| serde              | 1        | plugin                                       | Manifest serialization                        |
| toml               | 0.8      | plugin                                       | Plugin manifest format                        |
| **Compression**    |          |                                              |                                               |
| flate2             | 1        | bsa, plugin                                  | Zlib decompression for BSA + BA2 + ESM records |
| lz4_flex           | 0.11     | bsa                                          | LZ4 frame decompression for BSA v105, LZ4 block for BA2 v3 |
| **Image**          |          |                                              |                                               |
| image              | 0.24     | renderer                                     | PNG / non-DDS image loading                   |
| **C++ interop**    |          |                                              |                                               |
| cxx                | 1        | cxx-bridge                                   | Type-safe C++ FFI                             |
| cxx-build          | 1 (build)| cxx-bridge build script                      | C++ compilation for the cxx bridge            |
| **Logging**        |          |                                              |                                               |
| log                | 0.4      | all                                          | Logging facade                                |
| env_logger         | 0.11     | byroredux, examples, integration tests       | Stderr log output                             |
| **Errors**         |          |                                              |                                               |
| anyhow             | 1        | plugin, byroredux                            | Context-rich error handling                   |
| thiserror          | 2        | core, renderer, bsa                          | Error type derives                            |

## Internal Crate Dependencies

| Crate            | Depends on                                                  |
|------------------|-------------------------------------------------------------|
| byroredux-core   | (none — leaf)                                               |
| byroredux-platform | byroredux-core                                            |
| byroredux-renderer | byroredux-core, byroredux-platform                        |
| byroredux-plugin | byroredux-core                                              |
| byroredux-nif    | byroredux-core                                              |
| byroredux-bsa    | (none — leaf)                                               |
| byroredux-ui     | byroredux-core                                              |
| byroredux-scripting | byroredux-core                                           |
| byroredux-cxx-bridge | (none — leaf)                                           |
| byroredux (binary) | all of the above                                          |

The integration tests in `crates/nif/tests/parse_real_nifs.rs` add a
**dev**-dependency on `byroredux-bsa` so a single test binary can walk both
BSA and BA2 archives through a unified `MeshArchive` enum without
introducing a runtime dependency.

## Dependency Philosophy

- **Minimal.** No frameworks. Single-purpose focused crates.
- **Workspace-managed.** All versions in one place; no per-crate version
  drift. Adding a new crate means one entry in the root `Cargo.toml` and
  one `{ workspace = true }` reference.
- **No duplicates.** Each capability is covered by exactly one crate
  (one math library, one image loader, one log facade, one allocator, etc.).
- **Leaf crates first.** `byroredux-core`, `byroredux-bsa`, and
  `byroredux-cxx-bridge` have no engine dependencies. They're testable in
  isolation and would be the first candidates for extracting into separate
  workspaces if the project ever splits.
- **Don't pull in async runtimes.** No `tokio`, `async-std`, or `futures`.
  The engine is single-threaded today (M27 brings Rayon for parallel system
  dispatch); concurrency is via Vulkan command buffer parallelism and
  per-frame double-buffering, not async.

## Build-time deps

Beyond `cxx-build`, the renderer's GLSL shaders are pre-compiled to SPIR-V
with `glslangValidator` and embedded with `include_bytes!`. The build
script doesn't run `glslangValidator` automatically — recompiling shaders
is a manual step (see [README](../../README.md) for the command). This
keeps the build dependency surface small and avoids requiring the Vulkan
SDK to be on `PATH` for crate consumers.
