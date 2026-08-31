# ByroRedux Launcher — public-facing front end

**Status**: PROPOSED (2026-08-30). No code written.

The **launcher** is the first thing a non-developer sees. Today the engine's
only entry point is a ~60-flag argv surface driven from a terminal, backed by a
profile file whose default games root is hard-coded to one developer's
`/mnt/data/SteamLibrary` path. Everything a launcher needs to *decide* already
exists as data; what does not exist is a way for someone who is not us to reach
it.

This document is the development plan for closing that gap.

**Goal**: a person who owns Skyrim SE, has never opened a terminal, and has
never heard of a BSA can download ByroRedux, double-click it, and be walking
around Whiterun — with a clear, honest account of what works, what does not, and
what their machine can run.

**Non-goal**: replacing the argv surface. Development flags
(`--cornell-oracle`, `--bench-mode`, `--sf-smoke`, …) stay exactly as they are
and are never surfaced in the UI. The launcher targets *intent*; argv stays the
power/dev path. See §2.

---

## 0. The architectural decision: a separate process

**Decision: `byro-launcher` is a separate binary, rendering through `eframe`
on the `glow` (OpenGL) backend, that hands the engine a `BootRequest` file.**

Three independent arguments, in descending order of force.

### 0.1 The launcher must work when the engine does not

This is decisive on its own. The existing egui overlay
([`crates/debug-ui/`](../../crates/debug-ui/)) draws through
`egui-ash-renderer` **inside the Vulkan swapchain**, sequenced in `draw_frame`
right after the composite pass. It structurally cannot render a single pixel
until entry → instance → surface → physical device → logical device → allocator
→ swapchain → render pass → pipeline have all succeeded.

That makes it unusable as the screen where a user fixes a broken GPU
configuration, because the class of user who needs that screen is exactly the
class for whom device creation failed. An in-engine launcher would present, on
an unsupported GPU, as a process that exits to a terminal the user never opened.

The `glow` backend also matters here and not only for dependency hygiene: a
launcher on OpenGL 3.3 opens on machines where our Vulkan 1.3 + ray-query
requirement does not hold, which is precisely where it must open to say *why*.

### 0.2 Dependency isolation

`wgpu 27` is already in the tree, pulled by `ruffle_render_wgpu`
([`crates/ui/Cargo.toml:17`](../../crates/ui/Cargo.toml)). The workspace pins
`egui 0.33` because that is the newest series `egui-ash-renderer 0.11` admits
without a second `egui` in the tree
([`Cargo.toml:206-214`](../../Cargo.toml)). `eframe 0.33` on the wgpu backend
would pull a *third* GPU stack at a different wgpu major. `eframe 0.33` on
`glow` shares the `egui 0.33` pin, adds no wgpu, and links no Vulkan.

### 0.3 It matches what the audience already knows

Every game in the target lineage ships a launcher window with a Play button.
Meeting that expectation is free UX.

### 0.4 Refuted alternatives (recorded so they are not re-litigated)

| Alternative | Why rejected |
|---|---|
| In-engine pre-window state, reusing the egui overlay | Cannot render before device init (§0.1). Fatal. |
| A second `winit` window opened before `VulkanContext::new` | Solves ordering but not the *failure* case — a device-creation panic still kills the launcher window, and we would own two event loops in one process. |
| Extend the existing `--menu` Scaleform route into a main menu | Ruffle needs a wgpu device, so it inherits §0.1. `--menu` remains what it is: a *content* route for rendering game SWF menus. |
| GUI that composes an argv string | The flag surface is developer-shaped and changes constantly; the launcher would need a new widget every time a bench flag lands. §2 replaces this. |
| Web/Electron shell | A second runtime and an install-size regression for a native game. |

**Cost accepted**: a process boundary, and therefore a handoff contract that
must be designed rather than assumed. §2 is that contract.

---

## 1. Inventory — what already exists

The launcher is substantially a second, friendlier skin over two registries
that are already built, tested, and persisted. This section is the honest
accounting so the plan does not re-implement any of it.

