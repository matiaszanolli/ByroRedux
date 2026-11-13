# ByroRedux

Clean rebuild of the Gamebryo/Creation engine lineage in Rust + C++ with Vulkan.
Long-term goal: load and render content from Gamebryo/Creation-era games.

> `AGENTS.md` is a symlink to this file — edit here, never there (#5107).

## Quick Reference

```bash
cargo check                    # Type check (fast)
cargo test -p byroredux-core --features inspect   # ECS/core tests (the feature is load-bearing, below)
cargo test                     # Full workspace tests
cargo run                      # Launch engine (spinning cube demo)
cargo build --release          # Release build
```

- **`--features inspect` on core (#3895).** Without it the `AnimationPlayer` /
  `AnimationStack` serialization round-trips (`#[cfg(all(test, feature = "inspect"))]`)
  are silently dropped, and a bare `cargo test -p byroredux-core` is green without them.
- **Toolchain: rustc ≥ 1.96 (#4466).** `rust-toolchain.toml` pins 1.96.0 (wasmtime 49's
  MSRV). If an older distro cargo shadows the rustup shims, call the pinned one directly,
  and verify any `byroredux/src/` change this way before pushing
  ([docs/contributing.md](docs/contributing.md) §Tests):
  ```bash
  TC=$(rustup which --toolchain 1.96.0 cargo)
  PATH="$(dirname "$TC"):$PATH" "$TC" test -p byroredux --bin byroredux
  ```
- **Shaders.** SPIR-V is pre-compiled and `include_bytes!`'d: after a GLSL edit,
  `cd crates/renderer/shaders && glslangValidator -V <file> -o <file>.spv`.
- **Debug CLI.** `cargo run -p byro-dbg` attaches to a running engine on port 9876
  (`BYRO_DEBUG_PORT` overrides); see [docs/engine/debug-cli.md](docs/engine/debug-cli.md).
- **Smoke tests** need a Vulkan device and game data; the one index is
  [docs/smoke-tests/README.md](docs/smoke-tests/README.md) (#6166).
- **PRD gate.** Feature-sized work (a new milestone, layer, crate or host contract; a
  per-game promise change; ≥3 crates or ≥5 issues) needs an APPROVED PRD under
  `docs/prd/` before its first implementation issue: [docs/prd/README.md](docs/prd/README.md),
  `/prd new|review`, and `python3 scripts/check-prd-conformance.py` (CI `source-integrity`).

## Running the engine

```bash
cargo run -- path/to/mesh.nif --kf anim.kf                     # loose NIF (+ animation)
cargo run -- --esm FalloutNV.esm --cell <id> --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"
cargo run -- --esm FalloutNV.esm --grid 0,0 --radius 3 --bsa …  # exterior grid (radius 1..=7)
cargo run -- --master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 --bsa …
cargo run --release -- … --bench-frames 300 --bench-hold         # bench, then hold open for byro-dbg
```

- `--bsa` opens the literal path (vanilla FNV has no bare `Meshes.bsa`); `<stem>N.bsa`
  siblings load automatically.
- `byro-dbg` reaches only an engine held open by `--bench-hold`, and a release build also
  needs `BYRO_DEBUG_SERVER=1`. `screenshot` takes a bare filename and writes under the
  engine cwd's `screenshots/` (#5142).
- "Chrome / posterized" surfaces usually mean **missing textures**, not a lighting bug:
  run `tex.missing` first.

Full invocation set: [README.md § Run](README.md#run).

## Where things are

- `byroredux/` is the binary: game loop, systems, cell loading, render-data collection,
  console commands. `crates/` holds the libraries: `core` (ECS, math, animation),
  `renderer` (Vulkan), `nif`, `plugin` (ESM/ESP), `bsa` (BSA/BA2), `audio`, `scripting`,
  `physics`, `save`, `ui` and more. `tools/` holds `byro-dbg` and other CLIs.
- Designs live in `docs/engine/` ([index](docs/engine/index.md)), PRDs in `docs/prd/`,
  and the Gamebryo 2.3 analysis in `docs/legacy/` (the source path and key headers are in
  [key-files.md](docs/legacy/key-files.md)).
- The file-level map is [docs/engine/workspace-map.md](docs/engine/workspace-map.md). It
  is not loaded into context: read the part you need, and confirm a path with `ls` / `rg`
  before relying on it.
- State lives elsewhere; do not copy it here. [ROADMAP.md](ROADMAP.md): milestones, known
  issues, the per-game compatibility matrix, project stats. [HISTORY.md](HISTORY.md):
  session narratives. `git log`: the rest.

## Architecture Invariants

1. **ECS over scene graph.** Components are data, systems are logic. No God Objects.
2. **Components declare their storage.** `PackedStorage` for hot-path (Transform), `SparseSetStorage` for sparse data (Name, MeshHandle).
3. **Interior mutability via RwLock.** Query/resource access takes `&self`. Structural mutation takes `&mut self`.
4. **TypeId-sorted lock acquisition** in multi-component queries to prevent deadlocks.
5. **Vulkan done properly.** Validation layers in debug, per-image semaphores, dynamic viewport/scissor, clean reverse-order teardown.
6. **No shortcuts on Vulkan init.** Full chain: entry → instance → debug → surface → physical device → logical device → allocator → swapchain → render pass → pipeline → framebuffers → command pool → sync.

## Critical Patterns

### ECS Component Declaration
```rust
struct MyComponent { ... }
impl Component for MyComponent {
    type Storage = SparseSetStorage<Self>;  // or PackedStorage<Self>
}
```

### System Signature
Systems take `&World` (not `&mut self`). Mutation via QueryWrite/ResourceWrite:
```rust
fn my_system(world: &World, dt: f32) {
    let mut q = world.query_mut::<Transform>().unwrap();
    for (entity, transform) in q.iter_mut() { ... }
}
```

### Multi-Component Query (intersection)
```rust
let (q_read, mut q_write) = world.query_2_mut::<Velocity, Position>().unwrap();
for (entity, vel) in q_read.iter() {
    if let Some(pos) = q_write.get_mut(entity) { ... }
}
```

## Git Conventions

- Conventional commit messages (what + why, not how)
- No `Co-Authored-By` / AI co-author trailer — commit body only (per global instruction)
- Branch: `main` · Remote: `origin` → `github.com:matiaszanolli/ByroRedux.git`
