# #6254: LC-2026-10-21-D2-02: ObScript `PlayGroup` swaps to a bare sequence clip — its non-transform channels have no sinks, and the standalone effects stop

**Labels**: medium,bug,legacy-compat,animation,scripting

**Source**: docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 2 — Animation model (`NiControllerManager` sequence activation) · **Location**: `crates/scripting/src/obscript_anim.rs:72-88` (`play_group`).; `byroredux/src/cell_loader/spawn.rs:899-922`: sinks are attached for the spawn clip only, then `NamedSequences` is inserted.; `byroredux/src/cell_loader/object_sequences.rs` (`named_sequences_for`).; Commit `f3380f6c8` (2026-10-18).

- **Severity**: MEDIUM. This is the newest consumer of the legacy sequence model, and it silently discards authored visibility, alpha and UV animation on the content it targets (Oblivion gate open/close, FNV terminals and holograms).
- **Dimension**: 2 — Animation model (`NiControllerManager` sequence activation)
- **Location**:
  - `crates/scripting/src/obscript_anim.rs:72-88` (`play_group`).
  - `byroredux/src/cell_loader/spawn.rs:899-922`: sinks are attached for the spawn clip only, then `NamedSequences` is inserted.
  - `byroredux/src/cell_loader/object_sequences.rs` (`named_sequences_for`).
  - Commit `f3380f6c8` (2026-10-18).
- **Status**: NEW
- **Description**: `play_group` looks up the named sequence's handle and writes it into the placement's existing `AnimationPlayer.clip_handle`, resetting time to 0. Two legacy behaviours do not survive the swap:
  1. **Missing sinks.** `animation_system` can only write into an `Animated*` component that already exists (`anim_convert.rs` #2221 contract). The sinks were attached at spawn for the spawn clip's channels only. A named sequence that animates a target or channel kind the spawn clip doesn't therefore resolves and samples its value, then throws it away.
  2. **Standalone effects stop.** The spawn clip is the standalone-controller clip merged with the first sequence (see D2-01). The named clip is the sequence alone. After the swap, every standalone property controller (flipbooks, UV scrolls, emitter visibility) stops on whatever value it last wrote, although in Gamebryo those controllers run independently of the manager.
- **Evidence** (probe `lc-seq-probe`, multi-sequence object NIFs: FNV 453, Oblivion 214):
  - Sequences with sinks missing: FNV 2 NIFs, Oblivion 7.
    - FNV `furniture\tlpod01.nif` `Idle` (`Monitor:8` UV offset and scale) and `dlc05\effects\dlc05alienhologramhead.nif` `Forward`/`Backward`/`Left`/`Right` (`newHead01yellowRim` alpha).
    - Oblivion `obliviongate_forming.nif` `Forward`/`Backward`/`Equip`/`Unequip` (`CollapsingEnergyPlane` visibility, `FlashPlane:0` alpha, `GateBladeLeft` visibility), `obliviongate_simple.nif` `Forward`/`SpecialIdle`, and `magiceffects\shockshield.nif`.
  - Sequences that drop standalone channels the spawn clip plays: FNV 1, Oblivion 5. Examples are the `Tendrils02..04:0` flipbooks on `obliviongate_forming`, `oblivionarchgate01` and `obliviongate_simple`, and the `MagmaEmitter02-04` visibility on `oblivionwargateani02`.
  - Script usage: in the ESM text, `playgroup forward 0|1` appears 427 times in `Oblivion.esm` and 236 times in `FalloutNV.esm`, and `Forward` is index 3, inside the Oblivion limit of 5 that `group_name` allows.
- **Impact**: Once an object script plays a group, these effects freeze for the rest of the session:
  - The flash and blade visibility an Oblivion gate's open animation authors never applies.
  - The gate's tendril flipbooks stop.
  - A terminal pod's monitor scroll and the alien hologram's rim fade stop.

  The `IsAnimPlaying` answer stays correct; only the visuals are wrong.
- **Related**: D2-01 (the same "one clip for two animations" model), #4128 (actor-side KFM state machine), #2221 (sink contract).
- **Suggested Fix**:
  - When `named_sequences_for` registers a model's sequences, call `attach_clip_sinks` for *each* sequence handle on that placement. Sinks are insert-if-missing, so this is idempotent.
  - Run sequences on their own player (or stack layer) so that `PlayGroup` replaces only the sequence and leaves the standalone-controller player running.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
