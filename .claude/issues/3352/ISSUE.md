# FNV-2026-08-26-D9-06

**Issue**: #3352
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/misc/pack.rs:292-307`

**Premise verified**:
```rust
let end = start + self.duration_hours as f32;
if end <= 24.0 { h >= start && h < end }        // start == end → always false
else           { h >= start || h < end - 24.0 }
```
With `start_hour = Some(7)` and `duration_hours = 0`, `end == start == 7.0`
and the predicate is `h >= 7 && h < 7` — never true for any hour. The
package can never be selected by `active_package`, at any time of day,
regardless of conditions.

**Evidence** — census of `PSDT.time >= 0 && PSDT.duration == 0` in
FalloutNV.esm: **12 packages**, of which 5 are Travel (an implemented
procedure):
```
0x0e327d HVPaladinPatrolGoToLocker            Travel     time=7  dur=0
0x0b9a27 VChupacabraNightkinMoveToPen          Travel     time=0  dur=0
0x049f7e RCSecurityUnlockHangar6x0Alt          Travel     time=6  dur=0
0x02903e EvergreenAntSoldierGladiatorEntrance  Travel     time=12 dur=0
0x02903d EvergreenSuperMutantGladiatorEntrance Travel     time=12 dur=0
0x0b9a28/29/2a VChupacabraNightkinShootBrahmin1-3  (proc 16, not implemented)
0x1f08e/8f/90/91 JaniceKaplinskiFindNPC11/13/15/17 (proc 0, not implemented)
```
The complementary cases are all handled correctly and are worth recording
as guards: 2813 packages use `time = -1` (`start_hour = None` → always
active, `pack.rs:294-296`); `duration > 24` (Patrol 128 ×6, Travel 73/71/128,
etc., 20 records total) correctly degenerates to always-active through the
wrap branch; and **zero** FNV packages carry a negative duration, so the
`duration.max(0)` clamp at `pack.rs:901` never fires on this corpus.

**Impact**: 5 FNV Travel packages are unreachable by the engine's package
selector — `HVPaladinPatrolGoToLocker` and `RCSecurityUnlockHangar6x0Alt`
are Hidden Valley / REPCONN scripted-movement packages whose actors will
simply never travel. Small absolute count, and the *correct* semantic for
duration 0 is not documented anywhere I could reach (the GECK exposes it
as a plain integer; nothing in this repo or in `docs/legacy/` states what
the original engine does with a zero-length window), so I am **not**
proposing a specific value — per the no-guessing policy.

**Fix sketch**: do not guess a replacement semantic. Either (a) confirm the
original behavior from xEdit / GECK docs before changing `active_at`, or
(b) at minimum surface it — `parse_pack` should `log::debug!` when it
decodes a `start_hour.is_some() && duration_hours == 0` PSDT, so the
unsatisfiable window is visible rather than silent.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
