# #7036: M48: Fallout 4 vanilla HUD compass does not follow the heading (no ActionScript to drive)

Split out of #6661 (M48 Phase 2), which carries GUI-HUD-005 for Skyrim only. Part of #6641; PRD [`game-ui-integration`](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md) GUI-HUD-005.

## Problem

GUI-HUD-005 asks for the vanilla Fallout 4 compass to follow the camera heading. Phase 0 (#6643) found the widget cannot be driven from the shipped movie, and Phase 2 confirmed the path service can't reach it:

- `BottomCenterGroup_mc.CompassWidget_mc` (and `PowerArmorHUDMenu`'s copy) in `interface\hudmenu.swf` (`Fallout4 - Interface.ba2`) has **no ActionScript**: no class, no setter, no method. The heading strip and the quest / other marker clips (`QuestMarkerHolder_mc`, `OtherMarkerHolder_mc`) are positioned by native code (`docs/data/game-ui/hud-feeds-fallout4.tsv`, rows marked `compass`, all `inferred:no-movie-writer+exe-strings`).
- So no `__byro_path_get/set/call` operation can turn it: there is no member to set. Skyrim's compass works because its movie exposes `SetCompassAngle`.

## Options

1. **Engine-drawn compass** over the movie's compass slot: read the slot's bounds through the path service (`BottomCenterGroup_mc.CompassWidget_mc.x/y/width/height`) and draw the heading strip and markers natively, in the HUD's colour. The look would be an engine approximation of the original.
2. **Move the widget's children directly** through the path service: write `x` on the strip / marker clips as the native code does. This needs the strip's art width and the marker conventions measured from the movie (the native layout rule is undocumented).
3. Leave it static and say so in the ROADMAP's HUD liveness line (today's state).

## Done when

With `--hud` on Fallout 4 (MedTekResearch01), the compass follows `cam`'s heading (`hud.heading 90` vs `hud.heading 270` moves it), gated by a pixel diff in `m48-fo4-hud-meters.sh` or a sibling, and the ROADMAP M48 liveness line is updated.

