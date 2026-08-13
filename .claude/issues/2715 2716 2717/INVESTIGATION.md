# #2717 investigation notes

Widening the corpus sweep test (per the issue's "failing that" fallback) surfaced a
**real, separate, pre-existing bug**, unrelated to #2717's round-trip-serialization
concern: 4 real Fallout 4 menus fail `inject_host_object_adapter` entirely today —

- `interface\dialoguemenu.swf`
- `interface\multiactivatemenu.swf`
- `interface\specialmenu.swf`
- `interface\terminalmenu.swf`

`ScaleformHostCatalog::for_profile(Fallout4Avm2).host_object()` requires a lifecycle
class whose **own** ABC traits include the host-object property (`BGSCodeObj`) AND
**both** hooks (`onCodeObjCreate` AND `onCodeObjDestruction`). Diagnosed via a
throwaway test dumping each menu's root class' own trait list against the real
`Fallout4 - Interface.ba2` corpus (data present in this environment at
`/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data`):

- `DialogueMenu` traits: `[..., "BGSCodeObj", ..., "onCodeObjCreate", ...]` — **no**
  `onCodeObjDestruction`.
- `MultiActivateMenu`: same shape, also missing `onCodeObjDestruction`.
- `SPECIALMenu`: same shape, also missing `onCodeObjDestruction`.
- `TerminalMenu_fla.MainTimeline` / `Terminal`: same shape.

This is legal, shipped ABC — these menus' native lifecycle apparently never needs an
explicit destroy hook (or it's inherited/handled differently), but
`inject_host_object_adapter`'s class search treats its absence as "this isn't the
lifecycle class" and falls through to `Err("... lifecycle class was not found")`.
Every caller (`SwfPlayer::new` / `new_with_profile` / `from_resource_provider` in
`player.rs`) propagates that `Err` via `?` — so these 4 menus **fail to load
entirely** in the current engine, not merely lose the host-object bridge.

**Not fixed here** — out of scope for #2717 (which is specifically about
`parse_swf`→`write_swf` losslessness for menus that DO get patched successfully).
The new corpus-sweep test (`all_installed_fallout4_swfs_round_trip_through_injection`
in `crates/ui/src/avm2_host.rs`) names these 4 paths explicitly via
`KNOWN_MISSING_ON_DESTROY_TRAIT` and asserts the set doesn't silently grow or shrink,
so a future fix (or regression) is caught rather than the test just quietly excluding
them forever.

**Recommended follow-up** (not filed as an issue by this pass — flagging for the
user to decide): investigate whether `ScaleformHostCatalog`'s on-destroy requirement
should be optional, or whether the class search should also check inherited traits
(these classes' bases — `Shared.IMenu` — don't declare it either, per the same dump,
so it's not a same-file-inheritance gap; it may be a native-code convention where
destroy is genuinely optional for certain menu classes).
