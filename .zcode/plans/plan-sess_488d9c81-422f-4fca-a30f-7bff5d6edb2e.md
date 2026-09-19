# M48.5 — Game-agnostic MenuXml UI profile + Fallout 3 HUD

**Premise (verified on disk + repo record):** FO3/FNV UI is Bethesda MenuXml — same family as Oblivion — not Scaleform (`Fallout - Misc.bsa` carries `menus\main\hud_main_menu.xml` + ~130 XMLs; art in `Fallout - Textures.bsa` under `textures\interface\hud\`; R4 decision `ROADMAP.md:883` agrees). The work: extract the per-game knobs out of the M48.4 Oblivion HUD into profiles, then wire FO3 as the second consumer with its own corpus test and smoke gate.

## Step 0 — FO3 corpus probe (read-only, drives everything below)

New env-gated test `crates/menuxml/tests/fo3_corpus.rs` (pattern: existing `vanilla_corpus.rs`, silently skips unless `BYROREDUX_FO3_DATA` is set; default path `/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data`). It answers, with assertions so the facts stay pinned:

- `menus\main\hud_main_menu.xml` parses non-empty; tile names for the HP/AP/XP bars, compass, ammo counter; which `<font>` indices the text tiles use; what shape `<filename>` paths take (expected: `textures\interface\…` or `interface\…`, i.e. the existing passthrough arm — `TextureSet` then stays Oblivion-scoped).
- Where FO3 `.fnt` fonts live (probe `Fallout - Misc.bsa` and `Fallout - Textures.bsa` file tables) and whether `Font::parse`'s 296-byte-header / 256×14-f32 layout holds for them; how the glyph atlas is named (Oblivion derives `fonts\<name>.tex` from bytes at offset 12 — FO3 may differ).
- The format of `menus\falloutdict.txt` (FO3's strings/dictionary — the analog of `menus\strings.xml`) → decides the strings-source handling.
- That the HUD menu's referenced textures resolve in `Fallout - Textures.bsa`.

## Step 1 — `crates/menuxml`: per-game `MenuProfile`

- Add `MenuProfile` (struct of data + small fn pointers, no new deps): font table (`font_paths: &'static [&'static str]`), atlas-path derivation (`font_atlas_path: fn(&[u8]) -> Option<String>`; Oblivion = name@offset-12 → `fonts\<name>.tex`), and strings source (path + kind: `strings.xml` vs `falloutdict.txt`). Constructors `MenuProfile::oblivion()` / `::fallout3()` own these tables **once** — collapsing today's `FONT_PATHS` triplication (`byroredux/src/hud.rs:24`, `examples/render_hud.rs`, `tests/vanilla_corpus.rs` all switch to referencing the profile).
- `MenuRenderer::load_with_profile(assets, menu_path, screen, profile)`; existing `load()` delegates with `MenuProfile::oblivion()` so current call sites and corpus tests don't churn.
- Texture resolution in `menu.rs::texture()` stays as-is unless the probe shows FO3 needs a remap (the `textures\`-prefixed passthrough arm likely covers FO3's paths already).
- If the probe finds FO3 `.fnt` layout differs, extend `font.rs` behind the profile rather than forking the crate.
- Unit tests with a synthetic FO3-shaped profile (odd font indices, dict-as-strings) via the existing `MapSource`/`SynthAssets` fakes.

## Step 2 — engine: game-parameterized HUD driver (`byroredux/src/hud.rs`)

- New `HudGameProfile { label, misc_bsa, default_textures_bsa, menu_profile, bar_labels: [&'static str; 3], av_keys: [u32; 3], bar_overrides: [(tile, trait); 3], compass_override, mode_override }` with `oblivion()` + `fallout3()` (an `fallout_nv()` twin is nearly free — include the table, no separate smoke).
- `hud_archive_args` becomes game-aware: discover `Fallout - Misc.bsa` vs `Oblivion - Misc.bsa` beside `--esm` (existence probe, matching today's discovery-walk style); texture default `Fallout - Textures.bsa` vs `Oblivion - Textures - Compressed.bsa`; `--hud-textures` override unchanged. Error messages name both candidates.
- `OblivionHud` → `MenuXmlHud` — triple-buffer rotation, change-signature skip, 33 ms cadence cap, and `HudAssets` all untouched; `render()` pushes overrides through the profile's vocabulary instead of the hardcoded `hudmain_*` names (`hud.rs:309-316`). Compass heading formula stays shared (same Z-up Gamebryo import convention; verify sign against probe output).
- Actor values: `bar_fractions`'s hardcoded Skyrim keys (`hud.rs:36-38`) move into `av_keys`; FO3 keys come from the existing FO3 actor-value tables (cross-check `commands/actor_value.rs` / `crates/core/src/character/profile.rs`); the pinned/full-bars fallback behavior is unchanged.
- `HudControl` bar fields → `[Option<f32>; 3]` with per-game labels surfaced in `hud.status`; `hud.values <a> <b> <c> | auto` CLI shape unchanged. `commands/hud.rs` descriptions/status wording become profile-labeled. Smoke-gate greps preserved (`hud: loaded …`, `hud: launched visible=`).
- `app_frame.rs`'s `tick_hud_overlay` branch keeps its current shape (the `crates/ui` tests pin `app_frame.rs` textually via `include_str!` — the SWF branch must stay byte-identical).

## Step 3 — smoke gate `docs/smoke-tests/m48-5-fo3-hud.sh`

Clone of the m48-4 procedure against the FO3 install (fixture args from `docs/smoke-tests/fixtures/fo3.env`, Megaton cell): engine + `--hud` + `--bench-hold` under `xvfb-run`, boot-line gate, `hud.status` gate, two pinned captures via `byro-dbg` (`hud.values 1 1 1` vs a partial pin), five-filter PNG decode and a fill-run classifier over the FO3 HP-bar rows (row coordinates and color thresholds taken from Step 0 probe renders — FO3 bars are solid fills, so the classifier is simpler than Oblivion's ornament-guarded red-run). README row + SKIP-77 when data is absent.

## Step 4 — docs + verification

- `ROADMAP.md` M48.5 milestone row; `docs/engine/ui.md` legacy-UI section updated (FO3 column: MenuXml profile wired, per-game knob table); `AGENTS.md` workspace-tree line for `hud.rs`; HISTORY.md entry at `/session-close`.
- Verification: `cargo test -p byroredux-menuxml` (both corpus tests with envs set); engine bin via the 1.96.0 toolchain path (`TC=$(rustup which --toolchain 1.96.0 cargo); PATH=… "$TC" test -p byroredux --bin byroredux` — AGENTS.md gotcha); **m48-4 Oblivion smoke re-run green** (the regression gate for the refactor), then m48-5.

**Non-goals:** any SWF/Scaleform path for FO3 (contradicts on-disk data and R4); unifying the loading-screen/HUD/SWF overlay producers behind a trait (they already agree on `Option<u32>`; the `include_str!`-pinned SWF branch makes that churn risky for no user-visible gain); VATS/dialog/inventory menus and the FO3 menu stack beyond the HUD (later M48.x slices, now cheap thanks to the profile); a separate FNV smoke gate.

**Risks:** FO3 `.fnt` layout/atlas naming may differ from Oblivion's (probe first, extend behind profile); `falloutdict.txt` format unknown until probed; FO3 AVIF keys need verification against the plugin records; if FO3 fonts fail the probe, ship bars+compass first (text tiles already degrade gracefully via the `None`-slot skip) and note the gap.