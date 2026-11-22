# #7234: GAME-2026-11-22-11: `weapon_defense`'s "no projectile" clause is vacuous, and the `UseWeapon` hit producers hard-code `ignores_resistance: false` (latent)

**Labels**: low,gameplay,combat,bug,game:oblivion

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-11**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 4 (combat)
- **Domain**: combat · **Game**: Oblivion (latent)
- **Trigger**: A projectile or `UseWeapon` hit by an actor with no `EquippedWeapon` on an Oblivion load. Nothing reaches it today: Oblivion has no UseWeapon procedure, no NPC archery and no scripted projectile fire.
- **Location**: `byroredux/src/combat.rs:556` (`unarmed = event.projectile == 0 && !event.arrow && no EquippedWeapon`); `crates/scripting/src/trap_hit_requests.rs:68-80` (`projectile: 0`); `byroredux/src/systems/script_requests.rs:135,182` (`ignores_resistance: false`)
- **Status**: NEW
- **Description**: #7195 defines a natural creature attack as "a `CreatureActor` blow with no weapon and no projectile". But the `TrapHit` flush, which lands every projectile and every UseWeapon strike, always writes `projectile: 0`, so the clause cannot exclude a projectile. A shooter whose launcher is not its `EquippedWeapon` would count as unarmed, and a creature or a Journeyman Hand-to-Hand actor would bypass Resist Normal Weapons. The UseWeapon producers know `run.request.weapon` but pass `false` for `ignores_resistance`.
- **Evidence**: `HitEvent.projectile` is non-zero only at `mines.rs:157`, whose aggressor has no `ActorVitals`.
- **Impact**: None on a shipped path. The "no projectile" guard looks like protection it does not give.
- **Related**: #7195, #7199
- **Suggested Fix**: Carry a real `from_projectile` marker on `TrapHit` → `HitEvent` in place of the `projectile == 0` test, or reword #7195's docs to say the test rests on `EquippedWeapon` alone. Answer `ignores_resistance` from `run.request.weapon` in the UseWeapon producers.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
