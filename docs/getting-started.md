# Getting Started

This guide is the shortest path from a fresh checkout to a useful ByroRedux
session. Choose the outcome you want:

- **Explore the project without Bethesda game files:** build the workspace and
  run its unit tests.
- **Run original game content:** point the engine at files from a game you own.
- **Contribute code:** finish this guide, then continue with
  [Contributing](contributing.md).

ByroRedux is an engine under active development, not a packaged replacement
for the original games. It does not include Bethesda assets, plugins, or
scripts, and it does not currently provide a launcher or installer.

## 1. Get the source

```bash
git clone https://github.com/matiaszanolli/ByroRedux.git
cd ByroRedux
```

If you already have a checkout, run the commands below from its repository
root—the directory containing the workspace `Cargo.toml`.

## 2. Check the platform

The supported development platform is Linux. You need:

| Requirement | Needed for |
|---|---|
| Rust stable | Building and testing |
| A C++17 compiler | The `cxx` bridge |
| Vulkan development libraries | Building the renderer |
| Vulkan 1.3 plus `VK_KHR_ray_query` | Running the renderer |
| Legally obtained game data | Loading Bethesda cells and assets |

On Ubuntu 24.04, the minimum system packages are:

```bash
sudo apt-get install g++ libvulkan-dev
rustup toolchain install stable
```

The full optional tool list and distro notes are in
[Contributing → Prerequisites](contributing.md#prerequisites).

## 3. Build and test

From the repository root:

```bash
cargo check --workspace
cargo test --workspace
```

Neither command needs a GPU or game data. The first build downloads and
compiles a large dependency graph, so it will be much slower than later runs.
Current test totals belong in [ROADMAP.md](../ROADMAP.md#project-stats); this
guide deliberately does not copy that frequently changing number.

If both commands succeed, your development setup is ready. You can work on
parsers, ECS, scripting, archive formats, and most renderer data structures
without installing a Bethesda game.

## 4. Choose a first engine run

### Option A: load a loose NIF

If you have an extracted `.nif` file, this is the smallest content-loading
path:

```bash
cargo run --release -- /absolute/path/to/mesh.nif
```

Add `--kf /absolute/path/to/animation.kf` to play a compatible loose
animation. Textures referenced by the mesh must also be resolvable from the
content you provide; missing textures use diagnostic fallbacks.

### Option B: load an interior cell

Fallout: New Vegas is the simplest documented reference scene. Run from the
game's `Data/` directory because bare archive names resolve against the
current working directory:

```bash
cd "/path/to/Fallout New Vegas/Data"
cargo run --manifest-path /path/to/gamebyro-redux/Cargo.toml --release -- \
  --esm FalloutNV.esm \
  --cell GSProspectorSaloonInterior \
  --bsa "Fallout - Meshes.bsa" \
  --textures-bsa "Fallout - Textures.bsa"
```

The unsuffixed texture archive automatically discovers numbered siblings such
as `Fallout - Textures2.bsa` when they are present beside it. Skyrim's
zero-based series (`Textures0`, `Textures1`, and so on) is also discovered
from `Textures0`; you do not need to list every sibling individually.

Equivalent, benchmarked commands for Oblivion, Fallout 3, Skyrim SE, Fallout
4, and Starfield live in
[ROADMAP → Repro commands](../ROADMAP.md#repro-commands-for-every-bench-claim).
Those commands are the source of truth when a short example here differs from
a benchmark setup.

### What success looks like

An engine window opens and the selected mesh or cell begins rendering. Cell
loads can take time in a debug build, so use `--release` for real game data.
Missing optional assets are generally reported and skipped instead of
aborting the entire load.

Controls:

- `W`, `A`, `S`, `D` and mouse: move and look
- `F`: toggle walk and fly modes
- `Space`: jump in walk mode; move up in fly mode
- `Shift`: move down in fly mode
- `Ctrl`: move faster
- `Escape`: release or capture the mouse

## 5. Inspect a running engine

Start the scene with `--bench-hold` if you want it to remain open after a
fixed-frame run:

```bash
cargo run --release -- \
  --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
  --bsa "Fallout - Meshes.bsa" \
  --textures-bsa "Fallout - Textures.bsa" \
  --bench-frames 300 --bench-hold
```

In a second terminal, from the repository:

```bash
cargo run -p byro-dbg
```

Useful first commands are `help`, `stats`, `entities`, `tex.missing`,
`tex.loaded`, and `sys.accesses`. See the [Debug CLI guide](engine/debug-cli.md)
for queries, screenshots, entity inspection, and custom ports.

## Troubleshooting

### The engine reports that zero archives opened

Bare `--bsa` and `--textures-bsa` values are resolved from the process's
current directory, not automatically from the `--esm` path. Either run from
the game's `Data/` directory or pass absolute paths.

### The scene is empty or mostly empty

Check the startup log for archive-open failures, then run `tex.missing` from
`byro-dbg`. An ESM provides placements, but mesh and texture archives provide
the visible assets.

### Vulkan initialization fails

Confirm that `vulkaninfo` sees the intended GPU and that it exposes Vulkan 1.3
and `VK_KHR_ray_query`. Mesa lavapipe is useful for CI validation, but normal
interactive rendering expects a capable GPU and driver.

### A debug build is extremely slow

Use `cargo run --release` for full cells and corpus tests. Debug builds are
best for unit tests, validation, and small fixtures.

### An integration test says game data is unavailable

Set the appropriate data-directory variable, such as
`BYROREDUX_FNV_DATA=/path/to/Fallout New Vegas/Data`. The complete variable
table is in [Contributing → Game Data Paths](contributing.md#game-data-paths).

## Where to go next

| Goal | Read next |
|---|---|
| Make a contribution | [Contributing](contributing.md) |
| Understand one cell load end-to-end | [Pipeline Overview](engine/pipeline-overview.md) |
| Learn the architecture | [Engine Documentation](engine/index.md) |
| See current capabilities and gaps | [Feature Matrix](feature-matrix.md) |
| See priorities and live project statistics | [Roadmap](../ROADMAP.md) |
| Understand how the project evolved | [History](../HISTORY.md) |
