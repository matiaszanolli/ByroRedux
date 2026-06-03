# FO4-D5-LOW-02: nif_stats.rs default gate claims all 7 games parse at 100% — wrong for FO4

**Severity**: LOW  
**Source**: AUDIT_FO4_2026-06-02 (D5-LOW-02)  
**Location**: `crates/nif/examples/nif_stats.rs:54-60`

Comment and constant claim "All 7 supported games ship at 100%" and reference a ROADMAP phrase that does not exist. FO4 is 96.46% clean. Running the tool against FO4 without `NIF_STATS_MIN_SUCCESS_RATE=0.96` produces a false failure.

**Fix**: Update the comment to acknowledge per-game variance; document that FO4/FO76/Starfield require an env override.
