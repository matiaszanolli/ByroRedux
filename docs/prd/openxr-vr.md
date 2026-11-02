# OpenXR VR — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in
review round 1 (audio-listener HEAD claim corrected; the release mechanism
Q4 leaned on re-measured; a misattributed Meta comfort-default citation
replaced; OpenXR session states re-cited from the registry; Monado headless
operation made a Phase 0 verify with a fallback; Fallout 4 VR claims tied to
Bethesda's own store text with press interviews labelled as such; Q2 linked to
the world-editor PRD's Q8; the Vulkan init-chain invariant stated as a
deviation; exposure, action-coverage and validation-phase inconsistencies
fixed). Revision 1 was the first draft. The decisions it
needs from the project owner are collected in §10; nothing below is built as
a result of this document yet. Statements about how an external specification,
runtime, tool or original VR edition behaves cite external sources (the
Khronos OpenXR and Vulkan registries, the `openxr` crate documentation, the
Monado and SteamVR-for-Linux project pages, UESP, Bethesda's store
descriptions, press interviews with Bethesda staff where no primary source
exists, and design guidance); a claim that could not be verified is marked **Phase 0:
verify** instead of being stated as fact.

**Scope**: ROADMAP milestone M63
([Tier 9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon)):
a player wearing a head-mounted display plays any title the engine already
runs, in stereo, with head tracking and tracked controllers, through an
OpenXR 1.0 / 1.1 runtime on Linux (Monado or SteamVR), using the renderer,
input-action layer and settings registry that exist today. In: the OpenXR
session, the Vulkan hand-off, per-eye stereo rendering through the existing
frame (every temporal pass per view), the frame budget, controller input
mapped onto the existing `InputAction` set, comfort options, and UI on a
composition layer. Out: motion-controlled combat and other per-weapon physical
interaction (decision Q1), VR-aware authoring in the world editor (decision Q2,
paired with [World Editor](world-editor.md)'s Q8),
hand tracking, eye-tracked foveation, augmented reality, standalone-headset
builds, and the original VR editions' data unless decision Q4 brings it in.

**Related**: this document states the *promise*; the designs below own the
*how*. No design owns the OpenXR integration itself yet: Phase 0 writes it
(`docs/engine/openxr.md`, the name the Phase 0 issue confirms).

| Mechanism | Owning design |
|---|---|
| Vulkan init chain, per-frame draw, sync model, resize | [Vulkan Renderer](../engine/renderer.md#per-frame-draw) |
| Pass order, `GpuCamera`, descriptor sets | [Shader Pipeline](../engine/shader-pipeline.md#per-frame-submission-order) |
| Render / output extents, upscaler, temporal-history rules | [FSR 3.1 Integration Plan](../engine/fsr3-upscaler-integration-plan.md#15-temporal-history-rules) |
| Screen-sized VRAM ledger | [Memory Budget](../engine/memory-budget.md#rt-denoiser--post-process-screen-sized-resources) |
| Frame loop, input events | [Game Loop](../engine/game-loop.md#about_to_wait-every-frame--the-per-frame-tick) |
| Windowing and the planned input abstraction | [Platform](../engine/platform.md#future-expansion) |
| HUD and menu rendering (Scaleform, MenuXml) | [UI System](../engine/ui.md) |
| Deterministic capture and denoiser A/B | [Renderer Evaluation](../engine/renderer-evaluation.md#determinism-contract) |
| OpenXR session, views, actions, comfort | none yet: Phase 0 writes `docs/engine/openxr.md` |
| VR-aware authoring (not M63's) | [World Editor PRD](world-editor.md) (WED-NG-010, its decision Q8) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `VRX-` prefix.

---

## 1. Product thesis

The ROADMAP row argues that VR is the genre with the least well-running
ray-traced content, and that an RT-first renderer is useful there. Two
original products show that these games work in VR. Skyrim VR moved Skyrim
Special Edition into a headset with tracked hands, teleport or direct movement,
snap or smooth turning and field-of-view filters
([UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR); local dump
`/mnt/data/src/reference/uesp-wiki/Skyrim/Skyrim VR.wiki`). Bethesda describes
Fallout 4 VR as "the complete core game with all-new combat, crafting, and
building systems fully reimagined for virtual reality"
([Steam store, app 611660](https://store.steampowered.com/app/611660/Fallout_4_VR/)).
Its lead producer described VATS as redesigned to "slow-down time" instead of a
playback camera (interview, [UploadVR, 2017-09-28](https://uploadvr.com/behind-the-scenes-fallout-4-vr/)),
and a Bethesda video showed teleport and direct movement (described by
[Road to VR, 2017-12-08](https://roadtovr.com/?p=167144)). Both editions are
Windows releases on SteamVR (the two store pages' system requirements).
**Phase 0: verify** the two Fallout 4 VR details against a Bethesda primary
source (the official video, patch notes) or the in-game settings of an
owner's install; until then they are reported speech, not fact. ByroRedux can
offer the same presentation for every title it runs, on Linux, with a modern
renderer.

The thesis is only true if the frame fits. A stereo frame has to be finished
within one display period of the headset, for two views, at the per-eye image
size the runtime recommends. The engine's
[bench of record](../../ROADMAP.md#bench-of-record-live--stepped-camera-refresh-2026-10-22-head-d9a201c71)
renders one view at its benchmark output size, which is smaller than a
headset's per-eye image. In that record (checked 2026-11-01, HEAD
`d9a201c71`), no scene's native-TAA frame time is under the period of a
90 Hz display; the figures themselves stay in ROADMAP. The
**budget gap** is the main risk of this milestone, and the requirements
are ordered around it. Phase 1 measures the gap before any player feature is
built.

The corollary that shapes everything below: **VR is a presentation and input
mode of the existing engine, never a second engine.** One renderer, one frame,
one ECS, one input-action set, one settings registry. Per-game differences
stay where they already are (the translate step); nothing VR-specific branches
on the game past the translation boundary
([CLAUDE.md, Architecture Invariants](../../CLAUDE.md#architecture-invariants)).
The engine will *not* ship hand-presence combat, a VR playroom, a raster-only
VR fallback or a second render path for VR.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **XR mode** | The engine running with an OpenXR session as its presentation target, as opposed to the desktop window path. |
| **Runtime** | The OpenXR implementation on the host: Monado ([monado.freedesktop.org](https://monado.freedesktop.org/)), SteamVR ([SteamVR for Linux](https://github.com/ValveSoftware/SteamVR-for-Linux)), or a streaming runtime such as WiVRn ([VR-on-Linux index](https://github.com/VR-on-Linux/VR-on-Linux)). |
| **View** | One of the two eye views of `XR_VIEW_CONFIGURATION_TYPE_PRIMARY_STEREO`. Each has a pose and an `XrFovf` with four independent angles, so a view's frustum can be asymmetric ([XrFovf](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrFovf.html)). |
| **Recommended image rect** | `XrViewConfigurationView::recommendedImageRectWidth/Height`, "the optimal width [height] of `XrSwapchainSubImage::imageRect` to use when rendering this view into a swapchain" ([XrViewConfigurationView](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrViewConfigurationView.html)). The per-view render extent in XR mode derives from it. |
| **Display period** | `XrFrameState::predictedDisplayPeriod`, which `xrWaitFrame` returns: "the XrDuration of the display period for the next application-generated frame" ([XrFrameState](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrFrameState.html)). The frame budget. |
| **Session state** | The runtime-driven `XrSessionState`. Frames are shown in `VISIBLE` and `FOCUSED`; only `FOCUSED` "can receive XR input" ([XrSessionState](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrSessionState.html)). |
| **Budget gap** | Measured stereo GPU frame time minus the display period, per scene (§8). |
| **Stereo frame** | Everything the engine records and submits to produce both views for one predicted display time. |
| **Per-view history** | A temporal pass's history images (TAA, SVGF, ReSTIR reservoirs, FSR context, volumetric reprojection), held once per view so one eye never reprojects from the other. |
| **Action set** | An OpenXR `XrActionSet` whose actions mirror the engine's `InputAction` variants (`byroredux/src/interaction.rs`). Bindings are suggested per interaction profile; the runtime chooses the final binding ([xrSuggestInteractionProfileBindings](https://registry.khronos.org/OpenXR/specs/1.1/man/html/xrSuggestInteractionProfileBindings.html)). |
| **Aim pose** | The controller pose the runtime reports for pointing. Replaces the camera-forward ray as the interaction ray in XR mode. |
| **Artificial motion** | Camera motion the player did not make with their head: stick locomotion, smooth turn, physics knock-back, engine-driven camera moves. |
| **Comfort option** | A setting that reduces artificial-motion discomfort: snap turn, turn speed, vignette, teleport. |
| **Quad layer** | An `XrCompositionLayerQuad`: a flat panel the runtime composites in 3D space. The HUD and menus go here in XR mode. |
| **Desktop mirror** | The desktop window in XR mode, showing one view for spectators and screenshots. |
| **BU** | Bethesda unit. The engine's conversion is `BETHESDA_UNITS_PER_METER` (`crates/core/src/lighting.rs`). OpenXR poses and depth ranges are in metres ([XrCompositionLayerDepthInfoKHR](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrCompositionLayerDepthInfoKHR.html)). |

---

## 3. Core model

XR mode is a presentation mode with one closed set of frame states, and the
engine reports the state it is in:

| State | Meaning | Reported as |
|---|---|---|
| **X0 — Desktop** | No `--xr` request, or no runtime found. The present path at HEAD. | nothing new |
| **X1 — Session idle** | Runtime found, session created, session state `IDLE`, `READY` or `SYNCHRONIZED` (headset off, another application visible). The engine keeps simulating; while `XrFrameState::shouldRender` is false it skips layer rendering, as that member asks ([XrFrameState](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrFrameState.html)). | `xr: idle (<session state>)` |
| **X2 — Presenting** | Session state `VISIBLE` or `FOCUSED`: stereo frames reach the headset. Action-set input is read only in `FOCUSED`; in `VISIBLE` (a runtime overlay has input) the actions read as released. | `xr: presenting <w>x<h> per view @ <Hz> (<session state>)` |
| **X3 — Over budget** | Presenting, but the frame-time EWMA is over the display period. The engine is degrading quality (VRX-PERF). | `xr: over budget by <ms>, tier <n>` |
| **X4 — Lost** | Session state `LOSS_PENDING`, `XR_ERROR_SESSION_LOST` or instance loss. The engine returns to X0 without exiting. | `xr: lost — <reason>; desktop resumed` |

### 3.1 What the frame looks like at HEAD

The renderer and input inventory below is the state at HEAD on 2026-11-01,
checked by reading each named function. It is hand-written. Phase 0 replaces
it with the design document's own inventory, and that inventory is kept true
by the tests VRX-REN-002 and VRX-REN-004 add.

| # | Piece | XR consequence | Today |
|---|---|---|---|
| 1 | Vulkan instance | The runtime must be able to add instance extensions | `create_instance` (`crates/renderer/src/vulkan/instance.rs`) asks for Vulkan 1.3 and enables only the surface extensions `ash_window` reports, plus debug utils when validation is on |
| 2 | Physical device | The runtime names the device for the HMD | `pick_physical_device(instance, surface_loader, surface)` (`crates/renderer/src/vulkan/device.rs`) chooses from `enumerate_physical_devices` against a window surface |
| 3 | Logical device, multiview | Multiview is Vulkan 1.1 core but a feature bit | `create_logical_device` (same file) chains `PhysicalDeviceVulkan12Features` and `PhysicalDeviceVulkan13Features`. It chains no `Vulkan11Features`, so `multiview` is never enabled; RT is mandatory (#3759) |
| 4 | Main render pass | Raster passes could render both views in one pass | `create_render_pass` (`crates/renderer/src/vulkan/context/helpers.rs`): one subpass, nine attachments, no `VkRenderPassMultiviewCreateInfo` |
| 5 | Frame inputs | One camera per frame | `FrameInputs` (`crates/renderer/src/vulkan/context/frame_params.rs`) carries one `view_proj` and one `camera_pos`; `draw_frame` (`context/draw.rs`) records one view |
| 6 | Camera uniform | One view's matrices | `GpuCamera` (`scene_buffer/gpu_types.rs`) holds one `view_proj`, `prev_view_proj`, `inv_view_proj` |
| 7 | Projection | Views have asymmetric frusta | `Camera` (`crates/core/src/ecs/components/camera.rs`) stores `fov_y` plus `aspect`; `projection_matrix` builds a symmetric perspective |
| 8 | Temporal passes | Histories must be per view | `TaaPipeline` (`vulkan/taa.rs`), `SvgfPipeline` (`vulkan/svgf.rs`) and the ReSTIR temporal reservoirs (`vulkan/restir.rs`) each hold one history set per frame in flight; volumetrics reprojects one previous V-buffer (`vulkan/volumetrics.rs`); one `FrameUpscaler` (`vulkan/frame_upscaler.rs`) serves FSR 3.1 |
| 9 | Extents | Per-view extent comes from the runtime | `FrameExtentSet { render, output }` (`vulkan/upscaling.rs`), one pair per swapchain generation; FSR 3.1 Quality is the default (`UpscalerMode::default`) |
| 10 | Present | Runtime-owned swapchain images | `create_swapchain` (`vulkan/swapchain.rs`) builds a `VkSwapchainKHR`; `choose_present_mode` prefers MAILBOX; `draw_frame` calls `queue_present` |
| 11 | Frame pacing | `xrWaitFrame` must pace the loop | `about_to_wait` (`byroredux/src/app_events.rs`) calls `render_one_frame` directly every iteration, uncapped (Phase 14 comment) |
| 12 | Ray budget | Budget must follow the display period | `AdaptiveRayBudget` (`scene_buffer/ray_budget.rs`) steers RT tiers against the fixed constant `TARGET_LIGHTING_MS` |
| 13 | Culling | Both eyes' frusta | `assemble_camera` (`byroredux/src/render/camera.rs`) builds one `FrustumPlanes::from_view_proj` from the `ActiveCamera` |
| 14 | Input actions | Controllers feed the same actions | `InputAction`, `ActionBindings` (keyboard and mouse maps only; its doc says gamepad sources "will join this resource") and `ActionState` (`byroredux/src/interaction.rs`); look is `InputState::yaw/pitch` (`byroredux/src/components/input.rs`) |
| 15 | Interaction ray | Point with the hand, not the head | `camera_ray` (`interaction.rs`) casts from `ActiveCamera`'s forward vector |
| 16 | UI | Screen-space overlay cannot be shown in a headset as is | `FrameInputs::ui_texture_handle`: one optional overlay texture composited into the frame |
| 17 | Audio listener | Must follow the head | the camera entity carries the `AudioListener` component (`byroredux/src/scene.rs`, "ears at the eyes"), and `crates/audio/src/system.rs` places the listener at that entity's `GlobalTransform` |
| 18 | Settings | Comfort options need a home | `SettingsRegistry` with `register_builtin_settings` (`crates/core/src/settings/builtin.rs`); bindings persist as `controls.bind.*` ids (`interaction.rs`) |
| 19 | OpenXR | — | no workspace crate depends on `openxr`; no XR code exists |
| 20 | Field of view | The runtime owns each view's FOV | `gameplay.field_of_view` (`FOV_SETTING_ID`, `crates/core/src/settings/builtin.rs`) sets the desktop camera's FOV |

---

## 4. Goals and non-goals

### 4.1 Goals

- **VRX-G-001 — A measured budget gap.** The stereo GPU frame time at the
  runtime's recommended per-view extent is published per bench scene beside
  the bench of record, before any player-facing VR feature ships.
- **VRX-G-002 — Stereo through the one renderer.** Both views are produced by
  the existing frame (RT lighting, denoisers, volumetrics, post), with every
  temporal pass running per view, on a Linux OpenXR runtime.
- **VRX-G-003 — Frame fits on the reference scene.** On the decided reference
  scene and headset rate (Q3), the frame-budget hit rate (§8) meets the
  Phase 3 exit.
- **VRX-G-004 — Play with controllers.** Every configurable `InputAction`
  has a controller binding, and the interaction ray follows the aim pose.
- **VRX-G-005 — Comfort by default.** Snap turn, turn speed, vignette and (per
  Q6) teleport are settings in the one registry, with defaults taken from
  published guidance.
- **VRX-G-006 — Never strand the player.** Losing the runtime or the session
  returns the engine to the desktop path without exiting.

### 4.2 Non-goals

- **VRX-NG-001.** Motion-controlled combat and physical interaction (weapon
  swings by hand, bow nocking, physical sneaking, realistic swimming), as
  Skyrim VR ships them ([UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)).
  They need a per-weapon, per-game interaction model that no design owns.
  Decision Q1 keeps them out of this milestone.
- **VRX-NG-002.** VR-aware authoring in the M50 editor. The ROADMAP row names
  it. Decision Q2 recommends moving it to M50's own scope.
- **VRX-NG-003.** A raster-only or reduced-feature second render path for VR.
  The [keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)
  row reads "RT-first, rasterized fallback", but at HEAD on 2026-11-01 RT is
  mandatory: `DeviceCapabilities::ray_query_supported` is documented as
  "Always `true`" and `is_device_suitable` rejects a device without RT
  (`crates/renderer/src/vulkan/device.rs`, #3759). There is no rasterized
  fallback for XR mode to reuse, and this PRD does not add one. Over budget,
  the engine degrades the existing tiers (VRX-PERF). It does not switch paths.
- **VRX-NG-004.** Hand tracking, eye tracking, foveated rendering, passthrough
  or AR, and standalone-headset (Android) builds. Each is a separate OpenXR
  extension family with its own hardware matrix. Linux desktop is first
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **VRX-NG-005.** OpenVR and vendor SDKs. OpenXR is the only API. A host
  without an OpenXR runtime stays on the desktop path (VRX-SES-003).
- **VRX-NG-006.** A VR playroom, menu cave or tutorial space like Skyrim VR's
  ([UESP: Skyrim VR § Playroom](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)).
  Boot and setup stay with the launcher; floor height comes from the runtime
  (VRX-VIEW-006).
- **VRX-NG-007.** Wrist-mounted or diegetic per-game UI (a wrist Pip-Boy).
  Menus are shown on a quad layer (VRX-UI). A diegetic variant would be a
  per-game presentation feature and would need its own PRD.

---

## 5. Requirements by area

### 5.1 Session and runtime (`VRX-SES`)

- **VRX-SES-001.** The engine MUST reach OpenXR through the `openxr` crate
  (openxrs, MIT / Apache-2.0, [github.com/Ralith/openxrs](https://github.com/Ralith/openxrs),
  [docs.rs/openxr](https://docs.rs/openxr/latest/openxr/)), with the loader
  strategy decided in Q5. Today: no workspace crate depends on it (§3.1 row 19).
- **VRX-SES-002.** XR mode MUST be requested explicitly (a CLI flag named in
  Phase 0, and a settings-registry entry the launcher can write). Without the request the engine MUST NOT load
  the OpenXR loader.
- **VRX-SES-003.** When XR mode is requested and no runtime is found or
  `xrCreateInstance` fails, the engine MUST start on the desktop path (state
  X0) and log one line naming the failure. It MUST NOT exit.
- **VRX-SES-004.** The session MUST use `XR_KHR_vulkan_enable2`
  ([registry](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XR_KHR_vulkan_enable2.html)):
  the Vulkan instance and device are created through the runtime
  (`create_vulkan_instance` / `create_vulkan_device`), and the physical device
  is the one the runtime reports as "a suitable `VkPhysicalDevice` for use with
  a particular `system`" (`Instance::vulkan_graphics_device`,
  [openxr::Instance](https://docs.rs/openxr/latest/openxr/struct.Instance.html)).
  Today `create_instance` and `pick_physical_device` choose both alone (§3.1
  rows 1–2). The graphics-binding structure's valid usage says its
  "`physicalDevice` … **must** match the device specified by
  xrGetVulkanGraphicsDeviceKHR" and that `device` "**must** have enabled all
  the device extensions specified by xrGetVulkanDeviceExtensionsKHR"
  ([XrGraphicsBindingVulkanKHR](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrGraphicsBindingVulkanKHR.html));
  the `xrGetVulkanGraphicsDevice2KHR` page itself states no such rule
  ([xrGetVulkanGraphicsDevice2KHR](https://registry.khronos.org/OpenXR/specs/1.1/man/html/xrGetVulkanGraphicsDevice2KHR.html)).
  **Phase 0: verify** in the OpenXR 1.1 specification's
  `XR_KHR_vulkan_enable2` chapter
  ([xrspec](https://registry.khronos.org/OpenXR/specs/1.1/html/xrspec.html))
  that the same match rule binds the `enable2` path (where
  `XrGraphicsBindingVulkan2KHR` aliases that structure), and what the engine
  can do when the runtime's device lacks an extension the engine requires.
  The crate requires `Instance::graphics_requirements::<Vulkan>()` to be
  called before `create_vulkan_instance` (its doc); Phase 0 also records the
  Vulkan version range Monado and SteamVR report there against the
  engine's Vulkan 1.3 request (§3.1 row 1).
  The init chain keeps its order under this requirement (§6 row 9).
- **VRX-SES-005.** The device the runtime names MUST still pass the engine's
  own suitability rules (RT extensions, BC textures, `shaderInt64`). If it
  fails them, the engine MUST report which rule failed and fall back to X0.
- **VRX-SES-006.** The engine MUST follow the session state machine the
  runtime drives. It MUST call `xrBeginSession` on `READY` and `xrEndSession`
  on `STOPPING`, as the state descriptions ask
  ([XrSessionState](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrSessionState.html)),
  and report the current state in the `stats` console output (§3 table).
- **VRX-SES-007.** On `LOSS_PENDING`, `XR_ERROR_SESSION_LOST` or instance
  loss, the engine MUST destroy the XR objects in reverse creation order and
  resume the desktop path in the same process (state X4 → X0). A `cargo test`
  that drives a mock loss through the state machine checks this.
  **Phase 0: verify** in the `XR_KHR_vulkan_enable2` chapter whether a
  `VkInstance` / `VkDevice` created through `xrCreateVulkanInstanceKHR` /
  `xrCreateVulkanDeviceKHR` stays valid after its `XrInstance` is destroyed.
  If it does not, the desktop path resumes by recreating the renderer, and
  the design states that cost.
- **VRX-SES-008.** The engine MUST run on Monado, which describes itself as
  "Officially an OpenXR conformant implementation" and supports Vulkan
  applications ([monado.freedesktop.org](https://monado.freedesktop.org/)).
  It SHOULD run on SteamVR. SteamVR for Linux "is built on top of the Vulkan
  API" ([SteamVR for Linux](https://github.com/ValveSoftware/SteamVR-for-Linux)).
  **Phase 0: verify** that SteamVR ships an OpenXR runtime on Linux today, and
  which version. The project README does not mention OpenXR.

### 5.2 Views and projection (`VRX-VIEW`)

- **VRX-VIEW-001.** In state X2 the engine MUST locate both views each frame
  with `xrLocateViews` at the frame's `predictedDisplayTime` and build each
  view's projection from that view's own `XrFovf`, asymmetric angles included.
  Today `Camera::projection_matrix` is symmetric only (§3.1 row 7). A
  `cargo test` checks that a projection built from an asymmetric `XrFovf` maps
  the four tangent angles to the clip-space edges.
- **VRX-VIEW-002.** View translations MUST convert from metres to BU with
  `BETHESDA_UNITS_PER_METER`, the engine's single conversion. A second scale
  factor MUST NOT be introduced.
- **VRX-VIEW-003.** Each view MUST compose onto the player's head anchor (the
  `ActiveCamera` entity's transform) through the existing render-origin path
  (`FrameInputs::render_origin`), so the precision rules of the raster path
  ([Shader Pipeline § Coordinate Spaces](../engine/shader-pipeline.md#coordinate-spaces--precision))
  apply to both views unchanged.
- **VRX-VIEW-004.** CPU frustum culling MUST test against a volume that
  contains both views' frusta. An object visible in either eye MUST NOT be
  culled. Today one frustum is built per frame (§3.1 row 13). A `cargo test`
  checks this with an object visible only at the outer edge of one eye.
- **VRX-VIEW-005.** The projection-layer submission SHOULD attach depth
  through `XR_KHR_composition_layer_depth` when the runtime exposes it.
  `nearZ` / `farZ` are in metres, and the engine's depth mapping is stated in
  `minDepth` / `maxDepth`, reversed mapping included
  ([XrCompositionLayerDepthInfoKHR](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrCompositionLayerDepthInfoKHR.html)).
  **Phase 0: verify** whether Monado and SteamVR use submitted depth for
  reprojection when a frame is late. The registry page states only that depth
  is associated with the colour layer.
- **VRX-VIEW-006.** The engine MUST place the player's floor from
  `XR_REFERENCE_SPACE_TYPE_LOCAL_FLOOR`, core since OpenXR 1.1, when the
  runtime offers it, and from `STAGE` or `LOCAL` plus a stored height
  otherwise ([XrReferenceSpaceType](https://registry.khronos.org/OpenXR/specs/1.1/man/html/XrReferenceSpaceType.html)).
- **VRX-VIEW-007.** A recentre action MUST reset the yaw and horizontal
  offset between the tracking space and the player's head anchor.
- **VRX-VIEW-008.** The audio listener MUST follow the head pose (the view
  midpoint), not the body. Today the `AudioListener` sits on the camera
  entity and follows its `GlobalTransform` (§3.1 row 17). In XR mode the
  entity carrying `AudioListener` MUST be the entity whose transform holds
  the tracked head pose, not the head anchor of VRX-VIEW-003 alone.
- **VRX-VIEW-009.** In XR mode `gameplay.field_of_view` MUST NOT change
  either view's projection; the projection comes only from the runtime's
  `XrFovf` (VRX-VIEW-001). Today the setting drives the desktop camera
  (§3.1 row 20).

### 5.3 Renderer frame (`VRX-REN`)

- **VRX-REN-001.** In XR mode, both views MUST be rendered by the same
  `draw_frame` passes the desktop path runs. The design (Phase 0) chooses how:
  multiview for render passes (`VkRenderPassMultiviewCreateInfo` at
  render-pass creation, `ViewIndex` in shaders,
  [VK_KHR_multiview](https://docs.vulkan.org/refpages/latest/refpages/source/VK_KHR_multiview.html)),
  per-view dispatch for compute passes, or sequential per-view recording.
  Whatever the choice, the set of passes that run in XR mode MUST equal the
  desktop set minus those VRX-REN-008 names.
- **VRX-REN-002.** Every temporal pass the design's inventory lists (at
  least TAA, SVGF, the ReSTIR temporal reservoirs, volumetric reprojection
  and the FSR context) MUST hold its history per view. A view's history MUST
  NOT be read by the other view. A `cargo test` checks the history-slot count
  for each listed pass (views × frames in flight). Today every history is
  single-view (§3.1 row 8). Exposure adaptation is shared, not per view
  (VRX-REN-008).
- **VRX-REN-003.** Each view MUST keep its own previous view-projection for
  motion vectors and its own sub-pixel jitter sequence. A head-pose change
  between frames MUST NOT by itself make `is_camera_cut`
  (`crates/renderer/src/vulkan/context/frame_params.rs`) report a cut; a
  `cargo test` feeds it the largest per-frame head rotation the design
  states and expects no cut.
- **VRX-REN-004.** The per-view render extent MUST derive from the runtime's
  recommended image rect, scaled by the upscaler mode and a render-scale
  setting, through `FrameExtentSet`'s validation, and stay under the
  device's 2D image limit. A `cargo test` covers the derivation.
- **VRX-REN-005.** FSR 3.1 SHOULD run per view when Phase 0 confirms it can.
  **Phase 0: verify** whether the FidelityFX FSR 3.1 API supports two
  simultaneous upscaler contexts on one device, with separate histories
  (FidelityFX SDK documentation). If it cannot, XR mode MUST use the TAA path
  with the render-scale setting.
- **VRX-REN-006.** The final presentation pass MUST write into the runtime's
  swapchain images (`xrAcquireSwapchainImage` / `xrWaitSwapchainImage` /
  `xrReleaseSwapchainImage`) instead of the `VkSwapchainKHR`. Today it writes
  the window swapchain (§3.1 row 10).
- **VRX-REN-007.** In XR mode the frame loop MUST be paced by `xrWaitFrame`,
  which "throttles the application frame loop in order to synchronize
  application frame submissions with the display"
  ([xrWaitFrame](https://registry.khronos.org/OpenXR/specs/1.1/man/html/xrWaitFrame.html)).
  Today `about_to_wait` renders uncapped (§3.1 row 11).
- **VRX-REN-008.** Passes that depend on one screen point of view MUST be
  listed in the design with their XR disposition: run per view, run once and
  share, or off in XR mode. The design MUST cover at least depth of field
  (`DofView`), screen-space bloom, the exposure meter, and the egui debug
  overlay. Exposure (the meter and its adaptation history) SHOULD be
  metered once and shared by both views, so the two eyes never see different
  exposures.
- **VRX-REN-009.** The desktop mirror MUST show one view's presented image,
  as a copy, without rendering a third view.
- **VRX-REN-010.** The screen-sized VRAM ledger
  ([Memory Budget](../engine/memory-budget.md#rt-denoiser--post-process-screen-sized-resources))
  MUST gain a per-view formula. At the reference headset's recommended
  extent, XR mode MUST fit the RT-minimum VRAM target that document states.
- **VRX-REN-011.** XR mode MUST pass the renderer's validation-layer run
  (`BYRO_VALIDATION=1`) with no new validation messages in a 300-frame
  session.

### 5.4 Frame budget (`VRX-PERF`)

- **VRX-PERF-001.** The engine MUST report the stereo GPU frame time (both
  views, first command to last), the CPU record time and the display period
  each frame, in the `stats` console output and in the bench TSV columns.
- **VRX-PERF-002.** The per-pass GPU timers (`gpu_timers.rs`, `TimerPass`)
  MUST report per view, so the per-view cost of each pass can be compared
  with the single-view bench of record.
- **VRX-PERF-003.** In XR mode the RT tier controller MUST take its target
  from the display period instead of the fixed `TARGET_LIGHTING_MS`
  (`AdaptiveRayBudget`, §3.1 row 12). A `cargo test` feeds a 90 Hz and a
  120 Hz period and checks that the derived targets differ.
- **VRX-PERF-004.** When the frame-time EWMA stays over the display period,
  the engine MUST first lower RT tiers, then the render scale, down to floors
  stated in the design. It MUST report state X3 while doing so. It MUST NOT
  disable ray tracing (VRX-NG-003).
- **VRX-PERF-005.** XR bench runs MUST follow the bench of record's
  [standing methodology](../../ROADMAP.md#how-to-run-a-bench): a same-machine
  control, pinned `--camera-pos` / `--camera-forward` (head pose held fixed by the
  Phase 0 Monado setup, or held still on a live headset, §7.2), and an isolated
  `BYROREDUX_SETTINGS_PATH`.
- **VRX-PERF-006.** A frame missed while presenting MUST be counted (the
  engine missed its predicted display time), never hidden by runtime
  reprojection. **Phase 0: verify** how Monado and SteamVR report missed or
  reprojected frames to the application, so the count can use the runtime's
  own number where one exists.

### 5.5 Input (`VRX-IN`)

- **VRX-IN-001.** XR mode MUST create one action set that reaches every
  `InputAction::CONFIGURABLE` variant (`byroredux/src/interaction.rs`): the
  four move variants through the locomotion axis (VRX-IN-003), every other
  variant through one boolean action. Each XR action MUST have a suggested
  binding in every interaction profile the engine supports. A `cargo test`
  fails when a `CONFIGURABLE` variant is reached by no XR action. The design
  states the XR disposition of the variants outside `CONFIGURABLE`
  (`Quicksave`, `Quickload`, `Pause`), which have no configurable binding
  at HEAD.
- **VRX-IN-002.** XR actions MUST feed the existing `ActionState`
  (held / pressed / released), the source `ActionBindings`' doc already
  reserves for non-keyboard input. Consumers of `ActionState` (character
  controller, combat, VATS, interaction) MUST NOT read OpenXR state directly.
- **VRX-IN-003.** Locomotion MUST come from a 2-axis thumbstick action that
  produces the same movement intent as the move actions. Turning MUST come
  from a second axis, through the comfort settings (VRX-CMF). In XR mode the
  mouse-look `InputState::yaw/pitch` MUST NOT drive pitch, because the head
  owns pitch.
- **VRX-IN-004.** The engine MUST suggest bindings for each interaction
  profile the Phase 0 runtime survey lists, and MUST accept the runtime's
  rebinding. Bindings are suggestions, and "the runtime may provide an
  alternate binding for the action, or leave the action unbound"
  ([xrSuggestInteractionProfileBindings](https://registry.khronos.org/OpenXR/specs/1.1/man/html/xrSuggestInteractionProfileBindings.html)).
- **VRX-IN-005.** In XR mode the interaction ray (`camera_ray`'s consumers:
  activation, pickup, dialogue targeting) MUST originate at the main hand's
  aim pose. Today it is camera-forward (§3.1 row 15).
- **VRX-IN-006.** The main hand MUST be a setting (left / right), as in
  Skyrim VR's VR settings "Main Hand"
  ([UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)).
- **VRX-IN-007.** The engine MAY drive controller haptics on hit, block and
  activation through an OpenXR haptic output action.

### 5.6 Comfort (`VRX-CMF`)

- **VRX-CMF-001.** Turning MUST offer snap and smooth modes. Snap increments
  MUST include the three Meta recommends, "30, 45, or 90 degrees", and smooth
  turn MUST have an adjustable speed, as the same page asks ("allow users to
  adjust the speed of the rotation")
  ([Meta: Locomotion user preferences](https://developers.meta.com/horizon/design/locomotion-user-preferences/)).
  Skyrim VR exposes the same choices as Turning Mode, Angle Snap Amount and
  Rotation Speed ([UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)).
- **VRX-CMF-002.** A vignette (dynamic field-of-view restriction) MUST apply
  during artificial motion, with separate strengths for moving and turning,
  matching Skyrim VR's "FOV Filter While Turning" / "FOV Filter While Moving".
  Its strength MUST be zero while the player is stationary. Basis: Fernandes
  and Feiner, "Combating VR sickness through subtle dynamic field-of-view
  modification", IEEE 3DUI 2016,
  [doi:10.1109/3DUI.2016.7460053](https://doi.org/10.1109/3DUI.2016.7460053).
- **VRX-CMF-003.** Teleport locomotion SHOULD be offered as decided in Q6.
  Skyrim VR offers it and enables it by default ("By default, teleportation
  movement is enabled",
  [UESP: Skyrim VR § Playroom](https://en.uesp.net/wiki/Skyrim:Skyrim_VR));
  Fallout 4 VR's teleport is reported by press only
  ([Road to VR, 2017-12-08](https://roadtovr.com/?p=167144); Phase 0 verify,
  §1). Meta lists teleports among the techniques that "help users reorient"
  and asks to "Offer smooth turning as an opt-in feature"
  ([Meta: Locomotion best practices](https://developers.meta.com/horizon/design/locomotion-best-practices/)).
  A teleport target MUST be a position the character controller accepts.
- **VRX-CMF-004.** Engine-driven camera moves (the cinematic system,
  `byroredux/src/systems/cinematic.rs`, VATS playback, kill cameras, camera
  cuts) MUST NOT move the head anchor while presenting. They MUST be replaced
  by a fade or a fixed-position view. Fallout 4 VR is reported to have
  dropped the VATS playback camera the same way (interview,
  [UploadVR, 2017-09-28](https://uploadvr.com/behind-the-scenes-fallout-4-vr/);
  Phase 0 verify, §1).
- **VRX-CMF-005.** Every comfort option MUST be a `SettingsRegistry` entry
  registered beside the existing `controls.*` ids (`register_input_settings`,
  `byroredux/src/interaction.rs`), so the launcher, the debug
  UI and the engine edit the same value.
- **VRX-CMF-006.** Defaults MUST be the comfort-oriented choices of Q6, and
  the design MUST cite the source for each default.

### 5.7 UI (`VRX-UI`)

- **VRX-UI-001.** In XR mode the HUD and menu overlay (today
  `FrameInputs::ui_texture_handle`, §3.1 row 16) MUST be presented as an
  `XrCompositionLayerQuad` at a head-relative distance stated in the design.
  It MUST NOT be drawn into the stereo views as a screen-space overlay.
- **VRX-UI-002.** Menus that pause the game MUST place their quad in front
  of the head at open time, and the quad MUST stay world-locked while the
  menu is open.
- **VRX-UI-003.** The quad layer MUST carry the same texture both HUD
  drivers (Scaleform and MenuXml) already produce, so XR mode adds no
  per-game UI path.
- **VRX-UI-004.** The egui debug overlay MUST stay on the desktop mirror only.

---

## 6. Deliberate deviations and risks

The reference behaviour here is the original VR editions. ByroRedux
does not reproduce them.

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Skyrim VR: hand-driven melee, blocking, spell casting and bow drawing ([UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR), [UESP: Archery § Skyrim VR](https://en.uesp.net/wiki/Skyrim:Archery)) | Combat through `InputAction` buttons (VRX-IN-001) | — | Players expect physical combat → VRX-NG-001, decision Q1 |
| 2 | Fallout 4 VR: VATS as slowed time with real-time teleport, as its lead producer described it (interview, [UploadVR](https://uploadvr.com/behind-the-scenes-fallout-4-vr/); Phase 0 verify, §1) | VATS keeps its engine rules; the playback camera is suppressed in XR mode (VRX-CMF-004) | the playback camera, in XR mode | VATS in VR feels unlike FO4 VR → reported, not hidden; a VR-specific VATS rule would be a CHARAL/VATS change, out of scope |
| 3 | Skyrim VR: playroom main menu, which also calibrates height ([UESP § Playroom](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)) | Launcher boot; floor height from the runtime (VRX-VIEW-006) | the playroom | No in-world tutorial → VRX-NG-006 |
| 4 | Skyrim VR: physical sneaking, realistic swimming and shield grip (VR settings list, [UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)) | Not provided | — | Feature gap vs Skyrim VR → VRX-NG-001 |
| 5 | Fallout 4 VR: Pip-Boy on the wrist, optionally locked to the view (press, [Road to VR](https://roadtovr.com/?p=167144); Phase 0 verify, §1) | Menus on a quad layer only (VRX-UI-001) | — | Less diegetic → VRX-NG-007 |
| 6 | Skyrim VR: separate VR performance presets (supersampling, dynamic resolution, LOD and fade toggles; VR Performance settings list, [UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)) | One budget controller over the RT tiers and render scale (VRX-PERF-004) | per-feature VR quality toggles | Users cannot hand-tune one feature → VRX-PERF-004 states its floors and reports state X3 |
| 7 | Skyrim VR: "SKSE-dlls are a possible exception" to mod compatibility ([UESP](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)) | No VR-specific mod surface; mods behave as on the flat game | VR ports of extender plugins | None in this PRD: dispositions are [Legacy Mod Compatibility](legacy-mod-compatibility.md)'s |
| 8 | Engine (desktop): the camera can be moved by script, cinematics and physics | In XR mode only the head moves the view; other moves become a fade (VRX-CMF-004) | — | Scripted scenes lose framing → VRX-CMF-004 states the replacement |
| 9 | Engine (desktop): [CLAUDE.md invariant 6](../../CLAUDE.md#architecture-invariants), "Full chain: entry → instance → debug → surface → physical device → logical device → allocator → swapchain …", built by the engine alone | XR mode keeps that order, but the instance and logical device are created through the runtime, the physical device is the runtime's, and the presented images come from an `XrSwapchain`; the window surface and `VkSwapchainKHR` serve the desktop mirror only (VRX-SES-004, VRX-REN-006) | — | The engine no longer chooses its own device in XR mode → VRX-SES-005 keeps the suitability rules and falls back to X0; the design states the XR chain beside the desktop one, so invariant 6 is extended, not bypassed |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

XR mode adds no per-game code (corollary, §1). A title is in scope exactly
when the engine renders and plays it, and waves follow the engine tiers in
[Game Compatibility](../engine/game-compatibility.md#per-game-support-detail).
The UI quad carries whichever HUD driver the title already uses.

| Game | In scope | Why / wave |
|---|---|---|
| Fallout: New Vegas | yes | Wave 1. The engine's reference title; MenuXml HUD exercises VRX-UI-003 |
| Skyrim (LE / SE / AE) | yes | Wave 1. Has an original VR edition, so §6 can be compared with it; Scaleform HUD. Skyrim VR's own data per Q4 |
| Fallout 4 | yes | Wave 2. Has an original VR edition; FO4 VR's own data per Q4 |
| Fallout 3 | yes | Wave 2. Shares FNV's machinery; adds no new XR surface |
| Starfield | yes | Wave 3. Follows its engine tier (walkable interiors) |
| Oblivion | yes | Wave 3. Follows its engine tier |
| Fallout 76 | no | No playable cell path ([Game Compatibility § Fallout 76](../engine/game-compatibility.md#fallout-76)), so no XR scenario can run. Because XR adds no per-game code, it would join when the title renders |

### 7.2 Acceptance

Scenarios A1–A6 and A8 need no XR runtime. A7, A9 and A10 need a running
OpenXR runtime, and whether that can be headless is not yet known. Monado's
module index lists a "Simple do-nothing simulated driver", a "Driver for
emulated HMD and controllers through keyboard and mouse" (qwerty), and a
"non-rendering alternate for the main compositor that still can support
applications fully" (null compositor)
([Monado modules](https://monado.pages.freedesktop.org/monado/topics.html)).
`XR_MND_headless` is not the answer: it is an "XrSession without graphical
output and compositor" ([monado.freedesktop.org](https://monado.freedesktop.org/)),
so it cannot carry the stereo frame.

**Phase 0: verify**, on the dev machine with no headset attached, that a
Monado service using the simulated (or qwerty) driver and the null
compositor (1) accepts a Vulkan session through `XR_KHR_vulkan_enable2`,
(2) reaches `FOCUSED` and accepts projection layers from `xrEndFrame`,
(3) holds the head pose fixed for a whole run (the simulated driver may
animate it), and (4) runs without a desktop display, or under the Xvfb
harness the existing smoke tests use. Record the build options, the
environment variables and the reported recommended image rect.

**Fallback** if any of (1)–(4) fails: A7, A9 and A10 become live-gated on
the owner's headset with the head pose held still, run by the same script,
and the bench TSV records the runtime and headset so live rows are never
mixed with simulated ones. The per-view extent then comes from that
headset. Headless coverage is then A1–A6 and A8 only, and the Phase 1 exit
(§9) reads its A7 and A9 conditions as live-gated.

Live-gated scenarios need the owner's headset; Phase 0 records which one.
No game data is checked in. The data named is the user's own install, as
for the existing smoke tests.

| # | Scenario | How it runs | Data | Passes when |
|---|---|---|---|---|
| A1 | XR projection from asymmetric `XrFovf` (VRX-VIEW-001) | new `cargo test` `xr_projection_matches_asymmetric_fov` | none | the four tangent angles map to the clip-space edges |
| A2 | Per-view histories (VRX-REN-002) | new `cargo test` `per_view_history_slots_never_cross_views` | none | every temporal pass holds views × frames-in-flight slots and no view reads the other's |
| A3 | Action coverage (VRX-IN-001) | new `cargo test` `every_configurable_action_has_an_xr_binding` | none | no `InputAction::CONFIGURABLE` variant lacks an XR action |
| A4 | Budget target from period (VRX-PERF-003) | new `cargo test` `ray_budget_target_follows_display_period` | none | the derived targets differ for 90 Hz and 120 Hz periods |
| A5 | Comfort maths (VRX-CMF-001, -002) | new `cargo test`s `snap_turn_uses_configured_increment`, `vignette_is_zero_when_stationary` | none | as named |
| A6 | Session lifecycle and loss (VRX-SES-003, -006, -007) | new `cargo test` over a mock runtime state machine | none | X0 → X1 → X2 → X4 → X0 without process exit |
| A7 | Stereo Cornell session | new script `docs/smoke-tests/m63-xr-session.sh`: `--xr --cornell --bench-frames 300 --bench-hold`, then `byro-dbg` `stats`; against the Phase 0 Monado setup, or live-gated per the fallback | none (Cornell is redistributable) | state X2 reported; both views presented; zero validation messages under `BYRO_VALIDATION=1` (VRX-REN-011) |
| A8 | No-runtime fallback (VRX-SES-003) | same script with no runtime active (no XR runtime needed) | none | engine reaches X0, logs the failure, renders the desktop frame |
| A9 | Budget-gap bench (VRX-G-001) | XR column set added to `scripts/fsr-bench-matrix.sh`, run with the methodology of VRX-PERF-005, against the Phase 0 Monado setup or live-gated per the fallback | every scene of the [bench of record](../../ROADMAP.md#bench-of-record-live--stepped-camera-refresh-2026-10-22-head-d9a201c71), at the record's own invocations | stereo GPU time, display period and per-view extent published per scene in ROADMAP |
| A10 | Reference-scene fit (VRX-G-003) | A9 on the scene and rate Q3 decides | as Q3 | frame-budget hit rate meets the Phase 3 exit |
| A11 | Wave 1 play (VRX-G-004) | live-gated on the owner's headset: walk, turn (snap and smooth), activate a door and a container, open the HUD menu | FNV, Skyrim SE installs | each step completes with the controller only; `byro-dbg` confirms the activation events |

---

## 8. Metrics

One home for the numbers: ROADMAP's bench-of-record section gains an XR
table (Phase 1). This document defines what is counted.

| Metric | Definition | Granularity |
|---|---|---|
| Budget gap | median stereo GPU frame ms − display period ms | per scene, per runtime, per upscaler mode |
| Frame-budget hit rate | presenting frames finished before their predicted display time ÷ presenting frames | per scene, per runtime |
| Stereo cost ratio | stereo GPU frame ms ÷ single-view GPU frame ms at the same per-view extent | per scene |
| Per-view extent | the render extent each view used (VRX-REN-004) | per scene, per runtime and headset |
| Action coverage | `InputAction::CONFIGURABLE` variants reached by an XR action ÷ `InputAction::CONFIGURABLE` variants | per release |
| Missed frames | frames counted by VRX-PERF-006 ÷ presenting frames | per scene, per runtime |

---

## 9. Delivery sequence

Phases are ordered by the budget gap: the gap is measured before any
player feature is built. M27 (parallel system dispatch) shipped
([Completed Milestones](../../ROADMAP.md#completed-milestones)); no phase
depends on M50 if Q2 is taken as recommended. Tier 9's own gate ("No active
work until M40 (streaming) and M45 (save/load) prove the underlying state
model holds up", [ROADMAP § Tier 9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon))
applies to Phase 1 onward; ROADMAP, not this document, says whether it is met.

**Phase 0 — Design, survey and decisions (no engine code).** Write the OpenXR
design (`docs/engine/openxr.md`): the stereo technique for render and compute
passes (VRX-REN-001), the per-pass XR disposition list (VRX-REN-008), the
budget floors (VRX-PERF-004), the UI quad placement (VRX-UI-001). Survey the
runtimes and headsets available on the dev machine and their interaction
profiles (VRX-IN-004). Resolve §10. Close every **Phase 0: verify** item:
the vulkan_enable2 device rule (VRX-SES-004), SteamVR's Linux OpenXR runtime
(VRX-SES-008), depth-layer use for reprojection (VRX-VIEW-005), FSR 3.1
multi-context support (VRX-REN-005), runtime missed-frame reporting
(VRX-PERF-006), Monado headless operation (§7.2, items 1–4, or the
fallback recorded), Vulkan object lifetime after instance loss (VRX-SES-007),
the Fallout 4 VR behaviour claims (§1), the original VR editions' data files
and install layout (Q4). Also check whether the selected GPU supports multiview
together with the main pass's features, and `maxMultiviewViewCount`
(`VkPhysicalDeviceMultiviewProperties`). Exit: the design is checked in, every
verify item is closed or filed, and every §10 item carries **Decided**.

**Phase 1 — Session and the measured gap.** Session and runtime
(VRX-SES-001…008), views and projection (VRX-VIEW-001…004, -009), per-view
histories (VRX-REN-002, -003), runtime swapchain and pacing (VRX-REN-004,
-006, -007), the validation run (VRX-REN-011, which A7 checks), per-view
timers and frame reporting (VRX-PERF-001, -002, -005).
No comfort, input or UI work. Exit: scenarios A1, A2, A6, A7, A8 pass, and A9's
budget gap is published in ROADMAP for every bench-of-record scene (A7 and A9
live-gated if §7.2's fallback applies).

**Phase 2 — Budget convergence.** Display-period targets and the degradation
ladder (VRX-PERF-003, -004, -006), the FSR-per-view or TAA choice
(VRX-REN-005), pass dispositions (VRX-REN-008), the VRAM formula (VRX-REN-010).
Exit: A4 passes, and the budget gap on the
reference scene is published again with the new ladder in place.

**Phase 3 — Reference-scene fit.** The optimisation work Phase 2's numbers
point at, in the renderer's existing passes. Exit: A10 meets the hit rate
Q3 decides, against a same-machine control.

**Phase 4 — Play: input, comfort and UI.** Action set and bindings
(VRX-IN-001…007), comfort (VRX-CMF-001…006), UI quad (VRX-UI-001…004),
recentre, floor and listener (VRX-VIEW-005…008), desktop mirror
(VRX-REN-009). Exit: A3 and A5 pass and A11 passes on FNV and Skyrim SE.

**Phase 5 — Later waves.** Wave 2 and Wave 3 titles (§7.1), and the original
VR editions' data if Q4 brings it in. Exit: A7 and A11 pass per title; the
frame-budget hit rate is published per title's bench scene where one exists.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — What "full VR" promises.** Recommended: presentation plus
   controller play through the existing `InputAction` set (this PRD), with
   motion-controlled combat and physical interaction in a follow-up PRD once
   a per-weapon interaction model has a design. It ships something playable
   on every title with no per-game interaction code. Alternative: include
   hand-driven combat as Skyrim VR does. This needs a new interaction layer
   with per-game weapon data and makes M63 several times larger. It also
   delays the budget measurement behind gameplay work.
2. **Q2 — M50 dependency and VR-aware authoring.** Recommended: remove M50
   from M63's dependencies and move "VR-aware authoring" into M50's scope,
   as a later phase there. A VR *player* needs no editor. Keeping the
   dependency blocks the player promise on a separate Tier 9 milestone with
   its own PRD. The [World Editor PRD](world-editor.md) already makes the
   matching offer: WED-NG-010 keeps VR-aware authoring out of its Phases 0–6,
   and its Q8 recommends accepting it as a later M50 phase once M63's player
   promise has shipped. The two questions are one decision and are decided
   together; if they are answered differently, the item has no owner.
   Alternative: keep the ROADMAP row as written. M63 then cannot start until
   M50 ships, and this PRD needs an authoring area. On the recommended answer,
   the M63 row's scope and dependency cells change in the commit that records
   the decision.
3. **Q3 — Reference rate and scene for the fit gate.** Recommended: gate on
   a 90 Hz display period, with the runtime-reported period always used as
   the live budget. The reference
   scene is Prospector, the reference title's bench scene. Alternative A:
   gate on the headset's lowest offered rate (lower bar, earlier exit).
   Alternative B: gate on Cornell only (redistributable, but empty of game
   content, so it understates the gap).
4. **Q4 — The original VR editions' data.** Recommended: out of this PRD's
   promise. XR mode runs the flat games' data, and Skyrim VR and FO4 VR
   installs are considered in Phase 5. At HEAD on 2026-11-01 the
   file-selected `[[profiles.<key>.releases]]` mechanism
   (`assets/debug_profiles.toml`, `GameProfileEntry::for_data_dir`) can
   swap only archive lists: `GameRelease`
   (`crates/core/src/ecs/game_profiles.rs`) carries a name and the five
   `default_*_bsas` lists, and the profile's `subdir` and `esm` stay fixed
   (the `fo4` profile's `subdir` is `Fallout 4/Data`). A VR edition with its
   own master plugin or install directory therefore needs either a new
   profile per edition or a `GameRelease` that can also carry `esm` and
   `subdir`; the second changes the launcher-profile contract (README §1
   T2) and is decided in Phase 5. **Phase 0: verify** which master plugin,
   archives and install directory each VR edition ships and how they differ
   from SE / FO4. UESP states only that "Most mods and save games from the
   Special Edition are compatible with Skyrim VR" and names a separate
   `SkyrimVR.ini`
   ([UESP: Skyrim VR](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)).
   Alternative: in scope from Wave 1, which needs those installs on the dev
   machine and puts their data differences and the profile change on the
   critical path.
5. **Q5 — How OpenXR enters the build.** Recommended: the openxrs `loaded`
   feature, which loads `openxr_loader` "at run time" (`Entry::load`,
   [openxr::Entry](https://docs.rs/openxr/latest/openxr/struct.Entry.html)),
   so the build needs no XR SDK and a host without a loader simply stays on X0.
   Alternative A: `static`, which "builds in the Khronos OpenXR loader"
   (Apache-2.0, [OpenXR-SDK](https://github.com/KhronosGroup/OpenXR-SDK)) but
   needs a C++ compiler and CMake in every build ([openxrs README](https://github.com/Ralith/openxrs)).
   Alternative B: `linked` against the distribution's loader, which ties the
   binary to that loader being installed.
6. **Q6 — Comfort defaults and teleport.** Recommended: snap turn by
   default, vignette on during smooth locomotion, and teleport offered and on
   by default. Snap turn by default follows Meta's "Offer smooth turning as
   an opt-in feature"
   ([Meta: Locomotion best practices](https://developers.meta.com/horizon/design/locomotion-best-practices/));
   teleport on by default matches Skyrim VR's first run ("By default,
   teleportation movement is enabled",
   [UESP § Playroom](https://en.uesp.net/wiki/Skyrim:Skyrim_VR)). Meta's page
   names no default locomotion method, so the teleport default rests on
   Skyrim VR alone. Alternative: smooth locomotion by default with the
   vignette, and no teleport. This is less work (no teleport validation
   against the character controller) but departs from both sources above.

---

## 11. Document map

| Need | Go to |
|---|---|
| Milestone row and dependencies | [ROADMAP.md § Tier 9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon) |
| Bench numbers the budget gap is measured against | [ROADMAP.md § Bench of record](../../ROADMAP.md#bench-of-record-live--stepped-camera-refresh-2026-10-22-head-d9a201c71), [§ How to run a bench](../../ROADMAP.md#how-to-run-a-bench) |
| Init chain, per-frame draw, sync | [renderer.md](../engine/renderer.md#per-frame-draw) |
| Pass order and `GpuCamera` | [shader-pipeline.md](../engine/shader-pipeline.md#per-frame-submission-order) |
| Upscaler extents and temporal-history rules | [fsr3-upscaler-integration-plan.md](../engine/fsr3-upscaler-integration-plan.md#15-temporal-history-rules) |
| Screen-sized VRAM ledger | [memory-budget.md](../engine/memory-budget.md#rt-denoiser--post-process-screen-sized-resources) |
| Frame loop and input events | [game-loop.md](../engine/game-loop.md#about_to_wait-every-frame--the-per-frame-tick) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| HUD drivers | [ui.md](../engine/ui.md) |
| Mod dispositions in VR | [legacy-mod-compatibility.md](legacy-mod-compatibility.md) |
| VR-aware authoring and the M50 dependency (Q2) | [world-editor.md](world-editor.md) (WED-NG-010, its Q8) |
| Anti-scope and keep list | [ROADMAP.md § anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope), [§ keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation) |
| OpenXR session, views, actions, comfort (the *how*) | `docs/engine/openxr.md`, written in Phase 0 |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: audio-listener HEAD claim (§3.1 row 17, VRX-VIEW-008); release mechanism re-measured for Q4; Meta comfort-default citation corrected (VRX-CMF-003, Q6); session states re-cited (§3, VRX-SES-006/007); Monado headless as Phase 0 verify with fallback (§7.2, VRX-PERF-005, Phase 1 exit); Fallout 4 VR claims tied to Bethesda's store text, press labelled, Phase 0 verify; vulkan_enable2 device rule cited; invariant 6 deviation (§6 row 9); keep-list wording in VRX-NG-003; translator claim dropped (VRX-NG-005); Q2 paired with world-editor Q8; exposure out of VRX-REN-002; VRX-IN-001 vs IN-003; action-coverage denominator; bench scenes linked; VRX-REN-011 into Phase 1; VRX-VIEW-009 added | claude: revise |
