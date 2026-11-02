# Cross-Platform Builds — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (platform-boundary HEAD claim, the `\\?\` prefix defect re-measured
against std's path source, GitHub GPU larger runners, the RT-extension
rejection's silent path, settled portability-subset and Windows file-identity
verify items, sibling PRD coverage, one observable per ID, the ROADMAP lines
Q1 and Q5 would amend); nothing below is built as a result of this document
yet, and the decisions it needs from the project owner are collected in §10.
Statements
about how an external driver, toolchain, operating system, file format or
original engine behaves cite external sources; a claim that could not be
verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: M70 promises that the engine and its host tools (`byroredux`,
`byro-launcher`, `byro-detect`, `byro-dbg`) build, test and run on more
than the maintainer's Linux box, without a second renderer and without a
portability layer on Windows. In scope: native Windows x86_64 builds on
vanilla Vulkan; the Steam Deck as a measured device of the existing Linux
build; a macOS position that follows from what Vulkan-on-Metal drivers can
actually run (§10 Q1); the CI matrix that keeps those builds green; install
detection, per-user paths and release artefacts per platform. Out of scope:
consoles, a second graphics API, a rasterized renderer written to reach
hardware without ray queries, native gamepad input, mobile, ARM Windows and
ARM Linux, and every hosted service (updater, crash reporter, telemetry)
that ROADMAP's anti-scope already rules out (§4.2). The users are players
whose machine is not Linux, and contributors who develop on one.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Windowing, raw handles, platform-specific paths | [Platform Abstraction](../engine/platform.md) |
| Install detection, preflight, profiles and settings paths, launcher packaging | [Launcher](../engine/launcher.md) |
| Vulkan init chain, device selection, RT requirements | [Vulkan Renderer](../engine/renderer.md) |
| FSR 3 native build (`fsr3-sys`) | [FSR 3 Upscaler Integration Plan](../engine/fsr3-upscaler-integration-plan.md) |
| C++ bridge build | [C++ Interop](../engine/cxx-interop.md) |
| Crate graph, native and build-time dependencies | [Dependencies](../engine/dependencies.md) |
| CI jobs, toolchain, prerequisites | [Contributing](../contributing.md) |
| Loose files and archive-key folding | [Archives](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| WASM mod runtime (wasmtime host) | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md) |
| The CI and release pipeline per platform | none yet; Phase 0 writes `docs/engine/platform-ci.md` |
| Cross-target equality of procedural noise (consumer of the M70 target list) | sibling PRD [Procedural Exteriors](procedural-exteriors.md), PGEN-GEN-009 |
| Windows identity of a baked source (consumer of XPLAT-PORT-007) | sibling PRD [Asset Preprocessing](asset-preprocessing.md), BAKE-INV-004 |
| OpenXR runtimes (Linux only; not widened here, XPLAT-NG-009) | sibling PRD [OpenXR / VR](openxr-vr.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `XPLAT-` prefix.

---

## 1. Product thesis

ROADMAP states the intent in one line: the project is "Linux-first by
primary maintainer setup, **not Linux-only by design**"
([M70](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon)).
Most of the audience for Bethesda-era mods plays on Windows, and the Steam
Deck is the largest single Linux gaming device. A Linux-only engine asks
that audience to change operating system before it can try the content it
already owns.

Two facts shape what "cross-platform" can mean here:

1. **The renderer is RT-only at HEAD.** Since #3759 the committed shader set
   declares `RayQueryKHR`, and `is_device_suitable` in
   `crates/renderer/src/vulkan/device.rs` rejects any device without
   `VK_KHR_acceleration_structure`, `VK_KHR_ray_query` and
   `VK_KHR_deferred_host_operations` (`RT_EXTENSIONS`), at HEAD on
   2026-11-01. #4894 removed the non-RT branches. A platform is reachable
   only through a Vulkan driver that exposes those extensions.
2. **No Vulkan-on-Metal driver exposes them today.** MoltenVK's runtime user
   guide lists the extensions it supports and the list carries neither
   `VK_KHR_acceleration_structure` nor `VK_KHR_ray_query`
   ([MoltenVK Runtime User Guide](https://github.com/KhronosGroup/MoltenVK/blob/main/Docs/MoltenVK_Runtime_UserGuide.md));
   its "Raytracing support" and "Implementing Vulkan Acceleration
   Structures" issues are both open
   ([KhronosGroup/MoltenVK#427](https://github.com/KhronosGroup/MoltenVK/issues/427),
   [#1956](https://github.com/KhronosGroup/MoltenVK/issues/1956), state
   checked 2026-11-01), and the latest dated release note, 1.4.2
   (2026-07-20), adds no ray-tracing extension
   ([Whats_New.md](https://github.com/KhronosGroup/MoltenVK/blob/main/Docs/Whats_New.md)).
   Mesa's feature table lists `VK_KHR_ray_query` and
   `VK_KHR_acceleration_structure` as done for `anv/gfx12.5+, lvp,
   radv/gfx10.3+, tu/a740+, vn`, and not for `kk`
   ([Mesa features.txt](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/docs/features.txt)),
   KosmicKrisp, which Mesa describes as "a Vulkan conformant implementation
   for macOS on Apple Silicon hardware … implemented on top of Metal 4"
   ([Mesa KosmicKrisp docs](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/docs/drivers/kosmickrisp.rst)).

The corollary that shapes everything below: **a platform is promised only
where the one renderer runs unchanged.** M70 adds build targets, CI lanes,
paths and packaging. It never adds a second renderer, a second graphics API
or a per-platform branch inside the renderer, the ECS or the translation
layers. Where the driver cannot run the RT renderer, the engine refuses with
a named reason instead of pretending, and the platform's tier says so
(§3). macOS is that case today, and §10 Q1 asks the owner what to do about
it.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Platform** | One (operating system, CPU architecture, Rust target triple) a build is produced for: Linux `x86_64-unknown-linux-gnu`, Windows `x86_64-pc-windows-msvc`, macOS `aarch64-apple-darwin`. |
| **Device** | One hardware + driver combination the engine runs on inside a platform; the Steam Deck is a device of the Linux platform, not a platform. |
| **Platform tier** | The promise level of a platform or device, one of the four classes in §3. |
| **Hosted lane** | A CI job on a standard GitHub-hosted runner (`ubuntu-*`, `windows-*`, `macos-*`). Has no GPU; the only Vulkan device it can offer is a software one such as lavapipe, which the engine rejects as a CPU device unless `BYRO_ALLOW_CPU_VULKAN_DEVICE` is set (`is_hardware_render_device`, `crates/renderer/src/vulkan/device.rs`). |
| **GPU larger runner** | GitHub's paid GPU-powered larger runner: 4 CPU, one Tesla T4 with 16 GB VRAM, Ubuntu or Windows, on the Team and Enterprise Cloud plans ([GitHub: larger runners](https://docs.github.com/en/actions/reference/runners/larger-runners)). Whether its driver passes the RT gate is XPLAT-CI-007; whether to pay for one is Q2. |
| **GPU acceptance run** | A run of the §7.2 scenarios on a real RT GPU of the platform, recorded with commit and data, because no standard hosted lane has a GPU. |
| **Portability driver** | A Vulkan driver that exposes `VK_KHR_portability_subset`; the loader hides its devices unless the instance sets `VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR` ([Vulkan-Loader: portability enumeration](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderDriverInterface.md#loader-implementation-of-vk_khr_portability_enumeration)). MoltenVK is one. |
| **RT gate** | The device-suitability rejection in `is_device_suitable` plus `missing_required_features` (`crates/renderer/src/vulkan/device.rs`). The only rule that decides whether a device can run the engine. |
| **Platform boundary** | The crates allowed to carry `cfg(target_os)` / `cfg(windows)` / `cfg(unix)` branches in production code (XPLAT-PORT-001). |

---

## 3. Platform tiers

Every platform and device has exactly one tier:

| Class | Name | Meaning | Reported as |
|---|---|---|---|
| **P1** | Primary | Every CI lane, every smoke test, every bench-of-record. The maintainer's box. | `contributing.md` prerequisites row "primary" |
| **P2** | Supported | Hosted lane builds and runs the unit tests on every push and blocks merge (per Q2); the §7.2 GPU acceptance runs once per release and its record is published. Release artefacts are produced. | prerequisites row "supported"; release notes name the last acceptance run |
| **P3** | Build-only | Hosted lane compiles and runs the unit tests; the engine refuses at device selection with the RT gate's named reason; no release artefact. | prerequisites row "build-only: <reason>" and the launcher preflight blocker |
| **P4** | Unsupported | Not built, not tested. | prerequisites row "unsupported: <reason>" |

The inventory below is the state at HEAD on 2026-11-01. It is hand-written;
Phase 1 replaces the CI column with the CI matrix itself (XPLAT-CI-001) and
the prerequisites table in `contributing.md` becomes the one user-facing
home of the tiers (XPLAT-REL-004).

| # | Component | Platform-specific content | Today |
|---|---|---|---|
| 1 | CI (`.github/workflows/ci.yml`) | Runner OS | Every job runs on `ubuntu-latest`, `ubuntu-24.04` or an `ubuntu:26.04` container; no Windows or macOS job exists. |
| 2 | Release (`.github/workflows/release.yml`) | Artefacts | One job, `linux`, builds on `ubuntu-22.04` (glibc 2.35 floor, stated in the file header) and publishes a tarball plus `SHA256SUMS`. |
| 3 | Toolchain (`rust-toolchain.toml`) | Compiler pin | `channel = "1.96.0"`, platform-neutral. |
| 4 | Shaders (`crates/renderer/shaders/*.spv`) | Shader compiler | SPIR-V committed and loaded with `include_bytes!`; `cargo build` never compiles shaders ([contributing.md](../contributing.md#recompiling-shaders)); `shader-artifacts` CI job checks parity on Linux. |
| 5 | Vulkan instance (`crates/renderer/src/vulkan/instance.rs`, `create_instance`) | Portability enumeration | Requests API 1.3 and the surface extensions `ash_window` returns; sets no `ENUMERATE_PORTABILITY` flag and does not request `VK_KHR_portability_enumeration`. |
| 6 | Device selection (`device.rs`, `is_device_suitable`, `missing_required_features`) | RT gate | Requires `VK_KHR_swapchain` (`REQUIRED_EXTENSIONS`), the three `RT_EXTENSIONS`, and every feature bit `missing_required_features` lists (the function is the list; this row does not copy it). A device missing an RT extension is dropped with no per-device log line; one missing a feature bit is logged by name. No platform branch. |
| 7 | `crates/fsr3-sys/build.rs` | Native C++ build of FidelityFX SDK v1.1.4 | Has `msvc` / non-`msvc` and `windows` / non-`windows` arms (links `vulkan-1` on Windows, `vulkan` elsewhere; defaults to `clang++` with `-fms-extensions -fdeclspec -fshort-wchar` off Windows). Only the Linux arm has ever run in CI. Unconditional dependency of `byroredux-renderer`. |
| 8 | `crates/cxx-bridge/build.rs` | C++17 via `cxx_build` | Platform-neutral build script; Linux-only in CI. |
| 9 | `crates/bsa/src/read_at.rs` | Positional reads | `cfg(unix)` `pread` and `cfg(windows)` `seek_read` implementations (#4999); the Windows arm is uncompiled in CI. |
| 10 | `crates/game-detect/src/steam.rs` (`steam_roots`) | Steam root discovery | Windows: `%ProgramFiles(x86)%\Steam` and `%ProgramFiles%\Steam` only; the registry lookup is "still to do" (comment in `steam_roots`); macOS: `~/Library/Application Support/Steam`. Every candidate is canonicalised, which on Windows yields a `\\?\` path (XPLAT-WIN-008). GOG and Bethesda registry probes absent (`detect` doc comment in `crates/game-detect/src/lib.rs`; [launcher.md §3.1](../engine/launcher.md#31-detection)). |
| 11 | `crates/settings-io/src/lib.rs` (`discover_settings_path`) | Per-user settings path | `%APPDATA%\ByroRedux`, `~/Library/Application Support/ByroRedux`, `$XDG_CONFIG_HOME/byroredux`, `~/.config/byroredux`. |
| 12 | `crates/game-detect/src/profiles.rs` (`home_dir`, profiles path) | Per-user profiles path | `~/.byroredux/profiles.toml` on every OS, home from `HOME` then `USERPROFILE` ([launcher.md Q2](../engine/launcher.md#12-open-questions)). |
| 13 | `byroredux/src/save_io/registry.rs` (`save_dir_from`) | Save root | `BYROREDUX_SAVE_DIR` or the relative path `saves`, i.e. the process working directory. |
| 14 | `tools/byro-launcher` | Launcher window, engine location | `eframe` with `glow`; `ENGINE_EXE` appends `.exe` on Windows (`engine.rs`); `BYROREDUX_LAUNCHER_X11` is Linux-only (`main.rs`). Preflight (`preflight.rs`, `probe`) creates an instance with no portability flag. |
| 15 | `crates/ui/src/player.rs` | Ruffle wgpu backend | Requests `wgpu::Backends::VULKAN` only; `wgpu`'s Vulkan backend on macOS needs its `vulkan-portability` feature ([wgpu 30 Cargo.toml](https://github.com/gfx-rs/wgpu/blob/v30.0.0/wgpu/Cargo.toml)), which `crates/ui/Cargo.toml` does not enable. |
| 16 | `crates/mod-runtime` | wasmtime 49 (Cranelift) | No platform code. |
| 17 | `byroredux/src/hud.rs` | Debug dump path | `BYRO_HUD_DUMP=1` writes to the literal `/tmp/hud_engine_frame.rgba`. |
| 18 | Loose tier (`byroredux/src/asset_provider/archive.rs`, `LooseDir::index`) | Filesystem case | Folds every key to the lowercase, backslash archive form and warns on keys that collide after folding; `loose_case_variant_duplicates_resolve_to_the_last_path` skips itself on a case-insensitive filesystem. |
| 19 | Shared-archive cache (`archive.rs`, `ArchiveStamp::identity_of`) | Rewrite detection | `cfg(unix)` stamps device, inode and ctime (#5663); `cfg(not(unix))` returns zeros, so other platforms compare length and mtime only. |
| 20 | `byroredux/src/ui_input.rs` (`text_control`) | Text-edit shortcuts | `cfg!(target_os = "macos")` lets Super stand in for Ctrl (Cmd+A, …); the only production platform branch in the binary crate outside the asset provider. |

Proposed tiers, subject to §10:

| Platform / device | Proposed tier | Why |
|---|---|---|
| Linux x86_64 | P1 | The maintainer setup ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)). |
| Steam Deck (Linux x86_64, RADV) | P2 | Same binary; its GPU family is listed as ray-query capable by Mesa (XPLAT-DECK-002). |
| Windows x86_64 (MSVC) | P2 | Vanilla Vulkan drivers expose the RT extensions on the GPUs the engine already targets; nothing in the renderer is Linux-specific. |
| macOS aarch64 | P3 (Q1) | No Vulkan-on-Metal driver passes the RT gate (§1). |
| Windows ARM64, Linux aarch64, macOS x86_64 | P4 | XPLAT-NG-004. |

---

## 4. Goals and non-goals

### 4.1 Goals

- **XPLAT-G-001 — Windows is a supported platform.** The Windows hosted lane
  is green on `main`, and a GPU acceptance run of §7.2 passes on the release
  it is recorded for.
- **XPLAT-G-002 — Steam Deck is a measured device.** The Linux release
  artefact boots on a Deck, passes the RT gate and has its frame time
  published per release.
- **XPLAT-G-003 — macOS is honest.** Whatever Q1 decides, a macOS user gets
  either a working build or a refusal that names the missing Vulkan
  capability; never a crash and never a silent black window.
- **XPLAT-G-004 — One engine.** No requirement below adds a renderer path,
  a shader permutation or a `cfg` branch outside the platform boundary;
  `XPLAT-PORT-001`'s check counts zero violations.
- **XPLAT-G-005 — The matrix stays green.** Each P1–P3 platform has a CI
  lane, and the share of `main` commits on which every lane passed is
  published per month.

### 4.2 Non-goals

- **XPLAT-NG-001.** Consoles. ROADMAP's anti-scope rules them out
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **XPLAT-NG-002.** Non-Linux *primary* support. Linux stays P1; Windows and
  macOS are "downstream" in the anti-scope's own word. This PRD defines what
  downstream means (P2/P3); it does not move the maintainer's lanes.
- **XPLAT-NG-003.** A second graphics API (Direct3D 12, Metal) or a portability
  layer on Windows. ROADMAP's M70 row says "vanilla Vulkan, no portability
  layer"; a Metal backend for macOS is listed only as an alternative in Q1.
- **XPLAT-NG-004.** Windows ARM64, Linux aarch64 and Intel macOS. None has
  a demand signal. Windows ARM64 also has the weakest mod-sandbox
  guarantee: wasmtime rates `aarch64-pc-windows-msvc` Tier 3, "Not
  Production Ready"
  ([wasmtime stability tiers](https://docs.wasmtime.dev/stability-tiers.html)).
  Linux aarch64 is not excluded for its wasmtime tier: it is Tier 2, the same
  tier as `aarch64-apple-darwin`, which §3 keeps at P3. Intel Macs fail the RT
  gate for the same reason Apple silicon does (§1), and KosmicKrisp targets
  Apple Silicon only.
- **XPLAT-NG-005.** Hosted services on any platform: updater, crash
  reporter posting upstream, telemetry, store integration
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope);
  [launcher.md §11](../engine/launcher.md#11-out-of-scope)).
- **XPLAT-NG-006.** Native gamepad input. [Platform §Future Expansion](../engine/platform.md#future-expansion)
  owns the input abstraction; the Deck is measured with Steam Input mapping
  its controls (XPLAT-DECK-005).
- **XPLAT-NG-007.** Detection of Microsoft Store / Xbox app installs. Only
  Steam, GOG and the Bethesda registry keys of [launcher.md §3.1](../engine/launcher.md#31-detection)
  are in scope; any other install stays reachable through a `[roots]`
  override.
- **XPLAT-NG-008.** Code signing and notarization. Distribution policy is
  Q4; this PRD promises unsigned artefacts at most.
- **XPLAT-NG-009.** VR on Windows or macOS. The OpenXR promise is scoped to
  Linux runtimes by the sibling [OpenXR / VR PRD](openxr-vr.md); widening it
  is that PRD's revision, not this one's.

---

## 5. Requirements by area

### 5.1 CI matrix (`XPLAT-CI`)

- **XPLAT-CI-001.** `ci.yml` MUST carry a Windows job on a hosted
  `windows-*` x64 runner that runs `cargo check --workspace`,
  `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`
  with the `rust-toolchain.toml` pin. Today no job runs on Windows (§3
  row 1), at HEAD on 2026-11-01.
- **XPLAT-CI-002.** If Q1 keeps macOS at P3 or above, `ci.yml` MUST carry a
  macOS job on a hosted arm64 runner running the same three commands for
  `aarch64-apple-darwin`.
- **XPLAT-CI-003.** Jobs whose subject is platform-neutral
  (`issue-traceability`, `source-integrity`, `unused-dependencies`,
  `shader-artifacts`, `playable-smoke-contracts`) SHOULD stay Linux-only; the
  matrix MUST NOT duplicate them per platform.
- **XPLAT-CI-004.** A test that cannot run on a platform MUST be gated with
  `cfg` plus a one-line reason comment, or probe at run time and print a
  skip message. Today the probe pattern exists without the message:
  `loose_case_variant_duplicates_resolve_to_the_last_path`
  (`byroredux/src/asset_provider/tests/loose_override.rs`) returns silently
  on a case-insensitive filesystem, at HEAD on 2026-11-01.
- **XPLAT-CI-005.** The Windows (and macOS) job MUST be required for merge
  or advisory according to Q2, and `contributing.md`'s CI table MUST state
  which.
- **XPLAT-CI-006.** The GPU and game-data lanes (`vulkan-validation`,
  `rt-correctness.yml`, `real-data-gates.yml`, `playable-smoke.yml`) MAY
  remain Linux-only; the Windows GPU acceptance is the recorded run of
  XPLAT-WIN-005.
- **XPLAT-CI-007.** **Phase 0: verify** whether GitHub's Windows GPU larger
  runner (one Tesla T4, [GitHub: larger runners](https://docs.github.com/en/actions/reference/runners/larger-runners))
  exposes a device that passes the RT gate. Standard hosted runners have no
  GPU ([GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
  lists GPUs only under larger runners); a Mesa lavapipe build would offer
  `lvp`, which Mesa lists with `VK_KHR_ray_query`, but it is a CPU device
  that both the engine (`is_hardware_render_device`, unless
  `BYRO_ALLOW_CPU_VULKAN_DEVICE`) and the launcher (`Blocker::SoftwareOnly`,
  `tools/byro-launcher/src/preflight.rs`) refuse, so it serves only a
  validation lane, as on Linux. Settled by `vulkaninfo` plus
  `cargo test -p byro-launcher -- --ignored --nocapture probe` (the test is
  `#[ignore]`d) on that runner. If it passes and Q2 funds it, a Windows
  `vulkan-validation` lane SHOULD follow.
- **XPLAT-CI-008.** Each P1–P3 hosted lane MUST print, as one log line, the
  number of tests it skipped by `cfg` or by run-time probe (the input of the
  §8 skipped-test share).

### 5.2 Windows (`XPLAT-WIN`)

- **XPLAT-WIN-001.** `cargo build --release --locked -p byroredux -p
  byro-launcher -p byro-detect -p byro-dbg` MUST succeed for
  `x86_64-pc-windows-msvc`, the target Rust lists as Tier 1 with host tools
  ([rustc platform support](https://doc.rust-lang.org/rustc/platform-support.html))
  and wasmtime lists as Tier 1 ([stability tiers](https://docs.wasmtime.dev/stability-tiers.html)).
  Today it has never been built (§3 rows 1, 7).
- **XPLAT-WIN-002.** `crates/fsr3-sys` MUST build with the MSVC compiler
  through its existing `msvc` arm and link `vulkan-1` (`build.rs`, at HEAD
  on 2026-11-01).
- **XPLAT-WIN-010.** After the first Windows build, each of the nine
  portability patches listed in `third_party/fidelityfx-sdk-v1.1.4/UPSTREAM.md`
  MUST carry, in that file, the marker "needed on Linux only" or "needed
  everywhere".
- **XPLAT-WIN-003.** The Windows package MUST NOT contain a `vulkan-1.dll`:
  the Khronos loader "is generally installed to Windows systems as part of
  driver installation"
  ([Vulkan-Loader: LoaderApplicationInterface](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderApplicationInterface.md)).
- **XPLAT-WIN-011.** On a Windows machine without a Vulkan loader,
  `byroredux.exe` MUST exit non-zero with the `Failed to load Vulkan loader`
  error (`crates/renderer/src/vulkan/context/init.rs`), and the launcher MUST
  show `Blocker::NoVulkan` (`preflight.rs`, `probe`); both paths exist at
  HEAD on 2026-11-01 and have never run on Windows.
- **XPLAT-WIN-004.** `byroredux.exe --version` and `--help`, and
  `byro-launcher.exe --help` and `byro-detect.exe --help`, MUST exit 0 on
  the hosted runner, mirroring the "Smoke-test the binaries without a GPU"
  step of `release.yml`.
- **XPLAT-WIN-005.** On a Windows machine with an RT-capable GPU, the §7.2
  scenario W1 MUST pass, and its record (commit, GPU, driver version, data
  title, `bench:` line, `tex.missing` count) MUST be published with the
  release it is run for.
- **XPLAT-WIN-006.** `game-detect` MUST find a Steam install whose root is
  not under `%ProgramFiles(x86)%` or `%ProgramFiles%` by reading the Steam
  root from the registry, as [launcher.md §3.1](../engine/launcher.md#31-detection)
  specifies. Today only the two environment-variable candidates exist
  (`steam_roots`). **Phase 0: verify** the key and value name
  (`HKCU\Software\Valve\Steam`, `SteamPath`) against a current Steam client
  install; this draft found no Valve documentation for it.
- **XPLAT-WIN-007.** `game-detect` SHOULD probe the GOG and Bethesda
  registry keys listed in [launcher.md §3.1](../engine/launcher.md#31-detection).
  **Phase 0: verify** each key path per title on a real install (GOG
  Oblivion, FO3, FNV and Skyrim; disc/retail installs for the Bethesda
  `Installed Path` value); a key not confirmed MUST NOT be probed.
- **XPLAT-WIN-008.** No `SteamInstall::install_path` that `game-detect`
  returns, and so no root it writes to `profiles.toml` or passes to the
  engine, MAY carry the `\\?\` extended-length prefix; a new test
  `steam_install_under_a_canonical_root_has_no_verbatim_prefix` MUST pass on
  the Windows lane. Rust's `canonicalize` produces that prefix on Windows and
  warns that the result "may be incompatible with other applications (if
  passed to the application on the command-line, or written to a file
  another application may read)"
  ([std::fs::canonicalize](https://doc.rust-lang.org/std/fs/fn.canonicalize.html)).
  Today `steam_roots` canonicalises every candidate (`steam.rs`, the
  `candidate.canonicalize()` loop), `library_paths` seeds its list with that
  root, and `detect` keeps the first hit per appid, so an install in the
  Steam root's own library inherits the prefix, at HEAD on 2026-11-01. The
  forward-slash `"steamapps/libraryfolders.vdf"` join in `library_paths` is
  not itself the defect: when `self` is verbatim, `PathBuf::push` re-splits
  the pushed relative path into components and rejoins them with `\`
  (`_push` in `library/std/src/path.rs` of the pinned 1.96.0 toolchain), so
  the manifest is still found. A second consequence: the manifest's own
  entry for the root is not `\\?\`-prefixed, a verbatim and a plain disk
  prefix compare unequal (`PrefixComponent`'s `PartialEq`, same file), so
  `libraries.contains` does not recognise it and the root library is
  scanned twice; the appid dedup hides it. Both readings come from the std
  source, not from a Windows run; the test above is what confirms them.
- **XPLAT-WIN-009.** `profiles.toml` MUST resolve through the same per-user
  resolver as `settings.toml` (`discover_settings_path`), keeping
  `~/.byroredux/profiles.toml` as an honoured legacy location, which is the
  answer [launcher.md Q2](../engine/launcher.md#12-open-questions) proposes.
  Today it is `~/.byroredux` on every OS (§3 row 12).

### 5.3 Steam Deck (`XPLAT-DECK`)

The Deck runs "SteamOS 3 (Arch-based)" on an APU with "8 RDNA 2 CUs"
([Steam Deck tech specs](https://www.steamdeck.com/en/tech)); RADV "is also
the Vulkan driver on the Steam Deck"
([Mesa RADV docs](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/docs/drivers/radv.rst));
Mesa files the Deck's chip, `CHIP_VANGOGH`, under GFX10.3
([amd_family.h](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/src/amd/common/amd_family.h)),
and lists `VK_KHR_ray_query` and `VK_KHR_acceleration_structure` as done
for `radv/gfx10.3+` ([features.txt](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/docs/features.txt)).

- **XPLAT-DECK-001.** The Linux release tarball MUST start on a Deck in
  desktop mode with `ldd target/release/byroredux` reporting no
  `not found`, the check `release.yml` already runs on its build host.
  **Phase 0: verify** that the current SteamOS release ships glibc 2.35 or
  later (the floor `release.yml` states) and the ALSA and Vulkan loader
  libraries the binary links; settled by `ldd --version` and `ldd` on a
  Deck.
- **XPLAT-DECK-002.** The Deck's device MUST pass the RT gate. **Phase 0:
  verify** with `vulkaninfo` on the shipping SteamOS image that every
  extension in `RT_EXTENSIONS` and every feature in
  `missing_required_features` is reported; Mesa's table says what the
  driver supports upstream, not what SteamOS ships.
- **XPLAT-DECK-003.** Scenario D1 (§7.2) MUST record the Deck's frame time
  at its native 1280×800 per release in the ROADMAP table XPLAT-REL-004
  names; whether a target applies is Q3.
- **XPLAT-DECK-004.** The engine SHOULD start and present from Steam's Game
  Mode as a non-Steam shortcut. **Phase 0: verify** that winit creates the
  window under Game Mode's compositor; settled by running scenario D1
  there.
- **XPLAT-DECK-005.** Scenario D1 MUST be operable with the Deck's controls
  through Steam Input mapped to keyboard and mouse; native gamepad input is
  XPLAT-NG-006. Valve's own compatibility review would class a title that
  needs a manually chosen controller config as "Playable", not "Verified"
  ([Steam Deck compatibility review](https://partner.steamgames.com/doc/steamdeck/compat)),
  and the PRD makes no stronger claim.
- **XPLAT-DECK-006.** The preset `byro-launcher` recommends on the Deck MUST
  be one at which scenario D1 passes. Today `recommended_preset` and
  `meets_rt_floor` key on the device-local heap size alone
  (`tools/byro-launcher/src/preflight.rs`), against the 6 GB RT minimum of
  [Memory Budget](../engine/memory-budget.md). **Phase 0: verify** the
  device-local heap RADV reports on the Deck's unified memory; settled by
  the launcher's `summary` line on a Deck.

### 5.4 macOS (`XPLAT-MAC`)

Every requirement here holds under Q1's recommendation (P3); the
alternatives in Q1 add requirements, they do not remove these.

- **XPLAT-MAC-001.** `create_instance` and the launcher's `probe` MUST set
  `VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR` and request
  `VK_KHR_portability_enumeration` when the loader advertises the
  extension, so that MoltenVK's devices are enumerated at all: the loader
  hides portability drivers otherwise
  ([MoltenVK Runtime User Guide](https://github.com/KhronosGroup/MoltenVK/blob/main/Docs/MoltenVK_Runtime_UserGuide.md);
  [Vulkan-Loader](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderDriverInterface.md#loader-implementation-of-vk_khr_portability_enumeration)).
  Today neither sets it (§3 rows 5, 14), at HEAD on 2026-11-01.
- **XPLAT-MAC-006.** On a Linux or Windows machine with no portability
  driver, the change of XPLAT-MAC-001 MUST leave the `Suitable GPU:` line
  `pick_physical_device` logs, and the launcher's `summary` line, unchanged
  (scenario U1).
- **XPLAT-MAC-002.** On a device that fails the RT gate, the launcher
  preflight MUST report `Blocker::MissingExtension` naming the first
  missing RT extension, not `Blocker::NoAdapter`. The blocker shape exists
  and is unit-tested (`each_blocker_is_reported_specifically`); what is
  missing on macOS is the enumeration of XPLAT-MAC-001. `verdict` checks the
  API version before the extensions, so this holds only while MoltenVK
  reports Vulkan 1.3 or later; its user guide calls it "a layered
  implementation of Vulkan 1.4"
  ([MoltenVK Runtime User Guide](https://github.com/KhronosGroup/MoltenVK/blob/main/Docs/MoltenVK_Runtime_UserGuide.md)).
- **XPLAT-MAC-003.** `byroredux` started on such a device MUST exit non-zero
  after logging one line that names the device and its first missing RT
  extension or feature bit, without creating a logical device. Today the
  feature-bit rejection logs `Rejecting GPU …: missing required Vulkan
  features` per device, but a device missing an RT extension is dropped by
  `is_device_suitable` with no log line, and the final `No suitable GPU
  found` error lists the requirements generically
  (`crates/renderer/src/vulkan/device.rs`), at HEAD on 2026-11-01.
- **XPLAT-MAC-004.** If a device advertising `VK_KHR_portability_subset`
  ever passes the RT gate, `create_logical_device` MUST enable that
  extension. The rule is VUID-VkDeviceCreateInfo-pProperties-04451: "If the
  VK_KHR_portability_subset extension is included in pProperties of
  vkEnumerateDeviceExtensionProperties, ppEnabledExtensionNames must include
  "VK_KHR_portability_subset""
  ([Vulkan: VkDeviceCreateInfo](https://registry.khronos.org/vulkan/specs/latest/man/html/VkDeviceCreateInfo.html)).
- **XPLAT-MAC-005.** The PRD's macOS tier MUST be re-reviewed when either
  MoltenVK's extension list or Mesa's `features.txt` row for `kk` gains both
  `VK_KHR_acceleration_structure` and `VK_KHR_ray_query`; Phase 0 records
  the two sources and the check in `platform-ci.md`.

### 5.5 Platform-neutral code (`XPLAT-PORT`)

- **XPLAT-PORT-001.** Production (non-test) `cfg(target_os = …)`,
  `cfg(windows)` and `cfg(unix)` branches MUST be confined to the platform
  boundary: `crates/platform`, `crates/settings-io`, `crates/game-detect`,
  `crates/bsa/src/read_at.rs`, `crates/fsr3-sys/build.rs`,
  `crates/audio/src/fuz.rs`, `ArchiveStamp::identity_of` in
  `byroredux/src/asset_provider/archive.rs`, `text_control` in
  `byroredux/src/ui_input.rs`, and `tools/*`. A new test
  `platform_cfg_stays_inside_the_boundary` MUST scan the tree and fail on a
  branch elsewhere. Today an `rg` for `cfg(`/`cfg!(` with `target_os`,
  `windows`, `unix` or `target_env`, excluding test modules, finds
  production branches only in those locations (the `ui_input.rs` one is the
  macOS Cmd-for-Ctrl shortcut, §3 row 20), at HEAD on 2026-11-01; the
  renderer, core, ECS and translation crates carry none. Whether
  `text_control`'s branch stays in the binary crate or moves into
  `crates/platform` beside the input abstraction is a design call for
  [platform.md](../engine/platform.md#future-expansion), not this PRD's.
- **XPLAT-PORT-002.** No production code path MAY write to a hard-coded
  Unix absolute path; temporary files go through `std::env::temp_dir`.
  Today `byroredux/src/hud.rs` writes `/tmp/hud_engine_frame.rgba` under
  `BYRO_HUD_DUMP=1` (§3 row 17).
- **XPLAT-PORT-003.** The `loose_override` tests
  (`byroredux/src/asset_provider/tests/loose_override.rs`) MUST pass on every
  CI platform, which is how "asset lookup resolves the same file on
  case-sensitive and case-insensitive filesystems" is seen. Bethesda archives key
  entries by lowercase names under backslash folders ("menus\chargen";
  "a list of lower case file names", [UESP: Oblivion Mod:BSA File Format](https://en.uesp.net/wiki/Oblivion_Mod:BSA_File_Format),
  local dump `uesp-wiki/Oblivion Mod/BSA File Format.wiki`), the Windows
  file system "treats file and directory names as case-insensitive"
  ([Microsoft: case sensitivity](https://learn.microsoft.com/en-us/windows/wsl/case-sensitivity)),
  and macOS offers APFS in a case-insensitive and a case-sensitive variant
  ([Apple: Disk Utility file system formats](https://support.apple.com/guide/disk-utility/file-system-formats-dsku19ed921c/mac)).
  Today the loose tier folds keys itself (§3 row 18).
- **XPLAT-PORT-004.** The committed SPIR-V MUST remain the only shader
  artefact; no platform MAY compile shaders at build time (§3 row 4).
- **XPLAT-PORT-005.** Save slots SHOULD default to a per-user data directory
  resolved like `discover_settings_path` when `BYROREDUX_SAVE_DIR` is unset,
  instead of the working-directory-relative `saves` of `save_dir_from`
  (§3 row 13), so that a process started from a read-only directory still
  saves. The change MUST keep `BYROREDUX_SAVE_DIR` authoritative, since the
  smoke gates set it.
- **XPLAT-PORT-006.** `crates/ui`'s Ruffle backend MAY enable `wgpu`'s
  `vulkan-portability` feature on macOS only when Q1 selects a macOS tier
  that renders (§3 row 15); under P3 it MUST NOT, since nothing renders.
- **XPLAT-PORT-007.** The test
  `open_shared_rereads_a_same_length_rewrite_that_restores_the_mtime`
  (`byroredux/src/asset_provider/archive.rs`, `#[cfg(unix)]` today) SHOULD
  run and pass on the Windows lane, which is the observable of "the
  shared-archive cache detects a same-length, same-mtime rewrite on Windows
  as on Unix" (#5663). Today `ArchiveStamp::identity_of` returns device,
  inode and ctime under `cfg(unix)` and all zeros elsewhere, so on Windows
  the stamp is length + mtime only (§3 row 19), at HEAD on 2026-11-01. The
  pinned 1.96.0 std offers no stable replacement:
  `MetadataExt::volume_serial_number` and `file_index` are unstable
  (`windows_by_handle`, rust-lang/rust#63010) and `change_time` is unstable
  (`windows_change_time`, rust-lang/rust#121478), per
  `library/std/src/os/windows/fs.rs`. An in-place rewrite keeps the file
  index, so the field that would catch it is the change time, which Win32
  exposes as `FILE_BASIC_INFO.ChangeTime`, "the time the file was changed"
  ([Microsoft: FILE_BASIC_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_basic_info)).
  **Phase 0: verify** that `ChangeTime` moves on a same-length rewrite that
  restores the last-write time, settled by running the test above against
  a Win32 implementation on the Windows lane; if it does not, the gap is
  documented in `platform-ci.md`. The sibling
  [Asset Preprocessing PRD](asset-preprocessing.md)'s BAKE-INV-004 inherits
  whichever answer this item reaches.
- **XPLAT-PORT-008.** Golden tests that sibling PRDs use to assert
  cross-target numeric equality, starting with PGEN-GEN-009 of
  [Procedural Exteriors](procedural-exteriors.md), MUST be part of
  `cargo test --workspace` so that every P1–P3 hosted lane runs them; this
  PRD owns that they run per target triple (§2), not their result.

### 5.6 Release and reporting (`XPLAT-REL`)

- **XPLAT-REL-001.** `release.yml` MUST produce a Windows zip and add its
  line to `SHA256SUMS` beside the Linux tarball once Windows reaches P2,
  per Q4.
- **XPLAT-REL-002.** No release artefact on any platform MAY make a network
  request on its own; the launcher's "zero network requests" property
  ([launcher.md §11](../engine/launcher.md#11-out-of-scope)) holds per
  platform.
- **XPLAT-REL-003.** Release notes MUST name, per platform, the tier and
  the commit of the last GPU acceptance run (XPLAT-WIN-005, XPLAT-DECK-003).
- **XPLAT-REL-004.** `contributing.md`'s prerequisites table MUST carry one
  row per platform with its tier and build prerequisites (Vulkan SDK on
  Windows and macOS for `vulkan-1.lib` / `libvulkan`, the C++ compiler per
  platform); today it has one row, "Linux — Primary target. Windows/macOS
  untested", and lists "Rust stable" where the repository pins 1.96.0
  (`rust-toolchain.toml`).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The original executables run on Windows, whose file system is case-insensitive ([Microsoft](https://learn.microsoft.com/en-us/windows/wsl/case-sensitivity)); data paths assume it. | The engine folds case itself on every platform (XPLAT-PORT-003). | Case-fixing scripts for Linux installs of mods. | Two loose files differing only in case are distinct on Linux and impossible on Windows; the lexicographic rule may pick a different file than a Windows user expects. → the collision warning of `LooseDir::index`, XPLAT-PORT-003. |
| 2 | Original install discovery uses registry keys (per [launcher.md §3.1](../engine/launcher.md#31-detection); **Phase 0: verify**, XPLAT-WIN-007). | Steam manifests first, registry probes only for confirmed keys. | — | An unconfirmed key misses an install. → `[roots]` override, XPLAT-WIN-007. |
| 3 | — | The renderer runs only on drivers exposing ray queries; macOS is refused, not degraded (§1). | — | Mac users have no runtime. → XPLAT-MAC-002, XPLAT-MAC-003, Q1. |
| 4 | — | Windows GPU behaviour is checked once per release, not per commit (no GPU on standard hosted runners; a GPU larger runner is unverified and paid, XPLAT-CI-007, Q2). | — | A Windows-only driver regression lands unnoticed between releases. → XPLAT-WIN-005, XPLAT-REL-003, Q2. |
| 5 | — | The Linux artefact links the build host's glibc (`release.yml` header). | — | A SteamOS update below the floor, or a missing library, stops the Deck. → XPLAT-DECK-001. |
| 6 | — | `fsr3-sys` builds FidelityFX on every platform, with Linux-only portability patches. | — | A patch that only Linux needs changes behaviour on MSVC. → XPLAT-WIN-002; the TAA upscaler choice (`render.upscaler = "taa"`, `crates/settings-io/src/presets.rs`) stays available if FSR regresses on one platform. |
| 7 | — | Archive-rewrite detection is weaker off Unix (§3 row 19). | — | A same-length, same-mtime rewrite of an archive mid-session serves stale entries on Windows. → XPLAT-PORT-007. |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Platforms are orthogonal to games: the per-game promise on every P1/P2
platform is the one in the compatibility matrix
([ROADMAP](../../ROADMAP.md#compatibility-matrix);
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail)).
No requirement here adds a per-game branch, and no per-platform per-game
row is created.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Inherits its tier; Windows install detection via Steam, GOG (XPLAT-WIN-007). |
| Fallout 3 | yes | Inherits its tier; Steam and GOG detection. |
| Fallout: New Vegas | yes | Reference title; scenarios W1 and D1 use it. |
| Skyrim (LE / SE / AE) | yes | SE inherits its tier; second title for W1. LE follows its existing tier. |
| Fallout 4 | yes | Inherits its tier; heaviest scenes, so it is the Deck stress title in D2. |
| Fallout 76 | yes | Inherits its parser-only tier; no runtime scenario because it has none on Linux either. |
| Starfield | yes | Inherits its tier for Steam installs; Microsoft Store installs are XPLAT-NG-007. |

### 7.2 Acceptance

Game data is read from the user's own install and never checked in.
Release builds need `BYRO_DEBUG_SERVER=1` for `byro-dbg` to attach.

| ID | Platform | Data | Steps | Passes when |
|---|---|---|---|---|
| H1 | Windows hosted lane | none | `cargo test --workspace`; `cargo clippy --workspace -- -D warnings`; build of XPLAT-WIN-001; the `--version` / `--help` set of XPLAT-WIN-004 | All exit 0; the skipped-test count is printed (XPLAT-CI-004). |
| H2 | macOS hosted lane (if Q1 ≥ P3) | none | `cargo test --workspace`; clippy; `cargo build -p byroredux` | All exit 0. |
| W1 | Windows, RT GPU | FNV (`FalloutNV.esm`, `Fallout - Meshes.bsa`, `Fallout - Textures.bsa`); Skyrim SE as second title | `byroredux.exe --esm FalloutNV.esm --cell GSDocMitchellHouse --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --bench-frames 300 --bench-hold` with `BYRO_VALIDATION=1`, then `byro-dbg` → `stats`, `tex.missing`, `screenshot w1.png` | A `bench:` summary line is printed; no `[Vulkan]` ERROR line; `tex.missing` equals the Linux control run on the same commit, data and pinned camera ([bench method](../../ROADMAP.md#how-to-run-a-bench)). |
| W2 | Windows, RT GPU | as W1 | `byro-launcher.exe`, no profiles file; `byro-detect.exe` | The Steam install is detected (XPLAT-WIN-006); written roots carry no `\\?\` (XPLAT-WIN-008); the launcher starts W1's cell. |
| D1 | Steam Deck, desktop mode then Game Mode | FNV as W1 | The Linux tarball; W1's command at 1280×800 with the preset of XPLAT-DECK-006; Steam Input keyboard/mouse mapping | `bench:` line printed; frame time recorded (XPLAT-DECK-003); the window presents under Game Mode (XPLAT-DECK-004). |
| D2 | Steam Deck | FO4 interior used by the bench-of-record | As D1 | `bench:` line printed and recorded; no device loss. |
| M1 | macOS, Apple silicon, MoltenVK installed | none | `byro-launcher` preflight; `byroredux --cell …` | Preflight shows `MissingExtension` naming an RT extension (XPLAT-MAC-002); the engine exits non-zero with the named reason (XPLAT-MAC-003). |
| U1 | Linux (control) | none | `cargo test -p byro-launcher` and `cargo test -p byroredux-renderer` with XPLAT-MAC-001 applied | Existing preflight and device-selection tests pass unchanged; the `Suitable GPU:` and launcher `summary` lines on the RT box are unchanged (XPLAT-MAC-006). |

---

## 8. Metrics

The numbers live in ROADMAP: Phase 0 adds a "Platform matrix" table beside
the [compatibility matrix](../../ROADMAP.md#compatibility-matrix); this
document only defines what is counted.

| Metric | Definition | Granularity |
|---|---|---|
| Lane green rate | `main` commits on which the platform's hosted lane passed ÷ `main` commits | per platform, per month |
| Skipped-test share | tests skipped by `cfg` or probe on the platform ÷ tests in `cargo test --workspace` | per platform, per release |
| GPU acceptance freshness | commits between the release and the platform's last passing W1 / D1 run | per platform, per release |
| Deck frame time | median and 99th-percentile frame time of D1 and D2, same method as the bench-of-record | per release |
| Platform-boundary violations | production `cfg` branches outside the boundary (XPLAT-PORT-001) | per commit (test) |

---

## 9. Delivery sequence

ROADMAP places M70 in Tier 10, which has "no active work until the engine
ships something playable"
([Tier 10](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon)).
Phase 0 needs no engine code; whether Phase 1 may start before that gate is
Q5.

**Phase 0 — Survey and decisions (no engine code).** Resolve §10. Write
`docs/engine/platform-ci.md` (the CI and release pipeline per platform,
the macOS re-review check of XPLAT-MAC-005). Close every **Phase 0: verify**
item: the Windows GPU larger runner's device (XPLAT-CI-007), the Steam
registry value (XPLAT-WIN-006), GOG and Bethesda keys per title
(XPLAT-WIN-007), SteamOS glibc and libraries (XPLAT-DECK-001), the Deck's
extension and feature list (XPLAT-DECK-002), Game Mode windowing
(XPLAT-DECK-004), the Deck's heap size (XPLAT-DECK-006). The Windows
change-time check of XPLAT-PORT-007 needs the Windows lane, so it closes in
Phase 2 and is filed as an issue in Phase 0. Record
who supplies the Windows RT GPU and the Deck (Q2, Q3). Add the empty
Platform matrix table to ROADMAP. Exit: every verify item is closed with a
cited answer or filed as an issue, every §10 item carries **Decided**, and
`platform-ci.md` is checked in.

**Phase 1 — Windows builds and tests.** XPLAT-CI-001, XPLAT-CI-003,
XPLAT-CI-004, XPLAT-CI-005, XPLAT-CI-008, XPLAT-WIN-001 to XPLAT-WIN-004,
XPLAT-WIN-010, XPLAT-PORT-001, XPLAT-PORT-002, XPLAT-PORT-003,
XPLAT-PORT-008. Exit: scenario H1 passes on `main` and the lane green rate
over the following month is published.

**Phase 2 — Windows at runtime.** XPLAT-WIN-005 to XPLAT-WIN-009,
XPLAT-WIN-011, XPLAT-PORT-005, XPLAT-PORT-007, the instance change of
XPLAT-MAC-001 with XPLAT-MAC-006 (it is shared code and U1 proves it
neutral). Exit: scenarios W1, W2 and U1 pass and their record is published
in the Platform matrix.

**Phase 3 — Steam Deck.** XPLAT-DECK-001 to XPLAT-DECK-006. Depends on the
launcher presets of [launcher.md §4.3](../engine/launcher.md#43-presets-and-hardware-pre-flight).
Exit: scenarios D1 and D2 pass and the Deck frame time is published.

**Phase 4 — macOS per Q1.** Under the recommendation: XPLAT-CI-002,
XPLAT-MAC-002, XPLAT-MAC-003, XPLAT-MAC-005. Exit: scenarios H2 and M1
pass.

**Phase 5 — Release artefacts.** XPLAT-REL-001 to XPLAT-REL-004. Exit: one
tagged release carries the Windows zip, both checksums, and release notes
naming each platform's tier and last acceptance commit.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — What macOS is.** No Vulkan-on-Metal driver passes the RT gate
   (§1). Recommended: **P3 build-only**, with the honest refusal of
   XPLAT-MAC-002/003 and a re-review trigger (XPLAT-MAC-005), because it
   keeps the code compiling for the day MoltenVK (#427) or KosmicKrisp ships
   ray queries, at the cost of one hosted lane. Alternative A: **drop macOS
   to P4** until a driver qualifies; cheapest, and the M70 row's "macOS via
   MoltenVK" is amended. Alternative B: **a rasterized fallback renderer**;
   ROADMAP's keep list reads "RT-first, rasterized fallback"
   ([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)),
   but HEAD has no fallback (#3759, #4894), so this is a second renderer
   and shader set to keep in step, and it would also open non-RT PC GPUs.
   Alternative C: **a Metal backend**; Apple silicon from M3 has
   hardware-accelerated ray tracing
   ([Apple, 2023](https://www.apple.com/newsroom/2023/10/apple-unveils-m3-m3-pro-and-m3-max-the-most-advanced-chips-for-a-personal-computer/)),
   but a second graphics API contradicts XPLAT-NG-003 and doubles every
   renderer change. Either B or C also needs the keep list and CLAUDE.md's
   "Vulkan done properly" invariant revisited explicitly. Every option
   amends the M70 row's "macOS via MoltenVK": under the recommendation it
   reads "macOS build-only until a Vulkan-on-Metal driver exposes ray
   queries".
2. **Q2 — How binding the Windows lane is, and who runs Windows GPU
   acceptance.** Recommended: the hosted Windows lane is **required for
   merge** once Phase 1 exits, and W1 runs once per release on a Windows RT
   machine the owner names in Phase 0. Alternative: the lane stays
   **advisory**, which keeps Linux iteration speed unaffected by Windows-only
   breakage and lets Windows rot between releases; consequence: P2 is
   promised on weaker evidence and XPLAT-G-005 counts only failures, not
   blocks. The recommendation is in tension with the anti-scope's "Windows +
   macOS are downstream": a required lane means a Windows-only break blocks
   a Linux merge, and the owner should accept that explicitly. Second
   alternative, additive to either: pay for a **Windows GPU larger runner**
   (Tesla T4, Team or Enterprise Cloud plan, §2) once XPLAT-CI-007 shows it
   passes the RT gate; consequence: per-commit Windows GPU evidence instead
   of per-release, at a recurring cost, and a runner well below the
   4070 Ti the renderer is scoped to.
3. **Q3 — The Steam Deck promise.** Recommended: **"runs" with a published
   frame time and no target**, because the Deck is far below the RTX 4070 Ti
   the renderer is scoped to (keep list) and a target would force a Deck
   preset into the renderer budget. Alternative: a frame-time target on D1,
   for example Valve's own Verified bar, a default configuration that runs
   at "30fps at 800p"
   ([Steam Deck compatibility review](https://partner.steamgames.com/doc/steamdeck/compat)),
   which makes the Deck a renderer design input and needs a PRD of its own
   for the preset. Either way Phase 0 records
   whether a Deck is available; without one the Deck stays at P2 "expected,
   unmeasured".
4. **Q4 — Windows distribution.** Recommended: an **unsigned zip** on the
   GitHub pre-release, beside the Linux tarball, with checksums. Alternative:
   no Windows artefact (contributors build from source); consequence: P2
   means "builds" for players. A signed build needs a paid certificate and is
   excluded by XPLAT-NG-008 unless the owner reverses it.
5. **Q5 — Timing against the Tier 10 gate.** ROADMAP says Tier 10 has "**No
   active work** until the engine ships something playable for the existing
   community to mod"
   ([Tier 10](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon)),
   and its anti-scope says "Linux-first. Windows + macOS are downstream if
   they happen" ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
   Recommended: **keep the gate for Phases 2–5**, but allow Phase 0 and
   Phase 1 (hosted Windows build and tests) now, because a build lane
   catches the cheap portability rot early (§3 rows 7, 9) and costs no
   engine work. This is an amendment of ROADMAP, not a reading of it:
   approving it means adding to the Tier 10 intro an explicit exception
   ("except M70 Phases 0–1: survey, decisions and a hosted Windows build
   lane"), and the anti-scope bullet keeps its wording because P2 is still
   downstream of Linux (XPLAT-NG-002). Alternative: hold every phase
   behind the gate as ROADMAP is written; consequence: the first Windows
   build happens against a much larger tree, and this PRD stays PROPOSED
   or APPROVED-but-dormant until the gate opens.
6. **Q6 — Windows toolchain.** Recommended: **MSVC**
   (`x86_64-pc-windows-msvc`): Tier 1 for both Rust and wasmtime (XPLAT-WIN-001)
   and FidelityFX's native compiler. Alternative: **MinGW**
   (`x86_64-pc-windows-gnu`), which `fsr3-sys`'s portability header already
   anticipates but wasmtime rates Tier 2 ("missing … clear owner of the
   target", [stability tiers](https://docs.wasmtime.dev/stability-tiers.html));
   consequence: a GCC-family build on all platforms, and a weaker runtime
   guarantee for the mod sandbox.

---

## 11. Document map

| Need | Go to |
|---|---|
| Windowing, raw handles, platform paths, input abstraction | [platform.md](../engine/platform.md) |
| Install detection, preflight blockers, settings and profiles paths | [launcher.md](../engine/launcher.md) |
| Vulkan init chain and device requirements | [renderer.md](../engine/renderer.md#initialization-chain) |
| FidelityFX native build and its portability patches | [fsr3-upscaler-integration-plan.md](../engine/fsr3-upscaler-integration-plan.md) |
| C++ bridge build | [cxx-interop.md](../engine/cxx-interop.md#build-configuration) |
| Native and build-time dependencies | [dependencies.md](../engine/dependencies.md#build-time-deps) |
| CI jobs and prerequisites | [contributing.md § CI](../contributing.md#ci) |
| Loose-file case folding | [archives.md](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| RT VRAM minimum | [memory-budget.md](../engine/memory-budget.md) |
| Per-game tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| Bench method and the compatibility matrix | [ROADMAP.md](../../ROADMAP.md#how-to-run-a-bench) |
| Anti-scope, keep list, Tier 10 gate | [ROADMAP.md](../../ROADMAP.md#what-we-are-not-doing-anti-scope) |
| Cross-target noise equality that rides on the M70 lanes | [procedural-exteriors.md](procedural-exteriors.md) (PGEN-GEN-009) |
| Bake staleness on Windows | [asset-preprocessing.md](asset-preprocessing.md) (BAKE-INV-004) |
| VR runtime scope (Linux) | [openxr-vr.md](openxr-vr.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: platform-boundary HEAD claim (`ui_input.rs`), `\\?\` prefix defect re-measured against std source, GPU larger runners and lavapipe CPU-device rejection, silent RT-extension rejection (XPLAT-MAC-003), portability-subset VUID and Windows file-identity verify items settled, wasmtime-tier consistency, sibling PRDs (PGEN-GEN-009, BAKE-INV-004, OpenXR scope), one observable per ID (new CI-008, WIN-010, WIN-011, MAC-006, PORT-008, NG-009), ROADMAP amendments named in Q1/Q2/Q5 | claude: revise |
