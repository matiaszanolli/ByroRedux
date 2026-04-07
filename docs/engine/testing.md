# Testing

ByroRedux uses two layers of tests:

1. **Unit tests** (`#[cfg(test)] mod tests` inside source files) — fast,
   no game data required, run on every `cargo test`. **372 passing**.
2. **Integration tests** (`#[ignore]`'d by default) — exercise real game
   archives, parse rates, and end-to-end byte-level round-trips. Need
   the relevant game installed and resolve paths via env vars or Steam
   defaults. Run with `cargo test ... -- --ignored`. **14 in total.**

The split keeps CI fast and game-data-free while letting developers run
the heavy sweeps locally on demand.

## Per-crate test counts

Numbers are accurate at the time of writing (M24 Phase 1, April 2026). For
a live count, run `cargo test 2>&1 | grep "test result"`.

| Crate | Unit tests | Ignored |
|---|---|---|
| `byroredux-core` | 152 | — |
| `byroredux-nif` | 118 | — |
| `byroredux-plugin` | 64 | 2 |
| `byroredux-platform` | 19 | — |
| `byroredux-scripting` | 8 | — |
| `byroredux-bsa` | 8 | 7 |
| `byroredux-renderer` | 2 (doc-tests) | — |
| `byroredux` (binary) | 1 | 2 |
| Integration: `parse_real_nifs.rs` | — | 8 |
| **Total** | **372** | **19** |

## Unit test coverage by area

### ECS — `byroredux-core`
- **Storage backends** (sparse set, packed) — insert / remove / iterate / overwrite, swap-remove invariants, sort-order maintenance
- **World basics** — spawn, multi-storage coexistence, get/get_mut, lazy storage init
- **Single-component queries** — read, write, register-without-insert
- **Multi-component queries** — read+write coexistence, lock ordering, deadlock detection on same-type pairs
- **Resources** — insert/read/mutate, type-name in panic messages, missing-resource handling, overwrite, scheduler visibility
- **Scheduler** — closures, struct systems, ordering, mutation propagation, system names
- **Names + StringPool** — attach, find_by_name, missing pool, missing components
- **Math + types** — `Vec3`/`Quat` round-trips, `Color`, `NiTransform` defaults
- **Form IDs** — pool allocation, plugin slot mapping, content-addressed identity
- **Animation engine** — clip registry, player advance, blending stack, root motion split, interpolation kernels (linear, Hermite, TBC)

### NIF — `byroredux-nif`
- **Header parser** — minimal Skyrim header, blocks + strings, NetImmerse pre-Gamebryo, BSStreamHeader for FO4/FO76, user_version threshold
- **Stream reader** — primitives, version-dependent string format, block refs, transforms
- **Block parsers** — every supported block type (NiNode, NiTriShape, BSTriShape variants, NiSkinPartition, BSLightingShaderProperty across 8 shader-type variants, BSEffectShaderProperty, particle systems, Havok skip and full-parse, FO76 CRC32 flag arrays, FO76 stopcond, FO76 luminance/translucency)
- **Animation import** — `NiTransformInterpolator`, `NiKeyframeData`, `NiTextKeyExtraData`, controller manager
- **Coordinate conversion** — Z-up→Y-up identity, 90° rotation around each axis, vertex positions, vertex normals, winding order preservation
- **Scene parsing** — empty file, minimal node, unknown block recovery, downcasting via `get_as`

### Plugin — `byroredux-plugin`
- **Manifest parsing** — valid TOML, invalid TOML, no-deps case
- **Records** — `RecordType` 4-char codes, ECS spawn integration, `find_by_form_id`, equality / hashing
- **DataStore + resolver** — depth resolution, three-way chains, transitive deps, deterministic tiebreak
- **Legacy ESM/ESP/ESL bridge** — slot-to-PluginId mapping, save-generated forms, reserved slots
- **ESM cell parser** — STAT extraction, REFR position/scale, group walking
- **ESM record parser (M24)** — WEAP / ARMO / MISC field extraction, CONT inventory, LVLI leveled entries, NPC race/class/factions/inventory/AI, FACT relations + ranks, GLOB/GMST typed values, `extract_records` group walker, total counters

### BSA / BA2 — `byroredux-bsa`
- **Path normalization** — case-insensitive, slash agnostic
- **Reject non-archive files** — both BSA and BA2
- **DDS header reconstruction** — 148-byte layout invariants, BC1/BC7 linear-size, unknown-format fallback

### Other crates
- **`byroredux-scripting`** — event marker round-trips, timer expiry, end-of-frame cleanup
- **`byroredux-platform`** — window creation, raw handle round-trip
- **`byroredux-renderer`** — doc-tests on the public API

## Integration tests (`#[ignore]`'d)

These tests require real game data on disk and are gated behind the
`#[ignore]` attribute so CI doesn't fail without it. They resolve game
paths from environment variables, falling back to canonical Steam install
paths on the reference development machine.

| Test                                                       | What it does                                                                |
|------------------------------------------------------------|-----------------------------------------------------------------------------|
| `parse_rate_oblivion`                                      | Walks `Oblivion - Meshes.bsa`, asserts ≥95% NIF parse success               |
| `parse_rate_fallout_3`                                     | `Fallout - Meshes.bsa` (FO3)                                                |
| `parse_rate_fallout_nv`                                    | `Fallout - Meshes.bsa` (FNV)                                                |
| `parse_rate_skyrim_se`                                     | `Skyrim - Meshes0.bsa`                                                      |
| `parse_rate_fallout_4`                                     | `Fallout4 - Meshes.ba2` (BA2 v8)                                            |
| `parse_rate_fallout_76`                                    | `SeventySix - Meshes.ba2` (BA2 v1)                                          |
| `parse_rate_starfield`                                     | `Starfield - Meshes01.ba2` (BA2 v2)                                         |
| `parse_rate_smoke_all_games`                               | First 50 NIFs from each available game                                      |
| `parse_real_fnv_esm` (cell side)                           | Loads `FalloutNV.esm`, asserts >100 cells, >1000 statics, Saloon refs       |
| `parse_real_fnv_esm_record_counts` (M24)                   | Asserts FNV item / NPC / faction / global counts                            |
| `byroredux-bsa` archive tests                              | FNV BSA open/list/contains/extract/decompress round-trips                   |
| `byroredux` binary doc tests / args parsing                | CLI help, env var override                                                  |

## Game data resolution

The integration test suite shares one helper module
[`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs)
that exposes a `Game` enum and a `MeshArchive` enum wrapping both BSA and
BA2 archives behind a single `list_files()` / `extract()` API. Each game
declares its env-var name (`BYROREDUX_FNV_DATA`, etc.) and a default Steam
path; the helper picks whichever resolves first and prints a skip notice
when neither does.

The same helper is used by the `parse_real_nifs.rs` integration test and
the `nif_stats` example binary in `crates/nif/examples/nif_stats.rs`.

## Running tests

```bash
# Default — fast, no game data required (~372 tests)
cargo test

# A single crate
cargo test -p byroredux-core
cargo test -p byroredux-nif
cargo test -p byroredux-plugin

# A single module
cargo test -p byroredux-core -- ecs::world

# Integration tests requiring game data
cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored
cargo test -p byroredux-plugin --release -- --ignored parse_real_fnv_esm
cargo test -p byroredux-bsa -- --ignored

# nif_stats CLI: walks one archive and reports parse rate + block histogram
cargo run -p byroredux-nif --example nif_stats --release -- \
    "/path/to/Fallout - Meshes.bsa"
```

## Test infrastructure milestones

- **N23.10** introduced the per-game integration test infrastructure plus
  graceful per-block parse recovery in the NIF top-level walker. This is
  what turned single-block parser bugs from NIF-killing errors into
  measurable telemetry.
- **M26+** added the `MeshArchive` enum and BA2 game entries (Fallout 4,
  Fallout 76, Starfield) on top of the existing BSA games.
- **M24 Phase 1** added the `parse_real_fnv_esm_record_counts` test that
  verifies the new structured record parser against real FNV.esm.

See [Game Compatibility](game-compatibility.md) for the per-game parse
rate matrix the integration tests produce.
