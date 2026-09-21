# NIFAL-D7-2026-09-21-03: Code pins the fixture doc's alternate attack clip (2hmattackforwardb) while its comment claims the paths are pinned by that doc

**Labels**: low, nifal, animation, documentation, doc-rot, game:skyrim

**Severity**: LOW · **Dimension**: Animation / controllers (P2 fixture alignment) · **Tier Violated**: — (fixture/doc drift) · **Game Affected**: Skyrim
**Location**: `byroredux/src/asset_provider/animation.rs:236` (`DRAUGR_ATTACK_PATH = …2hmattackforwardb.hkx`) vs `docs/engine/p2-combat-anim-sound-fixture.md` freeze rule
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
The fixture doc's freeze rule designates the 2GS power chop (`2gsattackforwardpowerchop2.hkx`) as the gate's primary attack; the implementation pins `2hmattackforwardb` (the doc's non-power sibling) while the code comment (:227-233) says "Asset paths pinned by `docs/engine/p2-combat-anim-sound-fixture.md`". Both decode (84-track hk_2014 clips), so the mechanics work; doc and installed family disagree on which take the gate demonstrates.

### Evidence
`:227-233` comment + `:236` const, against the fixture doc §"Pinned animation set" freeze rule.

### Impact
The P2 gate, when wired per the doc, demonstrates a different attack than installed; a future reader reconciling code against the doc cannot tell which is authoritative.

### Related
NIFAL-D7-2026-09-21-01 (same fixture family)

### Suggested Fix
Either pin `2gsattackforwardpowerchop2.hkx` per the doc, or edit the doc's freeze rule to name `2hmattackforwardb` as the installed primary.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
