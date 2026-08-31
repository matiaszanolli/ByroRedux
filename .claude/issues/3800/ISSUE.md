# DBG-2026-08-30-01: cam.pos / cam.tp report success but are overwritten the same frame by camera_follow_system in Character mode

State: OPEN · Labels: bug low character 

## Symptom

With the engine in `PlayerMode::Character` (the default for a normal `--cell` launch), `cam.pos` and `cam.tp` report success and change nothing:

```
byro> cam.pos 1561 -130 -1950
"Camera teleported to (1561.00, -130.00, -1950.00)"
byro> cam.pos 1561 -130 -2450
"Camera teleported to (1561.00, -130.00, -2450.00)"
byro> cam.where
"Camera entity: 18392
   position: (250.89, -8.00, -2613.30)"
```

Two teleports 500 BU apart produced visually near-identical screenshots, and `cam.where` afterwards reports a position unrelated to either. Observed while framing shots in FO4 `BADTFL01`.

## Mechanism

`CamPosCommand::execute` (`byroredux/src/commands/view.rs:970`) writes `Transform.translation` on the active camera and returns. `CamTpCommand` does the same plus rotation/`InputState`.

`camera_follow_system` (`byroredux/src/systems/character.rs:507`) runs in `Stage::Late` every frame and, whenever `PlayerMode == Character` (`character.rs:512`), unconditionally overwrites **both** the camera's `Transform` (`character.rs:598`) and its `GlobalTransform` from `body_pos + eye_height`. The console write lands one frame before that overwrite and is gone before anything renders.

`cam.pos`'s own doc comment reasons carefully about `fly_camera_system` re-applying WASD input and concludes "the new position persists across frames" — that analysis predates the character-mode camera pin and is only true in `PlayerMode::FlyCam`. Nothing in either command consults `PlayerMode`.

## Impact

Dev tooling silently lies. The natural `--bench-hold` → attach → frame-a-shot workflow (which `cam.tp`'s doc comment describes verbatim for `skin.coverage`) doesn't work in the mode the engine boots into, and the success message gives no hint. Worse, it invites the operator to keep re-issuing teleports, which is how a real investigation loses time.

## Suggested direction

Make the mode explicit rather than teaching the operator a workaround:

- Have both commands read `PlayerMode`; when it is `Character`, either
  - **(a)** move the *player body* (reusing the existing camera→body snap helper that `toggle_player_mode` and the door-transition path share) so the camera legitimately follows, which is what the operator meant, or
  - **(b)** refuse with a one-line message naming the reason and the fix ("camera is pinned to the player body in Character mode; press F / switch to FlyCam first").

(a) is closer to what a Bethesda `coc`/`tcl` operator expects and keeps the physics state coherent. (b) is a two-line change if (a) is too much for now — but returning "Camera teleported to (…)" when nothing moved should not survive either way.

Also worth correcting `CamPosCommand`'s doc comment, which currently states the opposite of the observed behaviour.

