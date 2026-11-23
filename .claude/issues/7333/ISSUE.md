# #7333: Console FormID arguments still parse decimal-first outside the three #5817 sites (weather.force refuses a bare 0010E1F2)

Labels: low, tech-debt, bug

`weather.force` parses its WTHR FormID with `parse_console_u32` (`byroredux/src/commands/quest.rs`, `WeatherForceCommand::execute`), which reads a bare token as decimal. A bare console FormID such as `0010E1F2` (Skyrim's SkyrimClear_A) is refused with `bad WTHR FormID`, and only `0x0010E1F2` works. A bare `10` would address form 10 decimal, where `inv.add 10` addresses form 0x10.

#5817 (`e634fbf9b`) added `parse_console_form_id` (hex, with an optional `0x`/`0X` prefix) and documented in `commands/shared.rs` that "every FormID-shaped argument goes through this one parser". Its fix routed only the three gameplay commands. The other FormID arguments still parse decimal-first.

**Found by:** the TSD-4.3 coarse-only comparison (`docs/engine/terrain-displacement.md` §8.8). `weather.force 0010E1F2 lock` was refused while pinning Riverwood's weather.

## Remaining decimal-first sites
From `rg -n 'parse_console_u32\(' byroredux/src`. Each one needs classifying: a FormID moves to `parse_console_form_id`, while a plain number (stage index, actor-value index, flag bits) stays decimal.
- `commands/quest.rs`: the shared helper at line 46, plus `quest.*` FormIDs (280, 624, 898), the reference and item pair (955/958), the perk and entry pair (1003/1007), the package (1224), the form values (1323/1335/1356), and `weather.force` (1452). The stage at line 901 is a number.
- `commands/cndf.rs:32`, `commands/condition.rs:87`, `commands/anim_archetype.rs:51` (keyword), `commands/script_var.rs:161` (quest).
- `commands/actor_value.rs:70` and `commands/world_info.rs:1213`: probably plain numbers. Check them.

## Fix
- Now: route `weather.force` through `parse_console_form_id`, with a regression test. Every in-repo use passes `0x…` (`docs/smoke-tests/m-exteriors.sh`, the `lod.rs` doc), so nothing changes for them.
- Sweep: classify the rest and route the FormID ones. Smoke scripts and docs that pass a bare decimal FormID would need updating; check with `rg` before the switch.

## Completeness Checks
- [ ] **SIBLING**: every FormID argument in `byroredux/src/commands/` is classified
- [ ] **TESTS**: a regression test pins the bare-hex reading
- [ ] **DOCS**: `docs/engine/debug-cli.md` states that console FormIDs are hex
