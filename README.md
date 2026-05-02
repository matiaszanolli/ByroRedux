# ByroRedux

A clean Rust + Vulkan rebuild of the Gamebryo / Creation engine lineage
(Oblivion → Starfield). Linux-first. Not a port — a ground-up rebuild
that understands the legacy architecture and builds modern equivalents.

![Anvil Heinrich Oaken Halls (Oblivion)](docs/screenshots/anvil-oaken-halls.png)
*Anvil Heinrich Oaken Halls loaded directly from `Oblivion.esm` + meshes + textures BSAs — RT multi-light with ray-query shadows.*

![Prospector Saloon (Fallout: New Vegas)](docs/screenshots/prospector-saloon.png)
*Prospector Saloon (Goodsprings) from `FalloutNV.esm` — 1 200 entities,
streaming RIS shadows, 172.6 FPS / 5.79 ms on RTX 4070 Ti (commit 6a6950a,
wall-clock `--bench-frames 300`; see [ROADMAP Project Stats](ROADMAP.md#project-stats)).*

## At a glance

| | |
|-|-|
| **Games supported** | 7 — Oblivion · Fallout 3 · Fallout New Vegas · Skyrim SE · Fallout 4 · Fallout 76 · Starfield |
| **NIF parse rate** | **100% clean** on FO3 / FNV / Skyrim SE; 95–99% clean / 100% recoverable on Oblivion / FO4 / FO76 / Starfield — 184 886 files validated. See [ROADMAP compatibility matrix](ROADMAP.md#compatibility-matrix). |
| **Archive formats** | BSA v103 / v104 / v105 · BA2 v1 / v2 / v3 / v7 / v8 (GNRL + DX10, zlib + LZ4) |
| **NIF block types** | 291 dispatch arms (~38 Havok) — see `crates/nif/src/blocks/mod.rs` |
| **ESM records (FNV)** | 62 219 structured records — items, NPCs, factions, cells, CREA, LVLC, SCPT, PACK, QUST, DIAL, MESG, PERK, SPEL, MGEF, … |
| **Tests passing** | 1 456 across 16 workspace crates |
| **Source code** | ~121 K lines of Rust across 264 source files |
| **Renderer** | Vulkan 1.3 + `VK_KHR_ray_query` — multi-light RT shadows, reflections, 1-bounce GI, SVGF temporal denoiser, TAA, streaming RIS (8 reservoirs/fragment), BLAS compaction + LRU eviction |
| **Physics** | Rapier3D — collision import from NIF `bhk` chain, dynamic bodies, fixed 60 Hz substep |
| **Scripting** | Papyrus `.psc` lexer + Pratt expression parser + full AST; ECS-native event + timer runtime |
| **UI** | Scaleform / SWF menus via Ruffle (offscreen wgpu → Vulkan texture overlay) |

## Highlights

- **Full RT lighting pipeline** — ray-query shadows with streaming weighted
  reservoir sampling (8 reservoirs / fragment, unbiased weight clamped at
  64×), RT reflections with roughness-driven jitter, 1-bounce GI with
  cosine-weighted hemisphere sampling, SVGF temporal denoiser with
  motion-vector reprojection and mesh-id disocclusion, TAA with Halton(2,3)
  jitter and YCoCg variance clamp, ACES tone mapping.
- **100% parse coverage** across all seven supported Bethesda titles —
  100% clean on FO3 / FNV / Skyrim SE and 95–99% clean / 100% recoverable
  on Oblivion / FO4 / FO76 / Starfield (184 886 NIFs validated). CI fails
  on regression (per-game per-block-type baselines).
- **Full asset round-trip** from unmodified Bethesda game data —
  `Oblivion.esm` + BSA → rendered interior with XCLL lighting +
  per-mesh NiLight torches + RT shadows, no loose files required.
- **BLAS lifecycle done right** — batched builds (single GPU submission
  per cell load), `ALLOW_COMPACTION` + query-based compact copy (20–50%
  memory reduction), LRU eviction with VRAM/3 budget, TLAS frustum
  culling, TLAS refit when layout is unchanged.
- **Pipeline cache threaded through every create site** with disk
  persistence — 10–50 ms cold shader compile → <1 ms warm.
  SPIR-V reflection cross-checks every descriptor-set layout against
  shader declarations at pipeline-create time.
- **Debug CLI** (`byro-dbg`) with live ECS inspection over TCP, Papyrus
  expression query language (`42.Transform.translation.x`,
  `find("TorchSconce01")`, `entities(LightSource)`), screenshot
  capture. Zero per-frame cost when no debugger is connected.
- **Clean-room legacy reference** — parses `nif.xml` (niftools
  authoritative spec) and the Gamebryo 2.3 source tree for byte-exact
  serialization. No proprietary bits linked — just data understood.

## State

Interior cells load and render end-to-end across five games — Oblivion
(Anvil Heinrich Oaken Halls), FO3 (Megaton, 929 REFRs), FNV (Prospector
Saloon @ 172.6 FPS), Skyrim SE (Whiterun Bannered Mare, 1932 entities @
253.3 FPS), FO4 (MedTekResearch01, 7434 entities @ 92.5 FPS). Full RT
pipeline + sky/atmosphere + exterior sun operational. Skinning chain
verified end-to-end (M29 closed); GPU palette dispatch deferred to
M29.5 until M41 produces measurable load. World streaming Phase 1
shipped (single-cell async pre-parse); multi-cell grid pending. NPC
spawning (M41) gates the visible-actor work — every NPC today is in
bind pose because no actors are spawned yet. Oblivion exterior gated
on TES4 worldspace + LAND wiring. See **[ROADMAP.md](ROADMAP.md)**
for the authoritative capability matrix, active milestones, and
architecture decisions. Session narratives live in
**[HISTORY.md](HISTORY.md)**.

## Run

```bash
# FNV interior with full lighting (Textures2.bsa picked up automatically — see note below)
cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa"

# Oblivion interior
cargo run --release -- --esm Oblivion.esm --cell AnvilHeinrichOakenHallsHouse \
             --bsa "Oblivion - Meshes.bsa" \
             --textures-bsa "Oblivion - Textures - Compressed.bsa"

# Skyrim SE mesh + textures (Meshes0/1 are already numeric — list each
# explicitly; Textures0…4 likewise if more than one is needed)
cargo run -- --bsa "Skyrim - Meshes0.bsa" \
             --mesh "meshes\clutter\ingredients\sweetroll01.nif" \
             --textures-bsa "Skyrim - Textures3.bsa"

# Loose NIF + optional animation
cargo run -- path/to/mesh.nif [--kf path/to/anim.kf]

# Per-game NIF parse-rate sweep (requires game data)
cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored

# Debug CLI — connect to a running engine (TCP, port 9876)
cargo run -p byro-dbg
```

**Controls**: Escape captures mouse, WASD + mouse flies, Space/Shift
raise/lower, Ctrl for speed boost.

**Sibling archive auto-load.** When `--bsa` / `--textures-bsa` points
at an unsuffixed `.bsa` / `.ba2` (e.g. `Fallout - Textures.bsa`), the
loader also opens `<stem>2.bsa` … `<stem>9.bsa` next to it on disk.
That covers FNV/FO3's split textures (`Textures.bsa` +
`Textures2.bsa`) without a second flag. Skyrim's already-numeric
`Skyrim - Meshes0.bsa` / `Meshes1.bsa` is inert under this rule —
list each archive explicitly.

**Diagnostics.** `BYROREDUX_RENDER_DEBUG` enables fragment-shader
bypass / viz bits for ad-hoc bisection: `0x4` outputs world-space
normal, `0x8` colors fragments by tangent presence (green = authored
or synthesized tangent reaches Path 1, red = screen-space derivative
fallback), `0x10` skips normal-map perturbation entirely. See
[docs/engine/debug-cli.md](docs/engine/debug-cli.md) for the full bit
catalog and the `tex.missing` triage flow that closed the
"chrome walls" diagnosis.

## Build

- Rust stable (2021 edition)
- Vulkan SDK or drivers with validation layers
- `glslangValidator` for shader compilation
- C++17 compiler (for the cxx bridge)
- Linux (primary target)

## Per-game data paths

Integration tests resolve game data via environment variables, falling
back to canonical Steam install paths:

```
BYROREDUX_OBLIVION_DATA   .../Oblivion/Data
BYROREDUX_FO3_DATA        .../Fallout 3 goty/Data
BYROREDUX_FNV_DATA        .../Fallout New Vegas/Data
BYROREDUX_SKYRIMSE_DATA   .../Skyrim Special Edition/Data
BYROREDUX_FO4_DATA        .../Fallout 4/Data
BYROREDUX_FO76_DATA       .../Fallout76/Data
BYROREDUX_STARFIELD_DATA  .../Starfield/Data
```

## Documentation

- [ROADMAP.md](ROADMAP.md) — current state, active milestones, architecture decisions
- [HISTORY.md](HISTORY.md) — session narratives (2026-04 audit closeouts, etc.)
- [docs/engine/](docs/engine/) — architecture, renderer, NIF parser, ECS, physics, debug CLI
- [docs/legacy/](docs/legacy/) — Gamebryo 2.3 architecture reference, Papyrus API, Creation Engine UI

## Acknowledgements

- [**nifxml**](https://github.com/niftools/nifxml) — the NifTools project's
  machine-readable NIF format specification. ByroRedux's NIF parser is
  written directly against nifxml's block definitions, version gates, and
  field conditions. Without that community reverse-engineering effort,
  supporting seven Gamebryo/Creation-era games would not be tractable.
- [**Ruffle**](https://ruffle.rs) — the open-source Flash Player emulator.
  ByroRedux's UI layer embeds Ruffle to render the Scaleform/SWF menus
  Bethesda shipped with every Creation Engine title.
- [**OpenMW**](https://gitlab.com/OpenMW/openmw) — the open-source
  reimplementation of Morrowind that runs the full legacy-Gamebryo
  pipeline (Morrowind / Oblivion / FO3 / FNV / Skyrim LE) correctly.
  ByroRedux's understanding of the legacy `NiSkinData` skinning
  convention — specifically the role of `NiSkinData::mTransform` (the
  global skin transform) which NifSkope's partition path silently
  drops — comes from reading OpenMW's NIF skinning evaluator at
  `components/sceneutil/riggeometry.cpp` and the loader at
  `components/nifosg/nifloader.cpp`. OpenMW is GPLv3; we use it as a
  reference only — no code is copied. See M41.0 Phase 1b.x research
  in [byroredux/tests/skinning_e2e.rs](byroredux/tests/skinning_e2e.rs)
  for the specific findings.

## License

MIT
