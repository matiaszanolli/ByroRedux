# #2567 #2568 #2569 #2570 — Oblivion audit closeout (2026-08-14)

Four findings from the Oblivion per-game audit. **Two were fixed, one was
measured false, one is deliberately half-landed.**

## #2567 (OBL-D3-01) — MEDIUM, fixed
Placed creatures never reached the actor pipeline: `NPC_` and `CREA` parse into
two disjoint maps and every actor test in the cell loader read only `npcs`.

The audit's suggested fix — "thread `index.creatures` alongside `index.npcs`,
extend actor-detection to check both" — was verified against real data and is
**wrong on its own**. Dumping `Oblivion.esm`:

    MODL = "Creatures\Rat\Skeleton.NIF"        <- the SKELETON, not a body
    NIFZ = Eyes/Head/mange/Rat/Whiskers.NIF    <- body parts, unparsed
    RNAM = 0x60 (1 byte)                       <- attack reach, NOT a race ref

`NpcSpawnJob::runtime` hardcodes humanoid assets (`characters\_male\
upperbody.nif` + hands, RACE head, hair/brow/eyes), so routing alone would
spawn a human torso for a rat — worse than the static-mesh render it replaces.

**Implemented**: `NIFZ` parsing + an `is_creature` flag (plugin), a creature
branch in `prepare_runtime_state` filling the *same* `RuntimePhase` machine
from creature sources, per-creature skeleton + `idle.kf`, and `EsmIndex::actor`
/ `is_actor` as the single actor-detection accessor. The now-redundant `npcs`
parameter was removed from `load_references*` so the two maps cannot drift
apart at a call site again.

**Measured** (`Oblivion.esm` + all `* - Meshes.bsa`, in-tree self-skipping test):
909/909 creature skeletons resolve, 3936/4043 NIFZ parts (residual are DLC-only
meshes), 909/909 `idle.kf` clips.

## #2568 (OBL-D4-01) — HIGH as filed, **premise false**
Claimed Oblivion's legacy `NiParticleSystemController` /
`NiAutoNormalParticles` stack renders no particles. Swept every installed
target game for those block types:

| archive | NIFs | legacy blocks | `NiParticleSystem` |
|---|---|---|---|
| Oblivion - Meshes | 8032 | **0** | 547 |
| DLCShiveringIsles - Meshes | 1438 | **0** | 231 |
| Fallout New Vegas | 14881 | **0** | 1262 |
| Fallout 3 | 10989 | **0** | 422 |
| Skyrim SE - Meshes0 | 18862 | **0** | 1173 |

Oblivion (v20.0.0.5) is already fully on the modern stack. The in-code comment
the audit called false is true; the *actually* stale claim is the block
dispatcher's own "Oblivion magic FX, fire, dust, blood" (Morrowind-era). A
working emission arm was written, then dropped on the user's call — it would be
unreachable code. The measurement is now recorded at all three sites so this
isn't re-filed a fourth time.

## #2569 (OBL-D4-02) — MEDIUM, half-landed by decision
The legacy Lambert arm is duplicated and the copies disagree: clustered
`kD*albedo`, fallback `kD*albedo/PI` then `*0.8` — `0.8/PI ≈ 0.2546` on diffuse
but `0.8` on specular, and the same split applies to the Disney arm. Landed:
the parity tripwire (pinning the divergence, not asserting parity) plus
cross-referencing comments at both GLSL sites. **Not landed**: the shader edit
itself, which changes brightness on a path `cargo test` cannot observe — it
needs a live capture per project policy. Issue stays open for that half.

## #2570 (OBL-D4-04) — LOW, fixed
Added the negative test the issue asks for: legacy-only material input yields
`is_pbr == false`, and deriving legacy PBR *scalars* does not promote a
material onto the Disney lobe. This is what makes `MAT_FLAG_PBR_BSDF` provably
0 for Oblivion instead of accidentally so.
