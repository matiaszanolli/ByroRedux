# EXT-D4-2026-10-26-02: skyal.md is silent on the 20 m/s textured-cloud drift cap, which is documented only in watal.md

Labels: low,terrain-exterior,doc-rot,documentation
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW
- **Dimension**: 4 (doc-rot)
- **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `byroredux/src/systems/weather.rs:692-720` (`CLOUD_LAYER_MAX_DRIFT_M_PER_S`, `cap_cloud_layer_drift`); `docs/engine/watal.md:609-632` (the only prose); `docs/engine/skyal.md` §2.3-§2.5
- **Status**: NEW (documentation placement)
- **Description**: The cap is a user-chosen calibration ("near real life", 2026-10-26), not an authored value. It bounds every authored textured layer to 20 m/s of ground drift over the 9 km shell, silently overriding the WTHR scroll rate. Code is clean: it is a named constant derived from `CLOUD_UPPER_PLANE_ALTITUDE`, with a pinning test. But the sky's source-of-truth doc (skyal.md, the one the skill says to cite) does not mention it. A future sky audit that sees authored scroll rates (e.g. the Oblivion layer-0 0.0143 UV/s) not matching the rendered motion has no skyal.md entry to cite, and the rule "documented, do not re-file" can only be applied from watal.md. skyal.md §2.5 also does not list the effect-lit directional arm (#6244) in its sun-site enumeration ("lighting.glsl's two directional arms, grass, the water glint"), though the guard and skill do.
- **Evidence**: `grep -n "20 m/s\|drift\|scroll" docs/engine/skyal.md` -> no cap entry (one unrelated "drift speed" line at 294). `grep -rn CLOUD_LAYER_MAX_DRIFT` hits only `weather.rs` plus the watal.md prose, which names `cap_cloud_layer_drift` but not the constant.
- **Impact**: Doc-only; the calibration constant is easy to rediscover as a "magic number" and the sun-site list misleads a reader about guard coverage.
- **Related**: 4f2596bf6.
- **Suggested Fix**: Add a short "Textured-layer drift cap" paragraph to skyal.md §2.4 (constant name, 20 m/s, formula `scroll / tile_scale * CLOUD_UPPER_PLANE_ALTITUDE`, chosen-not-authored, deck advection untouched) and add "the effect-lit pass" to the §2.5 sun-site list; keep the watal.md cross-reference.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._
