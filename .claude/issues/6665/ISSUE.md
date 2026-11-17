# #6665: M48 Phase 3: Wave 1 (Skyrim, FNV, FO3) — host behaviour, extensions, text, MenuXml menus

Part of #6641. PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md#9-delivery-sequence). Depends on Phase 1 (#6653: the registry every later requirement reports through) and comes after Phase 2 (#6661: the path service, proven on both VMs); Phase 0 (#6643) supplies the classified catalog, the extension-member measurement, the Q5 font and the name of the MenuXml exit test. Wave 1: Skyrim (Scaleform) and FNV with, through the shared Fallout `MenuProfile`, FO3 (MenuXml). The [VATS PRD](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/vats.md)'s menu phase depends on GUI-XML-001…003 here on FNV and FO3. A*N* = §7.2 scenario *N*.

GUI-OPS-002, GUI-OPS-003, GUI-OPS-004 and GUI-HUD-006 are named by no §9 phase; they are placed here (each bullet says why).

## Acceptance Criteria
- [ ] GUI-HOST-001 (Skyrim): every catalog entry carries its behaviour class H1–H4 in `crates/ui/src/catalog.rs`, next to `provenance` (A7)
- [ ] GUI-HOST-002 (Skyrim): each H1 method acts through the canonical API the native page for that menu uses; one test per H1 action drives it once through the host method and once through the native page's control and asserts the same resulting ECS state
- [ ] GUI-HOST-003 (Skyrim): every H2 answer names its reason in the catalog; online-service methods are H2 with "anti-scope: hosted services"
- [ ] GUI-HOST-004 (Skyrim): an H3 or H4 call produces one attributed diagnostic per (menu, method) naming the class and is never answered with a fabricated value
- [ ] GUI-HOST-005 (Skyrim): response handlers answer from canonical state on the frame asked; the Scaleform HUD driver's poll handlers move behind the general handler table
- [ ] GUI-HOST-006 (Skyrim): per-game host-protocol differences (Skyrim's re-entrant `GameDelegate` request IDs, Fallout 4's `BGSCodeObj`) stay inside the host profile; a handler is written once against the logical method
- [ ] GUI-GFX-001: the AVM1 profile provides the Scaleform AS2 extension members vanilla movies read, enabled by `_global.gfxExtensions = true`
- [ ] GUI-GFX-003 (Skyrim): only the extension members Phase 0 measured are built, each H1 or H2
- [ ] GUI-GFX-004 (Skyrim): an extension member a movie reads that the profile lacks produces one diagnostic per (menu, member)
- [ ] GUI-FONT-001 (Skyrim): the profile resolves font classes from the game's font configuration (file and format per Phase 0's verify) to the font libraries it names
- [ ] GUI-FONT-002 (Skyrim): the font library is the one the active language installs
- [ ] GUI-FONT-003 (Skyrim): a font class or glyph that does not resolve falls back to the Q5 bundled font, with one diagnostic per (menu, class)
- [ ] GUI-FONT-004 (Skyrim): Scaleform `$`-token text resolves from the game's vanilla translation tables
- [ ] GUI-EVT-003 (Skyrim): script functions that open a menu open it through the registry under the inventory's game-facing name and surface, or report S4 with the fallback the effect already uses
- [ ] GUI-EVT-004 (Skyrim): a host call that changes game state reaches the ECS as the same event or command the native page emits for that action, applied after the UI frame
- [ ] GUI-XML-001 (FNV, FO3): the MenuXml track loads any menu document through the same `MenuProfile`, not only `hud_main_menu.xml`; which menus become S2 follows Q1
- [ ] GUI-XML-002 (FNV, FO3): MenuXml input (pointer hit-testing over tile rects, keyboard navigation) routes through `UiInputEvent` (`crates/ui/src/input.rs`), under the GUI-REG-009 input-owner rule
- [ ] GUI-XML-003 (FNV, FO3): values a MenuXml menu reads from the engine are set through `MenuRenderer::set_override` / `instantiate_template` / `graft_fragment`
- [ ] GUI-FONT-005 (FNV, FO3): `-s`-prefixed strings resolve through the plugin's game settings, as Phase 0's verify settles them
- [ ] GUI-OPS-002 (no §9 phase; A7, this phase's exit scenario, names it): `ui.host <menu>` reports for one open menu the count of calls per behaviour class and the distinct H3 / H4 method names, bounded by `MAX_DISTINCT_HOST_METHOD_NAMES` (A7)
- [ ] GUI-OPS-003 (no §9 phase; this phase's exit publishes UI frame cost): UI cost per frame (Ruffle tick + render + readback + upload, plus MenuXml raster) is measurable per open menu, published under the ROADMAP bench methodology (same-machine control, pinned camera)
- [ ] GUI-OPS-004 (no §9 phase; placed with GUI-OPS-003's cost measurement): a static menu costs no raster work
- [ ] GUI-HUD-006 (no §9 phase, Phase 2 takes GUI-HUD-001…005; placed with the FNV / FO3 MenuXml profile work): the FO3 / FNV HUD art is tinted by the game's HUD colour (the `&hudmain;` default Phase 0 verifies) instead of art-native white
- [ ] A7 (Skyrim): an ignored corpus test in the `installed_skyrim_host_calls_are_all_cataloged` pattern asserts every method the vanilla archive calls has a class and no H4 remains; `ui.host` reports the same counts live (GUI-HOST-001, GUI-OPS-002)
- [ ] The corpus-gated `cargo test` the Phase 0 design names passes: a non-HUD FNV and FO3 menu document loads and takes pointer input through `UiInputEvent` (GUI-XML-001, GUI-XML-002)
- [ ] Host behaviour coverage, placeholder share and UI frame cost published for Skyrim and FNV in their §8 ROADMAP homes
- [ ] A1: the regression floor passes unchanged

**Exit:** scenario 7 passes for Skyrim; a non-HUD FNV and FO3 menu document loads and takes pointer input through `UiInputEvent` in a `cargo test` the Phase 0 design names (corpus-gated like the existing `fo3_corpus.rs`); host behaviour coverage, placeholder share and UI frame cost are published for Skyrim and FNV.

