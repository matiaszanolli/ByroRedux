# #1020 — Investigation: cloud parallax screen-space vs world-XY

## Audit premise vs reality

Audit body claims:

> Cloud parallax direction is screen-XY rather than world-XY. Rotating
> the camera makes clouds appear to follow the view rather than scroll
> along world-space wind direction.

Both halves of the premise are false on HEAD.

## Site-by-site analysis

The audit lists [`triangle.frag`](crates/renderer/shaders/triangle.frag)
as the file. Cloud sampling does not live there — `grep cloud
triangle.frag` returns zero hits. The actual cloud code is in
[`composite.frag:169-233`](crates/renderer/shaders/composite.frag#L169-L233),
across all 4 WTHR layers.

The relevant UV math (all four layers, identical shape):

```glsl
vec2 uv = dir.xz / max(elevation, 0.05) * tile_scale
        + params.cloud_params.xy;
```

`dir` is supplied by [`screen_to_world_dir`](crates/renderer/shaders/composite.frag#L113-L128):

```glsl
vec3 screen_to_world_dir(vec2 uv) {
    vec2 ndc = uv * 2.0 - 1.0;
    vec4 clip = vec4(ndc, 1.0, 1.0);
    vec4 world = params.inv_view_proj * clip;
    ...
    return normalize(world.xyz / w);
}
```

`inv_view_proj` is the inverse view-projection — `dir` is **already in
world space**. The sample is therefore:

- `dir.xz` — world-space X / Z (Y is up)
- `dir.xz / dir.y` — projection onto an infinite horizontal plane
  overhead (world-XZ position on a unit-Y dome)
- `× tile_scale` — texture density
- `+ params.cloud_params.xy` — world-XZ scroll offset accumulated
  per-frame in [`weather.rs:522-550`](byroredux/src/systems/weather.rs#L522-L550)

For a fixed world-ray, the UV is invariant under camera yaw / pitch /
position. Clouds do **not** follow the camera.

## Why the audit-fix doesn't apply

The proposed fix:

> Project the parallax direction through view-inverse before sampling
> cloud layers.

is already exactly what `screen_to_world_dir` does. Doing it again would
transform world-space back through the inverse a second time and
produce twice-rotated screen-space sampling — the bug the audit thinks
it's fixing.

## Recommendation

**Close as `wontfix`.** Premise doesn't survive verification: cloud
sampling is fully world-XY today; the file path the audit cites doesn't
contain cloud code; and the proposed fix would break a correct
implementation. This is the `feedback_audit_findings` pattern again —
verify premise before drafting fixes.

No code touched in this pass.
