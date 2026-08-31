# Batch: #3801, #3802, #3803, #3804, #3805, #3806 (EX-16 sub-issues)

All six were filed by this session (previous turn) as splits of #2372.
User confirmed scope for this pass:

- **#3805** — debug telemetry command reporting REGN/NAVM/audio/AI owners
  per cell. Tractable, single-session. **Implement.**
- **#3802** — cross-tile NAVM path connectivity, blocked on the
  `NavmExternalConnection` source-triangle field. **Research the blocker**
  (nif.xml / xEdit fopdoc / Gamebryo 2.3 source / Elder Scrolls wiki);
  either unblock (document the field, don't yet implement the full A*
  search — that's a separate scope item) or document why it can't be
  resolved. Do not guess.
- **#3801** — REGN Weather/Grass/Landscape/Objects consumption. Each
  sub-part needs its own design decision (weather-override semantics vs
  CLMT/WTHR blend, ground-cover wiring, encounter-metadata home). **Leave
  open**, comment explaining scope/blockers, no speculative implementation.
- **#3803** — actor/package suspend/migrate/resume. Explicitly "real new
  architecture... expect multiple PRs." **Leave open.**
- **#3804** — per-emitter/per-region ambient audio ownership. Blocked on
  `RegionSound::chance_raw` fixed-point scale (no-guessing-policy).
  **Leave open.**
- **#3806** — boundary/soak test harness. Explicitly blocked on #3802 and
  #3803 landing first. **Leave open** (its blocking issues are not landing
  in this pass, since #3802 gets only a research spike, not the
  implementation).

## Plan
1. Implement #3805: new `byro-dbg` console command, binary crate.
2. Investigate #3802's NAVM external-connection field.
3. Post explanatory comments on #3801/#3803/#3804/#3806, leave open.
4. Close only #3805 (and #3802 if the research spike resolves it, per that
   issue's own "the research spike itself is valid scope" framing).
