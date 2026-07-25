# AUD-2026-07-25-01: footstep_system docstring misattributes fly-camera opt-in to main.rs::App::new

- **GitHub Issue**: #2163
- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring (Dimension 7)
- **Location**: `byroredux/src/systems/audio.rs:97-98`
- **Labels**: `low`, `documentation`, `tech-debt`
- **Source Report**: `docs/audits/AUDIT_AUDIO_2026-07-25.md`

## Description
The doc comment on `footstep_system` reads:

```
/// Spawn a `FootstepEmitter` on the player entity to opt in. The
/// fly-camera attach is wired in `main.rs::App::new`.
```

This is factually wrong on two counts. First, the actual attach call
(`world.insert(cam, crate::components::FootstepEmitter::new());`) lives in
`byroredux/src/scene.rs:449`, inside `setup_scene` — not in `main.rs`.
Second, even the *caller chain* doesn't reach `App::new`: `App::new` is
`main.rs:259-402` (verified by full-body grep, zero `Footstep`/`footstep`
hits) — a constructor. `setup_scene` is invoked from `App::setup_scene()`
(`main.rs:369`), which is itself called from `ApplicationHandler::resumed`
(`main.rs:856`, inside the winit event-loop callback), never from `App::new`.
So the doc doesn't just point at the wrong file — it points at the wrong
*kind* of call site (constructor vs. window-resume callback).

## Evidence
`git log -S"fly-camera attach is wired"` finds exactly one origin, commit
`3987ecd1` ("M44 Phase 3.5: footstep gameplay loop..."), 2026-05-05 — the
*same commit* that introduced the attach call in `scene.rs`. The docstring
has been wrong since day one and has survived the `systems.rs` →
`systems/audio.rs` module split (`2bdbc365`, 2026-05-12) and six subsequent
`/audit-audio` cycles (`_05-05` through `_07-16`) without being caught.

Re-verified directly against current HEAD (`ca7a4e0e`) during triage:
`grep -n Footstep byroredux/src/main.rs` returns zero hits, and
`grep -n FootstepEmitter byroredux/src/scene.rs` shows the actual insert at
line 449.

## Impact
Purely a documentation-accuracy bug — the actual opt-in behavior is correct
and component-driven. Impact is confined to future maintainers or audit
passes who trust the docstring instead of grepping. Same class of bug as
the already-fixed AUD-2026-07-02-01 / #1859 (`SoundCache` docstring citing
a stale path), just a different docstring in the same subsystem.

## Related
Analogous to #1859 (closed, same docstring-accuracy class). Not a
regression of it — a distinct docstring, never previously flagged.

## Suggested Fix
Update the doc comment to `The fly-camera attach is wired in
\`scene.rs::setup_scene\`.` (one-line fix, matches the existing `SoundCache`
docstring-fix pattern from #1859).

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (doc-comment accuracy has no automated test; consider a `grep`-based doctest or leave as manual review since this is prose, not behavior)

## Dedup Check
`gh issue list` fresh pull (59 open issues, run at time of publish) — no
open issue matched "footstep"/"docstring"/"audio" keywords except #1943
(unrelated Cornell glass-probe docstring, different subsystem). Closed
issue #1859 is the analogous-but-distinct precedent (different docstring),
not a duplicate.
