# Investigation — #1290 Starfield forward-blocker chain stale post-#762

(Reached via /fix-issue 12900 — a typo; #12900 doesn't exist. User confirmed #1290.)

## Premise — STALE (verified, doubly so)
The issue asked to re-order the Starfield forward-blocker chain so SF-D3-NEW-01
(the sfmaterial→asset_provider *consumer* wiring) sits at the top, since #762
(CDB parser) had closed with nothing reading from it.

Two things invalidate this as written:
1. **SF-D3-NEW-01 is #1289, and it's CLOSED** ("Fixed in 6bd510ba (Phase 1)" —
   `MaterialProvider::sf_cdb` + `has/load_starfield_cdb`, `build_material_provider`
   extracts `materialsbeta.cdb`, new `.mat` arm in `merge_bgsm_into_mesh`). So the
   consumer #1290 wanted promoted to the top has itself shipped (Phase 1).
2. **The chain was already re-ordered across every live doc** before this run:
   - `audit-starfield.md` Dim 6 (lines 84/114): "ESM parser is live (not a forward
     blocker)"; chain = "exterior worldspace tiles, space-cell/planet records, XCLL
     tail, NIF truncation tail — NOT the 'BGSM parser first / ESM very far' chain
     (both have shipped)".
   - `ROADMAP.md`: credits #1289 CDB `.mat` wiring as a Session 42 accomplishment.
   - `starfield-esm-roadmap.md`: "Render with Disney BSDF on Starfield content
     (already wired per #1289 Phase 1)"; Phase 2 (per-field CDB extraction) tracked.

So the re-ordering #1290 requested is done — and current state is past it (the real
top material blocker is now #1289 Phase 2, per-field CDB extraction).

## What was actually fixed
`audit-starfield.md` still listed `#1290` itself as *open* follow-up work
(lines 84 + 114) — self-referentially stale once #1290 closes. Updated both:
- Line 84: removed #1290 from "open follow-ups to scope" (kept #1293, #746/#747);
  noted the re-ordering is done.
- Line 114: marked the chain re-ordered per #1290 (closed), and named the current
  true top blocker — per-field CDB extraction (#1289 Phase 2; `.mat`-resolved
  materials currently reach the Disney lobe with NIF defaults, per
  `starfield-esm-roadmap.md:212`).

## SIBLING check — CLEAN
Scanned `audit-fo4.md` + `audit-skyrim.md` forward-roadmap sections for closed
blockers cited as pending:
- `audit-fo4.md` Dim 6 (lines 85-88, 116): explicitly "BGSM/BGEM parser is shipped …
  SCOL→expansion implemented … do NOT list either as pending; both are shipped."
- `audit-skyrim.md` (line 87): "Skyrim ESM/cell loading is WIRED and rendering, not a
  forward blocker."
Neither lists closed/shipped work as a blocker. No drift to fix.

## Scope / build impact
Skill-markdown only (`audit-starfield.md`). No code, no tests, no cargo/SPIR-V impact.
Path-validate gate: PASS (687 refs across 24 skill files).

## Completeness checks
- **UNSAFE / DROP / LOCK_ORDER / FFI**: N/A.
- **SIBLING**: audit-fo4 / audit-skyrim verified clean.
- **TESTS**: N/A — doc/roadmap fix only.

## Related
#762 (CDB parser, CLOSED), #1289 / SF-D3-NEW-01 (consumer wiring Phase 1, CLOSED,
6bd510ba; Phase 2 per-field extraction outstanding), #1293 (XCLL tail), #746/#747
(NIF truncation tail).
