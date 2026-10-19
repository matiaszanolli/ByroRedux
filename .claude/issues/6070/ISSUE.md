# #6070 (snapshot as filed)

Split from #5015.

**Split from #5015** (verification gap, no code defect known).

## Description
The FO4 ragdoll decode (`e1bd40bcf`) and the human/feral-ghoul `XRGD` corpse poses were verified against the vanilla data files only (unit tests, corpus consistency checks, forward kinematics on the real NIF bone tree). No engine was launched: nobody has seen an FO4 NPC ragdoll in the running engine, or an authored FO4 corpse lie in its pose.

## What to check
1. An FO4 human dies (console `kill` / combat): `activate_ragdoll` builds the 18-body multibody without the seed-pose rejection (#5161), limits hold, no explosion; the same for a feral ghoul and one creature.
2. An authored corpse (e.g. a Starts-Dead raider in a Commonwealth interior) lies slumped as authored rather than falling from the spawn pose; the log line `posed 18 bones from XRGD` appears.
3. `BYRO_VALIDATION=1` clean, and no hitch at death.

## Method
A smoke script in the `docs/smoke-tests/` family (`--bench-hold` + `byro-dbg` attach), as `m47-2-bleedout.sh` does for Skyrim: kill an FO4 NPC and read the ragdoll component and bone poses; screenshot a placed corpse. Needs a Vulkan device and FO4 data (out of `cargo test` scope).

## Related
#5015, #5161, `docs/smoke-tests/README.md`.

## Completeness Checks
- [ ] **TESTS**: the smoke script is committed and its result recorded
