# #6905 KillActor on the Oblivion player does nothing once a few seconds have passed since the cell loaded

Found while running `docs/smoke-tests/music-oblivion.sh` for #6903 / #6901.

**Repro** (`--cell BrumaJGhastasHouse`, headless, `--bench-frames 30 --bench-hold`, release build):
- `obscript.cmd 0x14 108b` (`KillActor` on the player) sent right after the cell loads, or after the gate's OBSE and first two `StreamMusic` legs: the player gains `Dead` and the death sting plays — 7 of 7 runs.
- The same command after a further `sleep 2` (no other command in between): no `Dead`, no sting, the command answers `0x108b -> (no answer)` as in the passing runs — 5 of 5 runs. Before #6903 added a one-second leg, the gate's kill landed near this boundary and failed about 3 in 10 runs.

So whether the kill lands depends on the time since the load, not on the music commands.

**Ruled out:** `scripted_kill_system` (`byroredux/src/combat.rs`) never spares the player (`spare_from_death` returns false for the player), and the music system is not involved: the Script layer the kill would have ended keeps playing, i.e. `Dead` was never inserted.

**To look at:** what changes a couple of seconds after the bench frames — whether `open_actor_effects` stops resolving `0x14`, or the effect is queued but `PendingScriptKills` is never drained (e.g. the system stops running under `--bench-hold`). The Fallout 3 gate's player kill (`music-fo3.sh`, much later after its load) works, so this looks Oblivion-specific or timing-specific.

The Oblivion music gate now runs its death-sting leg before the `StreamMusic` legs so it does not depend on this.
