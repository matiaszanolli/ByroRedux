# RT-2026-10-31-02: Three of five HUD smokes are red: an authored MessageBox opens over the HUD and the harness never answers it

Labels: medium,ui,test-gap,bug,game:oblivion,game:fnv,game:skyrim
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-02 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: The three scripts still contain no `ui.answer` / `smoke_answer_message_boxes` call.

- **Severity**: MEDIUM (declared gates red; they cannot catch a HUD regression)
- **Dimension**: Gate matrix / harness integrity
- **Location**: `docs/smoke-tests/m48-4-oblivion-hud.sh`, `m48-5-fnv-hud.sh`, `m48-6-skyrim-hud.sh` (pixel gates after the `pin_and_shoot` captures)
- **Status**: NEW (not in the 10-27 report, which did not run them)
- **Game / Cell**: oblivion / AbandonedMine, fnv (HUD smoke cell), skyrim
- **Baseline**: PASS expected
- **Current**: `m48-4`: "full-bars health fill missing (0 px run)". `m48-5-fnv`: "full HP tick band missing (0 white cols)". `m48-6`: "world too animated between captures (4093 vs 3459 changed px)". `m48-5-fo3` and `m48-7-fo4` PASS.
- **Description**: The captured `full.png` shows a modal dialog over the scene: Oblivion's "You dream of long days spent basking in the sunlight of your native lands…" (Done) and FNV's "Happy Trails Expedition" (Ok). The modal's backdrop dims the HUD strip, so the bar/tick colour runs the gates measure are not there. These boxes come from the ObScript quest runtime (M47.3, 10-18 and later), which now runs vanilla start-up message boxes. Every other live gate that meets them calls `smoke_answer_message_boxes` (`ui.answer`); the three HUD scripts do not, and they run their own launch lines instead of `lib/fixture.sh`. The Skyrim failure is the same family (world animated between captures); I did not open its image. Reproduced headless, with the screenshots inspected for Oblivion and FNV.
- **Evidence**: `/tmp/tmp.m2uevM9RDc/full.png` (Oblivion), `/tmp/tmp.UpOPtREaPT/full.png` (FNV; both deleted with this run's scratch cleanup, rerun with `BYRO_SMOKE_KEEP=1` to regenerate).
- **Impact**: Three of the five per-game HUD gates cannot discriminate. The CI contract loop only proves SKIP semantics, so nothing in CI notices.
- **Related**: *feedback_headless_capture_harness* (Oblivion message box needs a click), the P3/P4 gates that already answer boxes.
- **Suggested Fix**: Answer or suppress the start-up box before the first capture (`ui.answer` as `p3-hud` does), or pin the HUD captures with the modal up and sample outside its backdrop.

## Completeness Checks
- [ ] **SIBLING**: The same harness defect checked in the other gates of its family
- [ ] **TESTS**: The gate is re-run live and passes (or SKIPs with 77 on missing data)