| Piece | Location | State |
|---|---|---|
| **Game profile registry** | [`assets/debug_profiles.toml`](../../assets/debug_profiles.toml), [`byroredux/src/game_profiles.rs`](../../byroredux/src/game_profiles.rs), `GameProfileEntry` in [`crates/core/src/ecs/game_profiles.rs`](../../crates/core/src/ecs/game_profiles.rs) | **Done.** 6 profiles (`fnv`, `fo3`, `oblivion`, `skyrim_se`, `fo4`, `starfield`), each with `root`/`subdir`, `esm`, five archive categories (`default_bsas`, `_textures_`, `_scripts_`, `_sounds_`, `_materials_`), `new_game_worldspace`/`_grid`/`_radius`, and `sample_cells`. Per-user override at `~/.byroredux/profiles.toml` already layers over the shipped file. |
| **Profile → argv expander** | [`byroredux/src/boot.rs:1618`](../../byroredux/src/boot.rs) `expand_game_profile_args` | **Done.** `--game skyrim_se --new-game` already fans out to the full flag vector, with a `[defaults]` table that boots straight into a game/cell. |
| **Settings model** | [`crates/core/src/settings.rs`](../../crates/core/src/settings.rs) | **Done, and built for this.** `SettingEntry { id, section, label, description, value, default, control, restart_required }`; `SettingControl` is `Toggle` / `Slider{min,max,step,unit}` / `Choice{options}`; `SettingsRegistry` validates on `register` and `set`. Its module doc already states it exists so that "native game menus later" need not depend on a menu implementation. |
| **Settings persistence** | [`byroredux/src/settings_io.rs`](../../byroredux/src/settings_io.rs) | **Done.** Versioned TOML (`SETTINGS_VERSION = 1`), `(id, value)` pairs overlaid onto freshly-registered defaults, unknown/stale keys skipped individually. Path overridable via `BYROREDUX_SETTINGS_PATH`. Atomic write shared with the save container (#3472). |
| **Settings applied pre-device** | [`byroredux/src/main.rs:520-545`](../../byroredux/src/main.rs) | **Done, and load-bearing for §4.** The registry is loaded *before* `VulkanContext` is created, and the persisted `render.upscaler` choice already drives renderer config at that point specifically to avoid a first-frame upscaler rebuild. A launcher that writes the settings file therefore already steers device setup with zero engine changes. |
| **Panel harness** | [`crates/debug-ui/src/panels.rs`](../../crates/debug-ui/src/panels.rs) | **Reusable pattern, not reusable code.** Snapshot-in (`PanelSnapshot`) / outputs-out (`PanelOutputs`) with a player-facing `GameMenuState`/`GameMenuPage` pause menu and a `draw_player_setting` renderer. The *shape* is what §4 adopts. |
| **Save slots on disk** | [`crates/save/src/disk.rs`](../../crates/save/src/disk.rs) | **Partly done.** `save_<slot>.ess`, atomic write + ring, `list_slots`, `latest_slot`, `slots_by_recency`. Root from `BYROREDUX_SAVE_DIR`, default `saves`. |
| **Per-game compatibility data** | [`ROADMAP.md`](../../ROADMAP.md) compat matrix | **Done as prose.** Per-game archive format, NIF clean/recoverable parse rates, and which cells are known-good. Not machine-readable — see §5. |
| **Mod runtime** | [`crates/mod-runtime/`](../../crates/mod-runtime/) | **Early.** `PrincipalId` / `Principal` / `CapabilityId` / `CapabilitySet` identity and capability plumbing. No load-order model. Deferred to P4. |

**What genuinely does not exist**, and is therefore the actual work:

1. Install detection (§3). `DEFAULT_GAMES_ROOT` is
   `/mnt/data/SteamLibrary/steamapps/common` — one developer's box.
2. Pre-launch validation and a readable report (§3.2).
3. A stable, intent-shaped boot contract (§2).
4. Machine-readable per-game compatibility (§5).
5. Save-slot display metadata (§6) — the `.ess` container has a 32-byte binary
   header with no human-readable fields.
6. Graphics presets and a hardware pre-flight (§4.3).

---

## 2. The `BootRequest` contract

**This is the most important new artifact and the one most expensive to get
wrong**, because it is the boundary between a UI we want to keep simple and a
flag surface we want to keep free.

### 2.1 Rule

> The launcher emits **intent**, never flags. The engine owns the translation
> from intent to its own internals.

A GUI over argv would have to grow whenever a development flag lands. A GUI
over intent does not: `--cornell-oracle` is not an intent a player can hold.

### 2.2 Shape

New crate `crates/boot-request/`, depended on by both `byro-launcher` and
`byroredux`, so the two cannot drift. Serde-derived, TOML on disk.

```toml
# ~/.byroredux/boot.toml  — written by the launcher, read via `--boot <path>`
version = 1

[game]
profile   = "skyrim_se"                              # GameProfileEntry key
data_dir  = "/games/Skyrim Special Edition/Data"     # already resolved, absolute
esm       = "Skyrim.esm"
masters   = ["Update.esm"]                           # ordered

[game.archives]
meshes    = ["Skyrim - Meshes0.bsa"]
textures  = ["Skyrim - Textures0.bsa"]
scripts   = ["Skyrim - Misc.bsa"]
sounds    = ["Skyrim - Sounds.bsa"]
materials = []

[action]
kind = "new_game"
# kind = "continue";  slot = 3
# kind = "cell";      edid = "WhiterunBanneredMare"
# kind = "grid";      worldspace = "Tamriel"; x = 5; y = -24; radius = 1

[settings]
path = "~/.byroredux/settings.toml"                  # §4; the registry file

[mods]
load_order = []                                      # P4; empty until then
```

### 2.3 Engine side

One new flag, `--boot <path>`, handled in
[`boot.rs`](../../byroredux/src/boot.rs) at the **same seam** as
`expand_game_profile_args` and by the same mechanism: a `BootRequest` expands
into the argv vector the rest of the binary already consumes. Nothing
downstream of that function learns the launcher exists.

```
run()
  └── args = expand_boot_request(args)      # NEW — --boot <path> → argv
      └── args = expand_game_profile_args(args)   # existing — --game <key> → argv
```

Placing `expand_boot_request` *before* the profile expander means a
`BootRequest` may legitimately emit `--game <key>` and let the existing,
already-tested expander do the archive fan-out. The launcher only writes
explicit paths when it detected an install the shipped profile does not
describe.

### 2.4 Precedence

Explicit argv always wins over `--boot`, so a developer can override one field
of a launcher-written request without editing TOML:

```
cargo run -- --boot ~/.byroredux/boot.toml --cell WhiterunBanneredMare
```

This mirrors the precedence `expand_game_profile_args` already implements
(`--game` beats `[defaults].game`; other load flags suppress the default).

### 2.5 Versioning

`version = 1` is checked strictly. An unknown version is a launcher/engine
mismatch, which is a *user-visible install problem*, not a parse warning — the
engine refuses with a message naming both versions rather than half-loading.

---

## 3. Install detection and validation

The single largest UX gap, and the highest-value early work.

### 3.1 Detection

Detection produces *candidates*; the user confirms. Never silently pick.

| Source | Method |
|---|---|
| **Steam** | Parse `steamapps/libraryfolders.vdf` for every library path, then each library's `steamapps/appmanifest_<appid>.acf` for `installdir`. Known appids: Oblivion 22330, FO3 GOTY 22370, FNV 22380, Skyrim SE 489830, FO4 377160, Starfield 1716740. |
| **Steam root** | Linux: `~/.steam/steam`, `~/.local/share/Steam`, plus Flatpak `~/.var/app/com.valvesoftware.Steam/data/Steam`. Windows: `HKCU\Software\Valve\Steam\SteamPath`. macOS: out of scope. |
| **GOG** | Windows registry `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>\path`. Linux: Heroic/Lutris config JSON. |
| **Bethesda registry keys** | Windows `HKLM\SOFTWARE\WOW6432Node\Bethesda Softworks\<Game>\Installed Path` — still present for older titles. |
| **Manual** | A "Browse…" button (`rfd` native dialog). Always available, never a fallback-only path. |
| **Existing config** | `~/.byroredux/profiles.toml` and `BYROREDUX_GAMES_ROOT` are read first and treated as authoritative, so a developer's box keeps working unchanged. |

Detection results are written back into `~/.byroredux/profiles.toml` as `root`
overrides, which means **detection makes the existing `--game <key>` CLI path
work correctly for the first time on a machine that is not the dev box.** That
is a standalone win, deliverable in P1, independent of any GUI.

`DEFAULT_GAMES_ROOT` should stop being a Linux-Steam-specific literal and
become a last-resort after detection fails — the constant stays for
compatibility, but it is no longer the first answer.

### 3.2 Validation

Detection alone is not user-friendly; *validation with a readable verdict* is.
For a candidate install, before enabling Play:

1. `data_dir` exists and is readable.
2. The profile's `esm` exists; record its size and mtime.
3. Every archive in all five categories exists — **accounting for the
   sibling auto-load rule**: a `…0`-suffixed archive opens the whole
   zero-based series (`Textures0.bsa` → `Textures1..8`), per
   [`asset_provider/archive.rs`](../../byroredux/src/asset_provider/), so
   absent `Textures1.bsa` beside a present `Textures0.bsa` is not a finding.
4. Optional archives (Creation Club / Anniversary, DLC) are reported as
   present/absent, never as errors — the CC set varies per account.
5. Archive headers parse: open each with `byroredux-bsa` and read the header
   only. This catches a truncated download or a mod-manager-mangled archive
   *before* the engine renders an empty cell.
6. Free disk space for saves; write-permission probe on the save directory.

The result is a `ValidationReport` with one row per check at
`Ok` / `Warn` / `Fail`, rendered as plain sentences:

> **Fallout 4** — ready, with warnings
> ✓ `Fallout4.esm` (base game)
> ✓ Meshes: 2 archives · Textures: 9 archives
> ⚠ `DLCCoast - Main.ba2` missing — Far Harbor content will not load
> ✗ Materials archive `Fallout4 - Materials.ba2` missing — surfaces will render untextured

The current behaviour for every one of those rows is a line on stderr, or
silence followed by a visually broken scene. Converting them into a pre-launch
report is the plan's clearest single improvement.

### 3.3 Reuse, not duplication

Validation must call the real readers (`byroredux-bsa` for headers,
`byroredux-plugin` for the ESM `HEDR`), not re-implement file sniffing. Both
crates are renderer-independent and already build without Vulkan, so the
launcher can link them directly. **A validator that disagrees with the engine
is worse than no validator**, so there must be exactly one implementation of
"can this archive be opened".

---

## 4. Settings: one model, two skins

### 4.1 Rule

> Launcher and in-game menu share the **model** (`SettingsRegistry` snapshot in,
> `SettingChange` out), never the **widgets**.

This is already the architecture of
[`panels.rs`](../../crates/debug-ui/src/panels.rs) — `PanelSnapshot` in,
`PanelOutputs` out — and it is already the stated purpose of
`crates/core/src/settings.rs`. The launcher adopts it rather than inventing a
parallel notion of "graphics options".

### 4.2 Mechanism

The obstacle: settings are registered by *engine* code
(`byroredux_debug_ui::register_builtin_settings`,
`interaction::register_input_settings`) that the launcher must not link, since
`debug-ui` pulls `egui-ash-renderer` and therefore Vulkan.

Resolution — move registration down, not up:

1. Extract the built-in registrations into a renderer-free module
   (`crates/core/src/settings/builtin.rs`, or a thin `byroredux-settings`
   crate). They are pure `SettingEntry` constructions; nothing about them needs
   `egui`. `debug-ui` keeps its ID constants and re-exports.
2. The launcher builds the same registry, overlays the persisted file via the
   same `settings_io` load path, renders it, and writes it back.
3. The engine, unchanged, reads that file before `VulkanContext::new`
   ([`main.rs:520`](../../byroredux/src/main.rs)).

The `restart_required` flag on `SettingEntry` becomes meaningful for the first
time: in the launcher, *nothing* is restart-required, because the process has
not started. The launcher can therefore expose settings the in-game menu must
grey out.

### 4.3 Presets and hardware pre-flight

Two additions that are launcher-specific and do not belong in the registry:

**Presets.** `Low` / `Medium` / `High` / `Ultra` / `Custom` as named bundles of
`(id, value)` pairs shipped in `assets/graphics_presets.toml`. Selecting a
preset applies its pairs through `SettingsRegistry::set`, so preset values are
validated by the same bounds as manual edits. Editing any control afterwards
switches the label to `Custom` without discarding values.

**Pre-flight.** Query the GPU before offering presets. The launcher links
`ash` (already a workspace dep, and enumerating adapters does not require a
successful logical device) to read:

- Vulkan API version ≥ 1.3.
- `VK_KHR_ray_query` / `VK_KHR_acceleration_structure` present.
- `VkPhysicalDeviceMemoryProperties` device-local heap size.

Then state the verdict plainly. The project's own floor is ~6 GB VRAM for the
RT path; below it the launcher should default to the non-RT configuration and
say so, rather than let the user pick Ultra and meet a device loss:

> Detected: NVIDIA RTX 3060 (6 GB) · Vulkan 1.3 · ray query supported
> Recommended preset: **Medium**. Ray-traced GI is enabled but reflections are
> off at this VRAM budget.

A machine that fails the API/extension check gets an explicit, non-fatal
screen naming the missing capability — the §0.1 scenario, and the reason the
launcher renders on OpenGL.

---

## 5. Compatibility, surfaced honestly

The project supports seven titles at genuinely different maturity. A launcher
that shows six identical Play buttons is lying by omission.

Add `assets/compatibility.toml`, machine-readable, one block per profile,
authored from the [`ROADMAP.md`](../../ROADMAP.md) compat matrix:

```toml
[skyrim_se]
tier         = "playable"          # playable | partial | experimental
summary      = "Interiors and exteriors load. NPCs, combat, and saves work."
known_issues = ["No dialogue", "Weather is static"]
verified     = ["WhiterunBanneredMare", "BleakFallsBarrow01"]

[starfield]
tier         = "experimental"
summary      = "A single interior loads. Most systems are untested."
known_issues = ["Materials require materialsbeta.cdb", "No exterior support"]
verified     = ["Cydonia interior"]
```

Rendered as a badge plus one sentence on each game card, with the issue list
one click away. The tier gates nothing — an experimental game still launches —
it only sets expectation.

**Maintenance rule**: this file is a `/session-close` sync target alongside
ROADMAP/HISTORY. A compatibility claim that rots is worse than none, and this
is the one launcher artifact whose accuracy decays on its own.

---

## 6. Continue — save slots

### 6.1 The constraint

`.ess` is a 32-byte binary header (magic, `FORMAT_MAJOR`, `FORMAT_MINOR`,
schema fingerprint, CRC32, payload length) followed by a `serde_json` dump of
the entire world
([`crates/save/src/snapshot.rs`](../../crates/save/src/snapshot.rs)). There is
**no** human-readable metadata: no game, no cell, no timestamp, no playtime, no
player name. Producing a save list means either decoding a whole-world JSON
blob in the launcher — while holding the engine's schema fingerprint, which the
launcher cannot compute without linking the save registry — or changing the
format.

### 6.2 Resolution: a sidecar, not a format bump

Write `save_<slot>.json` beside `save_<slot>.ess`, through the same
`disk::atomic_write` already shared with `settings_io` (#3472):

```json
{ "version": 1, "slot": 3, "profile": "skyrim_se", "game_name": "Skyrim Special Edition",
  "location": "Whiterun", "cell": "WhiterunBanneredMare", "level": 4,
  "playtime_secs": 8134, "saved_at": "2026-08-30T14:02:11Z",
  "engine_format_major": 10, "schema_fingerprint": "0x…", "screenshot": "save_3.png" }
```

Rationale for the sidecar over extending the container: `FORMAT_MAJOR` is
already at 10, `decode` refuses any mismatch, and **no migrator chain exists** —
a bump invalidates every existing save for a cosmetic feature. The sidecar
touches nothing. A missing or corrupt sidecar degrades to "Slot 3 — no details",
never to a failed listing.

The launcher compares `engine_format_major` and `schema_fingerprint` against its
own build and marks incompatible slots as *unloadable, with the reason*, instead
of letting the user press Continue into a `SchemaMismatch` error.

Screenshot capture reuses the existing debug-server screenshot path
([`crates/debug-server/src/system.rs`](../../crates/debug-server/src/system.rs)).

---

## 7. Mods — deferred, but not designed away

`crates/mod-runtime` today carries identity and capability primitives
(`Principal`, `CapabilityId`, `CapabilitySet`) and no load-order model. Given
[`docs/engine/sandboxed-linked-mods.md`](sandboxed-linked-mods.md) and the
content-addressed-FormId direction, load order is a design question in its own
right and must not be improvised inside a launcher sprint.

P4 scope, when it comes: enumerate plugins in `data_dir`, read each `TES4`
header for its `MAST` list, topologically sort by declared masters, let the user
reorder within that constraint, and write the result to
`BootRequest.mods.load_order`. The `[mods]` table exists in the v1 contract from
day one, empty, so adding it later is not a `version` bump.

---

## 8. Screens

Five, no more. Every additional screen is a place for a first-time user to get
lost.

```
┌─ Library ──────────────────────────────────────────────┐
│  ┌──────────┐ ┌──────────┐ ┌──────────┐               │
│  │ Skyrim SE│ │ Fallout 4│ │ Starfield│  [+ Add game] │
│  │ PLAYABLE │ │ PARTIAL  │ │  EXPERIM.│               │
│  │ ✓ ready  │ │ ⚠ 1 warn │ │ ✓ ready  │               │
│  └──────────┘ └──────────┘ └──────────┘               │
└────────────────────────────────────────────────────────┘
        │                    │                  │
   [Play ▸]            [Details]          [Settings]
        │                    │                  │
   ┌────┴─────┐    ┌─────────┴────────┐  ┌──────┴──────┐
   │ New game │    │ Validation report│  │ Graphics    │
   │ Continue │    │ Known issues     │  │ Audio       │
   │ Load ▸   │    │ Paths / archives │  │ Controls    │
   └──────────┘    └──────────────────┘  │ Advanced    │
                                          └─────────────┘
```

Rules:

- **Play is one click from launch** for a validated install with no save.
- A failed validation replaces Play with **Fix** on the same button, opening
  Details at the offending row.
- Advanced settings are behind a disclosure, not a separate mode. The console
  and debug overlay are never mentioned.
- The launcher stays open behind the engine and shows a log tail on abnormal
  exit, so a crash produces a readable window rather than a vanished process.
  This is the §0.1 promise carried through the whole session.

---

## 9. Crate layout

```
crates/boot-request/        NEW — BootRequest types + TOML (de)serialisation.
                                 Serde only; no GPU, no engine deps.
                                 Linked by both launcher and byroredux.
crates/game-detect/         NEW — Steam/GOG/registry probing + ValidationReport.
                                 Links byroredux-bsa + byroredux-plugin for
                                 header-only checks (§3.3). No GPU.
tools/byro-launcher/        NEW — the eframe/glow binary. UI only; all logic
                                 lives in the two crates above so it is testable
                                 without a window.
crates/core/src/settings/   MOVED — built-in SettingEntry registrations, out of
                                 debug-ui, so the launcher can build the same
                                 registry without linking Vulkan (§4.2).
```

New dependencies: `eframe 0.33` (`glow` feature, `default-features = false`),
`rfd` (native file dialogs), `keyvalues-parser` or equivalent for VDF, and on
Windows `winreg`. `ash` and `serde` are already workspace deps.

**Invariant**: `byro-launcher` must contain no logic that is not also reachable
from a unit test without a window or a GPU. Detection, validation, preset
application, and `BootRequest` round-tripping are all pure functions over paths
and structs.

---

## 10. Rollout

Each phase ends at a gate that is demonstrable to someone who is not us.

### P1 — Boot contract and detection *(no GUI)*

1. `crates/boot-request` with round-trip tests and strict `version` handling.
2. `expand_boot_request` in `boot.rs`, ahead of `expand_game_profile_args`;
   argv precedence per §2.4.
3. `crates/game-detect`: Steam VDF/ACF walk, GOG and registry probes, the
   `ValidationReport`, and profile write-back to `~/.byroredux/profiles.toml`.
4. A `byro-dbg`-style CLI front end (`byro-detect`) that prints the report.

**Gate**: on a machine with no `BYROREDUX_GAMES_ROOT` and no hand-edited
profile, `byro-detect` finds every installed target title and
`cargo run -- --boot <written file>` reaches a rendered cell. This is the phase
that makes `--game <key>` correct off the dev box, and it ships value before any
window exists.

### P2 — The launcher window

1. `tools/byro-launcher`, eframe/glow, Library + Play + Details screens.
2. Detection on first run, with Browse fallback.
3. Launch by spawning `byroredux --boot <path>`; stay resident; tail the log
   and surface a non-zero exit.

**Gate**: a clean machine, no terminal, double-click to a walkable Whiterun.

### P3 — Settings and pre-flight

1. Extract built-in settings registration out of `debug-ui` (§4.2).
2. Settings screen driven by the shared registry; write through `settings_io`.
3. `assets/graphics_presets.toml` + preset application.
4. `ash` adapter enumeration, VRAM/extension pre-flight, recommended preset.

**Gate**: changing a graphics setting in the launcher measurably changes the
next launch, with no engine code aware the launcher exists. A machine without
ray-query gets a clear explanation instead of a failed launch.

### P4 — Continue and compatibility

1. `save_<slot>.json` sidecar written at save time; screenshot capture.
2. Save list with metadata, thumbnails, and explicit incompatible-slot marking.
3. `assets/compatibility.toml` + badges; added to the `/session-close` sync set.

**Gate**: quit mid-cell, relaunch, Continue, resume in place.

### P5 — Mods

Per §7, after the load-order design lands.

---

## 11. Out of scope

- Downloading, installing, or patching game content. ByroRedux redistributes no
  Bethesda data and the launcher must not appear to source any.
- A mod browser or any network service. P1–P4 make zero network requests; this
  is a deliberate property worth stating in the UI.
- Self-update. Distribution is unsolved and orthogonal.
- Controller remapping UI. Input settings are exposed through the shared
  registry; a binding capture UI is separate work.
- macOS. No Vulkan target today.

---

## 12. Open questions

| # | Question | Why it matters | Proposed answer |
|---|---|---|---|
| Q1 | Does the launcher stay resident, or exec-and-exit? | Resident gives crash reporting (§8); exec-and-exit is simpler and frees ~40 MB. | **Resident.** The crash-visibility argument is the same one that produced §0.1. |
| Q2 | Where does `~/.byroredux/` live on Windows? | Config path portability. | Adopt `directories`-style platform dirs, with `~/.byroredux` kept as an honoured legacy path on Linux so no dev box breaks. |
| Q3 | Should detection auto-write profile overrides, or ask? | Silent writes to a file a developer hand-edits are hostile. | Ask on first detection; never overwrite an existing non-empty `root` without confirmation. |
| Q4 | One `settings.toml` for all games, or per-profile? | Different games plausibly want different presets. | Start global (matches today). Add a per-profile overlay only when a concrete need appears. |
| Q5 | Can `ash` enumerate adapters when `vkCreateDevice` would fail? | The §4.3 pre-flight depends on it. | Believed yes — instance + `vkEnumeratePhysicalDevices` + property queries need no logical device — but **verify on a machine without ray-query before building the screen on it.** |
| Q6 | Does the shipped `debug_profiles.toml` become the launcher's catalogue, or does the launcher get its own? | Two catalogues would drift. | One file. The launcher reads the same registry; the "debug" name should be retired. |

---

## 13. Testing

- **Unit**: `BootRequest` round-trip; version-mismatch refusal; argv precedence;
  VDF/ACF parsing against checked-in fixture files; `ValidationReport` over a
  synthetic data dir with each failure mode injected; preset application through
  registry bounds.
- **Integration**: `expand_boot_request` → argv equivalence against the
  hand-written flag vectors in [`README.md`](../../README.md#run), so the
  contract provably reaches the same engine state as the documented CLI.
- **Smoke** (`docs/smoke-tests/`, per the existing `--bench-hold` + `byro-dbg`
  pattern): write a `BootRequest` for Skyrim SE `WhiterunBanneredMare`, launch
  through it, assert entity count and a clean `tex.missing`.
- **Manual matrix**: each of the six profiles, detected and validated, on the
  dev box; at minimum one clean-machine pass before P2's gate is claimed.
