# Contributing

How to build, test, and run the engine. No game data is required for
the unit tests and CI checks; real game files are only needed for
integration tests and the Vulkan renderer.

---

## Prerequisites

### Required

| Tool | Minimum | Notes |
|---|---|---|
| **Rust** | stable (2021 edition) | `rustup toolchain install stable` |
| **Vulkan driver** | 1.3 with `VK_KHR_ray_query` | Any RTX 20xx+ / RX 6000+ / Intel Arc GPU works |
| **C++17 compiler** | GCC 10+ / Clang 14+ | For the `cxx` bridge (`crates/cxx-bridge`) |
| **Linux** | — | Primary target. Windows/macOS untested. |

### Optional

| Tool | For what |
|---|---|
| `glslangValidator` | Recompiling GLSL shaders to SPIR-V (pre-compiled binaries are committed) |
| Mesa lavapipe | CI / headless Vulkan validation without a GPU (`mesa-vulkan-drivers` package) |
| `vulkan-validationlayers` | Debug builds; enabled automatically in debug mode |
| Game data | Integration tests and the renderer (see [Game data paths](#game-data-paths)) |

### Install on Ubuntu 24.04

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install stable

# Vulkan dev headers (for gpu-allocator / ash)
sudo apt-get install libvulkan-dev

# C++ compiler
sudo apt-get install g++

# Optional: shader recompile
sudo apt-get install glslang-tools

# Optional: CI / headless Vulkan
sudo apt-get install mesa-vulkan-drivers vulkan-validationlayers xvfb
```

---

## Build

```bash
# Type-check only (fastest feedback)
cargo check --workspace

# Build debug binary
cargo build -p byroredux

# Build release binary
cargo build --release -p byroredux

# Build everything (all crates)
cargo build --workspace
```

The `target/` directory grows large (~5 GB with all crates). Use
`cargo clean` if disk space is tight.

---

## Tests

### Unit tests (no GPU, no game data — run in CI)

```bash
# Full workspace (2 752 tests, ~30 s)
cargo test --workspace

# Single crate
cargo test -p byroredux-core
cargo test -p byroredux-nif
cargo test -p byroredux-plugin

# With the ABBA lock-order detector active (also runs in CI)
BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace
```

No Vulkan device or game files required. `cargo test` is the daily
development loop.

### Integration tests (require game data, `#[ignore]`d by default)

These parse real BSA/BA2 archives and NIF files. They are excluded from
`cargo test` and must be opted in explicitly:

```bash
# NIF parse-rate sweep (validates against per-game per-block-type baselines)
cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored

# BSA extraction
cargo test -p byroredux-bsa -- --ignored

# Per-game ESM structured-record counts
cargo test -p byroredux-plugin -- --ignored

# Regenerate NIF baselines after intentional parser changes
BYROREDUX_REGEN_BASELINES=1 cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored
```

### Vulkan validation test (requires Mesa lavapipe, runs in CI)

```bash
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json
export VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation
export RUST_LOG=error
xvfb-run --auto-servernum \
  cargo run -p byroredux -- --bench-frames 5
```

Fails if the Vulkan debug-messenger callback fires any `ERROR`-severity
message (printed as `[Vulkan] ...`).

### Smoke tests (require game data + a real GPU)

Manual end-to-end checks that can't run in CI. See
[`docs/smoke-tests/README.md`](smoke-tests/README.md). Currently:
- `m41-equip.sh` — FNV/Skyrim NPC equip pipeline
- `m-trees.sh` — SpeedTree billboard fallback

Pattern: launch engine with `--bench-frames N --bench-hold`, attach
`byro-dbg`, assert on console output.

---

## Running the Engine

The engine requires a Vulkan-capable display. Pre-compiled SPIR-V
shaders are embedded; no shader compilation step is needed unless you
edit GLSL.

### Single mesh

```bash
cargo run -- path/to/mesh.nif
cargo run -- mesh.nif --kf anim.kf
```

### Interior cell (game data required)

Run from each game's `Data/` directory — bare `--bsa` names resolve
against the current working directory, not the `--esm` location.

```bash
# Fallout New Vegas
cd ".../Fallout New Vegas/Data"
cargo run --release -- \
  --esm FalloutNV.esm \
  --cell GSProspectorSaloonInterior \
  --bsa "Fallout - Meshes.bsa" \
  --textures-bsa "Fallout - Textures.bsa" \
  --textures-bsa "Fallout - Textures2.bsa"

# Skyrim SE (list all 9 texture archives — numeric-sibling auto-load
# does not trigger for the "Textures0" suffix)
cd ".../Skyrim Special Edition/Data"
cargo run --release -- \
  --esm Skyrim.esm \
  --cell WhiterunBanneredMare \
  --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa" \
  --textures-bsa "Skyrim - Textures0.bsa" \
  --textures-bsa "Skyrim - Textures1.bsa" \
  # … through Textures8.bsa
```

For the full set of repro commands (including FO3, FO4, Oblivion,
Starfield), see [ROADMAP — Repro commands](../ROADMAP.md#repro-commands-for-every-bench-claim).

### Benchmarking + debug attach

```bash
# Run 300 frames, then hold open so byro-dbg can attach
cargo run --release -- \
  --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
  --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" \
  --bench-frames 300 --bench-hold

# In a second terminal
cargo run -p byro-dbg
# Commands: entities, tex.missing, tex.loaded, stats, screenshot, sys.accesses
```

---

## Recompiling Shaders

SPIR-V binaries in `crates/renderer/shaders/*.spv` are committed.
They are **not** recompiled automatically by `cargo build`. After
editing any `.vert`, `.frag`, or `.comp` file:

```bash
cd crates/renderer/shaders
glslangValidator -V <file>.vert -o <file>.vert.spv
glslangValidator -V <file>.frag -o <file>.frag.spv
glslangValidator -V <file>.comp -o <file>.comp.spv
```

Then commit both the GLSL and the `.spv`. **Mismatched SPIR-V is a HIGH
bug** (see #1447) — CI validates layout consistency but does not recompile.

---

## Game Data Paths

Integration tests and the renderer resolve game archives via environment
variables, falling back to canonical Steam install paths on Linux:

| Variable | Default fallback |
|---|---|
| `BYROREDUX_OBLIVION_DATA` | `~/.local/share/Steam/…/Oblivion/Data` |
| `BYROREDUX_FO3_DATA` | `~/.local/share/Steam/…/Fallout 3 goty/Data` |
| `BYROREDUX_FNV_DATA` | `~/.local/share/Steam/…/Fallout New Vegas/Data` |
| `BYROREDUX_SKYRIMSE_DATA` | `~/.local/share/Steam/…/Skyrim Special Edition/Data` |
| `BYROREDUX_FO4_DATA` | `~/.local/share/Steam/…/Fallout 4/Data` |
| `BYROREDUX_FO76_DATA` | `~/.local/share/Steam/…/Fallout76/Data` |
| `BYROREDUX_STARFIELD_DATA` | `~/.local/share/Steam/…/Starfield/Data` |

Setting the variable makes the corresponding `#[ignore]`d integration
tests runnable. The renderer also uses these paths when `--bsa` names
are relative (CWD must still be the game's `Data/` directory for the
engine binary itself).

---

## CI

Three jobs run on every push / PR:

| Job | What it checks | GPU? |
|---|---|---|
| `cargo-test` | `cargo check`, `cargo test --workspace`, `cargo clippy -D warnings` | No |
| `lock-order-check` | Same tests with `BYRO_LOCK_ORDER_CHECK=1` (ABBA deadlock detector) | No |
| `vulkan-validation` | 5-frame headless bench through lavapipe + `VK_LAYER_KHRONOS_validation` | No (lavapipe) |

CI passes if: all unit tests pass, no clippy warnings, no ABBA cycles
detected, and no Vulkan `ERROR`-severity validation messages fire.

The integration tests (`--ignored`) are not in CI — they require game
data files that cannot be redistributed.

---

## Conventions

- **Commit messages**: Conventional Commits (`feat(nif): …`, `fix(renderer): …`, `docs: …`). No `Co-Authored-By` trailers.
- **Tests**: every new parser feature needs at least one unit test; every bug fix needs a regression test. Integration tests (`#[ignore]`) for real-data validation.
- **No unsafe without a SAFETY comment** explaining the invariant that makes it sound.
- **No per-game branches past the NIFAL / EXAL translation boundary** — new game quirks belong in the `Imported*` → `Canonical` translate step, not in the renderer or gameplay code.
- **Session-close ritual**: run `/session-close` before ending a working session to sync ROADMAP / HISTORY / README / docs/.

---

## See Also

- [Architecture Overview](engine/architecture.md) — workspace layout, design principles
- [ECS](engine/ecs.md) — how to add components, systems, and resources
- [NIF Parser](engine/nif-parser.md) — how to add a new block type
- [Testing](engine/testing.md) — test inventory and how to add tests
- [Debug CLI](engine/debug-cli.md) — live ECS inspection while the engine runs
- [ROADMAP.md](../ROADMAP.md) — active milestones and known issues
