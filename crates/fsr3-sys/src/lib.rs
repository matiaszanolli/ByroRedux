//! Narrow Rust ABI for AMD FidelityFX FSR 3.1's Vulkan upscaler provider.
//!
//! This crate intentionally exposes only context lifecycle, pure queries, and
//! the upscaler dispatch. No frame-generation provider is compiled.

use std::ffi::{c_char, c_void, CStr};
use std::fmt;
use std::ptr::NonNull;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RawVersion {
    major: u32,
    minor: u32,
    patch: u32,
    provider_id: u64,
}

#[repr(C)]
struct RawContext {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RawCreateDesc {
    vk_device: usize,
    vk_physical_device: usize,
    vk_get_device_proc_addr: usize,
    max_render_width: u32,
    max_render_height: u32,
    max_upscale_width: u32,
    max_upscale_height: u32,
    high_dynamic_range: bool,
    debug_checking: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct RawMemoryUsage {
    total_bytes: u64,
    aliasable_bytes: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct RawImage {
    vk_image: u64,
    vk_format: u32,
    vk_usage: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct RawDispatchDesc {
    vk_command_buffer: usize,
    color: RawImage,
    depth: RawImage,
    motion_vectors: RawImage,
    exposure: RawImage,
    reactive: RawImage,
    transparency_and_composition: RawImage,
    output: RawImage,
    jitter_x: f32,
    jitter_y: f32,
    motion_vector_scale_x: f32,
    motion_vector_scale_y: f32,
    render_width: u32,
    render_height: u32,
    upscale_width: u32,
    upscale_height: u32,
    frame_time_delta_ms: f32,
    pre_exposure: f32,
    reset: bool,
    camera_near: f32,
    camera_far: f32,
    camera_fov_angle_vertical: f32,
    view_space_to_meters_factor: f32,
    enable_sharpening: bool,
    sharpness: f32,
}

extern "C" {
    fn byro_fsr3_query_version(out_version: *mut RawVersion) -> u32;
    fn byro_fsr3_query_render_resolution(
        display_width: u32,
        display_height: u32,
        quality_mode: u32,
        out_render_width: *mut u32,
        out_render_height: *mut u32,
    ) -> u32;
    fn byro_fsr3_query_jitter_phase_count(
        render_width: u32,
        display_width: u32,
        out_phase_count: *mut i32,
    ) -> u32;
    fn byro_fsr3_query_jitter_offset(
        index: u32,
        phase_count: i32,
        out_x: *mut f32,
        out_y: *mut f32,
    ) -> u32;
    fn byro_fsr3_context_create(
        out_context: *mut *mut RawContext,
        desc: *const RawCreateDesc,
    ) -> u32;
    fn byro_fsr3_context_dispatch(context: *mut RawContext, desc: *const RawDispatchDesc) -> u32;
    fn byro_fsr3_context_memory_usage(
        context: *mut RawContext,
        out_usage: *mut RawMemoryUsage,
    ) -> u32;
    fn byro_fsr3_context_destroy(context: *mut *mut RawContext) -> u32;
    fn byro_fsr3_error_string(error_code: u32) -> *const c_char;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub provider_id: u64,
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QualityMode {
    NativeAa = 0,
    Quality = 1,
    Balanced = 2,
    Performance = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    code: u32,
}

impl Error {
    pub fn code(self) -> u32 {
        self.code
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: the native shim returns pointers to static NUL-terminated
        // strings for every code, including unknown values.
        let message = unsafe { CStr::from_ptr(byro_fsr3_error_string(self.code)) };
        write!(
            formatter,
            "{} (FFX error {})",
            message.to_string_lossy(),
            self.code
        )
    }
}

impl std::error::Error for Error {}

/// FFX error code reported by the `BYRO_FSR_FORCE_DISPATCH_FAIL` fault
/// injection: `FFX_API_RETURN_ERROR_RUNTIME_ERROR` (3), which is what
/// `ffx-api` itself returns when the underlying Vulkan runtime or the
/// effect rejects a call (`ffx_provider.h:41`). Using a real code — rather
/// than an out-of-range sentinel — means the renderer's recovery takes the
/// same branch, and logs the same message, as a genuine SDK failure.
const FORCED_FAILURE_CODE: u32 = 3;

/// Fault injection: force [`Context::dispatch`] to fail without entering the
/// SDK, gated on `BYRO_FSR_FORCE_DISPATCH_FAIL=1`.
///
/// The FSR dispatch-failure recovery path (#2140) is unreachable on the
/// happy path and not exercisable from `cargo test` — it needs an SDK OOM,
/// an invalid descriptor, or a device loss mid-frame. This gate makes the
/// "SDK rejected the dispatch having recorded nothing" case reproducible so
/// the recovery barriers can be checked under validation.
///
/// Unlike `debug_checking: cfg!(debug_assertions)` next door, this is
/// deliberately **not** `cfg`-gated to debug builds: smoke tests and
/// benches run `--release`, and that's exactly where the recovery path
/// needs exercising (#2140, AUDIT_RENDERER_2026-08-12b REN-D23-H1). A
/// release binary that happens to inherit this variable from its
/// environment is therefore expected to degrade to the native blit for the
/// whole session — set it deliberately, not by accident. (#2825 /
/// REN-D23-04 — previously kept on `var_os(..).is_some()`, so `=0` and an
/// empty value both meant "on"; only an explicit non-empty, non-"0" value
/// now enables it, matching every other `BYRO_*=1` fault-injection toggle
/// in this codebase.)
///
/// Read once and cached: `dispatch` runs every frame, and re-reading the
/// environment per frame would put a lock-taking syscall on the hot path.
/// The cache means the toggle cannot be unset mid-process — set it before
/// launch, not from a running session.
fn force_dispatch_failure() -> bool {
    use std::sync::OnceLock;
    static FORCED: OnceLock<bool> = OnceLock::new();
    *FORCED.get_or_init(|| env_flag_is_set(std::env::var_os("BYRO_FSR_FORCE_DISPATCH_FAIL")))
}

/// Pure predicate backing [`force_dispatch_failure`], split out so the
/// `=0`/empty-means-off fix (#2825 / REN-D23-04) is unit-testable without
/// touching the process-wide `OnceLock` cache or real environment. Unset
/// and empty mean off; `"0"` means off; anything else — including a
/// non-UTF-8 value, which has no `"0"` it could equal — means on.
fn env_flag_is_set(value: Option<std::ffi::OsString>) -> bool {
    value.is_some_and(|v| match v.to_str() {
        Some(s) => !s.is_empty() && s != "0",
        None => true,
    })
}

fn check(code: u32) -> Result<(), Error> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error { code })
    }
}

pub fn version() -> Result<Version, Error> {
    let mut raw = RawVersion::default();
    // SAFETY: `raw` is valid writable storage for the duration of the call.
    unsafe { check(byro_fsr3_query_version(&mut raw))? };
    Ok(Version {
        major: raw.major,
        minor: raw.minor,
        patch: raw.patch,
        provider_id: raw.provider_id,
    })
}

pub fn render_resolution(
    display_width: u32,
    display_height: u32,
    quality: QualityMode,
) -> Result<[u32; 2], Error> {
    let mut width = 0;
    let mut height = 0;
    // SAFETY: both output pointers remain valid for the duration of the call.
    unsafe {
        check(byro_fsr3_query_render_resolution(
            display_width,
            display_height,
            quality as u32,
            &mut width,
            &mut height,
        ))?;
    }
    Ok([width, height])
}

pub fn jitter_phase_count(render_width: u32, display_width: u32) -> Result<i32, Error> {
    let mut count = 0;
    // SAFETY: `count` is valid writable storage for the duration of the call.
    unsafe {
        check(byro_fsr3_query_jitter_phase_count(
            render_width,
            display_width,
            &mut count,
        ))?;
    }
    Ok(count)
}

pub fn jitter_offset(index: u32, phase_count: i32) -> Result<[f32; 2], Error> {
    let mut x = 0.0;
    let mut y = 0.0;
    // SAFETY: both output pointers remain valid for the duration of the call.
    unsafe {
        check(byro_fsr3_query_jitter_offset(
            index,
            phase_count,
            &mut x,
            &mut y,
        ))?
    };
    Ok([x, y])
}

#[derive(Debug, Clone, Copy)]
pub struct VulkanCreateInfo {
    pub device: usize,
    pub physical_device: usize,
    pub get_device_proc_addr: *const c_void,
    pub max_render_size: [u32; 2],
    pub max_upscale_size: [u32; 2],
    pub high_dynamic_range: bool,
    pub debug_checking: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VulkanImage {
    pub image: u64,
    pub format: u32,
    pub usage: u32,
    pub size: [u32; 2],
}

impl From<VulkanImage> for RawImage {
    fn from(image: VulkanImage) -> Self {
        Self {
            vk_image: image.image,
            vk_format: image.format,
            vk_usage: image.usage,
            width: image.size[0],
            height: image.size[1],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DispatchDescription {
    pub command_buffer: usize,
    pub color: VulkanImage,
    pub depth: VulkanImage,
    pub motion_vectors: VulkanImage,
    pub exposure: Option<VulkanImage>,
    pub reactive: Option<VulkanImage>,
    pub transparency_and_composition: Option<VulkanImage>,
    pub output: VulkanImage,
    pub jitter_offset: [f32; 2],
    pub motion_vector_scale: [f32; 2],
    pub render_size: [u32; 2],
    pub upscale_size: [u32; 2],
    pub frame_time_delta_ms: f32,
    pub pre_exposure: f32,
    pub reset: bool,
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov_angle_vertical: f32,
    pub view_space_to_meters_factor: f32,
    pub enable_sharpening: bool,
    pub sharpness: f32,
}

/// GPU memory the SDK reserved for one upscaler context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryUsage {
    /// Every byte the provider reserved for this context.
    pub total_bytes: u64,
    /// The subset an application could alias with other transient
    /// resources. Reported for completeness; ByroRedux does not alias.
    pub aliasable_bytes: u64,
}

/// Owns an FSR 3.1 upscaler context and its SDK-managed Vulkan resources.
///
/// The Vulkan device, physical device, and loader function passed to
/// [`Context::create`] must outlive this value. The caller must also ensure no
/// submitted command buffer uses FSR resources when this value is dropped.
pub struct Context {
    raw: NonNull<RawContext>,
}

impl Context {
    /// Creates the SDK's Vulkan context.
    ///
    /// # Safety
    ///
    /// `info` must contain live, mutually compatible Vulkan handles and the
    /// matching `vkGetDeviceProcAddr`. Those objects must outlive the result.
    ///
    /// The caller must also ensure the device is idle with respect to FSR
    /// resources before the returned [`Context`] is dropped — no submitted
    /// command buffer may still reference them. `Drop` cannot check this and
    /// destroys the SDK context unconditionally.
    ///
    /// #2692 — this clause was previously only on the [`Context`] type doc,
    /// while `Drop`'s SAFETY comment cited *this* section as its source. A
    /// reader following that cross-reference found no idle requirement here
    /// and could reasonably conclude `Drop` was idle-safe on its own.
    pub unsafe fn create(info: VulkanCreateInfo) -> Result<Self, Error> {
        let desc = RawCreateDesc {
            vk_device: info.device,
            vk_physical_device: info.physical_device,
            vk_get_device_proc_addr: info.get_device_proc_addr as usize,
            max_render_width: info.max_render_size[0],
            max_render_height: info.max_render_size[1],
            max_upscale_width: info.max_upscale_size[0],
            max_upscale_height: info.max_upscale_size[1],
            high_dynamic_range: info.high_dynamic_range,
            debug_checking: info.debug_checking,
        };
        let mut raw = std::ptr::null_mut();
        // SAFETY: upheld by this function's contract; the native shim copies
        // `desc` and returns a uniquely owned opaque context pointer.
        unsafe { check(byro_fsr3_context_create(&mut raw, &desc))? };
        let raw = NonNull::new(raw).ok_or(Error { code: 5 })?;
        Ok(Self { raw })
    }

    /// Records one FSR upscaler dispatch into the caller's live Vulkan command
    /// buffer. The native shim wraps the supplied ash handles as FFX resources;
    /// it does not submit the command buffer.
    ///
    /// # Safety
    ///
    /// Every handle in `desc` must belong to the device used to create this
    /// context, remain live through command-buffer execution, and be in the
    /// layouts represented by the renderer-side FSR boundary contract.
    pub unsafe fn dispatch(&mut self, desc: DispatchDescription) -> Result<(), Error> {
        // #2140 — debug-only fault injection. Returns `Err` *before* the
        // SDK is entered at all, so nothing can have been recorded into
        // the command buffer, which exercises the renderer's
        // dispatch-failure recovery under the conditions it assumes.
        //
        // Those conditions are the real ones: every error the SDK can
        // report on this path fires before `fsr3upscalerDispatch`, the
        // first function that records anything. `ExecuteGpuJobsVK` does
        // record every queued job before checking `errorCode`, but that
        // check is unreachable — all four job executors return `FFX_OK`
        // unconditionally — and `fsr3upscalerDispatch` discards its status
        // anyway. See `vendored_sdk_contract_tests` below, which pins both
        // properties against the vendored sources.
        if force_dispatch_failure() {
            // `eprintln!` rather than `log`: this crate is a thin FFI shim
            // with no `log` dependency (matching the `destroy` path below).
            eprintln!(
                "BYRO_FSR_FORCE_DISPATCH_FAIL: returning a synthetic dispatch error without \
                 entering the SDK — nothing was recorded into the command buffer (#2140)"
            );
            return Err(Error {
                code: FORCED_FAILURE_CODE,
            });
        }
        let raw = RawDispatchDesc {
            vk_command_buffer: desc.command_buffer,
            color: desc.color.into(),
            depth: desc.depth.into(),
            motion_vectors: desc.motion_vectors.into(),
            exposure: desc.exposure.unwrap_or_default().into(),
            reactive: desc.reactive.unwrap_or_default().into(),
            transparency_and_composition: desc
                .transparency_and_composition
                .unwrap_or_default()
                .into(),
            output: desc.output.into(),
            jitter_x: desc.jitter_offset[0],
            jitter_y: desc.jitter_offset[1],
            motion_vector_scale_x: desc.motion_vector_scale[0],
            motion_vector_scale_y: desc.motion_vector_scale[1],
            render_width: desc.render_size[0],
            render_height: desc.render_size[1],
            upscale_width: desc.upscale_size[0],
            upscale_height: desc.upscale_size[1],
            frame_time_delta_ms: desc.frame_time_delta_ms,
            pre_exposure: desc.pre_exposure,
            reset: desc.reset,
            camera_near: desc.camera_near,
            camera_far: desc.camera_far,
            camera_fov_angle_vertical: desc.camera_fov_angle_vertical,
            view_space_to_meters_factor: desc.view_space_to_meters_factor,
            enable_sharpening: desc.enable_sharpening,
            sharpness: desc.sharpness,
        };
        // SAFETY: upheld by this method's contract. The native layer copies the
        // POD descriptor and records into (but never submits) the command buffer.
        unsafe { check(byro_fsr3_context_dispatch(self.raw.as_ptr(), &raw)) }
    }

    /// GPU memory the provider reserved for this context.
    pub fn memory_usage(&self) -> Result<MemoryUsage, Error> {
        let mut raw = RawMemoryUsage::default();
        // SAFETY: `self.raw` is the live context this value owns and `raw` is
        // valid writable storage for the duration of the call.
        unsafe { check(byro_fsr3_context_memory_usage(self.raw.as_ptr(), &mut raw))? };
        Ok(MemoryUsage {
            total_bytes: raw.total_bytes,
            aliasable_bytes: raw.aliasable_bytes,
        })
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        let mut raw = self.raw.as_ptr();
        // SAFETY: this is the unique pointer returned by context creation. The
        // Vulkan-idle requirement is part of `Context::create`'s `# Safety`
        // contract (and restated on the `Context` type doc). The renderer
        // honours it by retiring this context after `device_wait_idle` — see
        // `frame_upscaler.rs`, whose #2158 Drop-ordering source pins assert the
        // FSR context is destroyed before `vkDestroyDevice`.
        //
        // The native side frees the wrapper and nulls `raw` on every path,
        // including a failed `ffxDestroyContext` (#2829) — there is no retry
        // to make here, and holding the wrapper open would strand it with no
        // owner for the rest of the process. A non-OK code therefore means
        // "the provider may have kept its own resources", not "this handle is
        // still live": the pointer is dead either way.
        let code = unsafe { byro_fsr3_context_destroy(&mut raw) };
        if code != 0 {
            eprintln!(
                "failed to destroy FSR context: {} — the provider's own GPU \
                 resources for this context may be leaked",
                Error { code }
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_provider_is_fsr_3_1_4() {
        let version = version().expect("version query");
        assert_eq!((version.major, version.minor, version.patch), (3, 1, 4));
        assert_ne!(version.provider_id, 0);
    }

    /// #2825 (REN-D23-04) — `env_flag_is_set` previously keyed on
    /// `var_os(..).is_some()`, so an explicit `=0` or an empty value both
    /// meant "on". Only an explicit non-empty, non-"0" value should mean
    /// on now, matching every other `BYRO_*=1` fault-injection toggle in
    /// this codebase (`BYRO_VALIDATION`, `BYRO_FORCE_TLAS_ALLOC_FAIL`, …).
    #[test]
    fn env_flag_is_set_treats_zero_and_empty_as_off() {
        use std::ffi::OsString;
        assert!(!env_flag_is_set(None));
        assert!(!env_flag_is_set(Some(OsString::from(""))));
        assert!(!env_flag_is_set(Some(OsString::from("0"))));
        assert!(env_flag_is_set(Some(OsString::from("1"))));
        assert!(env_flag_is_set(Some(OsString::from("true"))));
        // Non-UTF-8 value: presence still means "on" — there's no "0" it
        // could equal.
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let non_utf8 = OsString::from_vec(vec![0xFF, 0xFE]);
            assert!(env_flag_is_set(Some(non_utf8)));
        }
    }

    #[test]
    fn quality_modes_query_expected_render_sizes() {
        assert_eq!(
            render_resolution(1920, 1080, QualityMode::NativeAa).unwrap(),
            [1920, 1080]
        );
        assert_eq!(
            render_resolution(1920, 1080, QualityMode::Quality).unwrap(),
            [1280, 720]
        );
        assert_eq!(
            render_resolution(1920, 1080, QualityMode::Performance).unwrap(),
            [960, 540]
        );
        let balanced = render_resolution(1920, 1080, QualityMode::Balanced).unwrap();
        assert!(balanced[0] > 1120 && balanced[0] < 1140, "{balanced:?}");
        assert!(balanced[1] > 630 && balanced[1] < 640, "{balanced:?}");
    }

    #[test]
    fn jitter_query_is_deterministic_and_nonzero() {
        let phases = jitter_phase_count(1280, 1920).unwrap();
        assert_eq!(phases, 18);
        let first = jitter_offset(0, phases).unwrap();
        assert_eq!(first, jitter_offset(0, phases).unwrap());
        assert_ne!(first, [0.0, 0.0]);
        assert!(first.into_iter().all(|component| component.abs() <= 1.0));
    }

    #[test]
    fn dispatch_abi_structs_are_plain_and_pointer_width_stable() {
        assert_eq!(
            std::mem::align_of::<RawImage>(),
            std::mem::align_of::<u64>()
        );
        assert_eq!(std::mem::size_of::<RawImage>(), 24);
        assert_eq!(
            std::mem::offset_of!(RawDispatchDesc, color),
            std::mem::size_of::<usize>()
        );
        assert!(std::mem::size_of::<RawDispatchDesc>() >= 7 * std::mem::size_of::<RawImage>());
    }
}

/// #2140 / CHAIN-D2-03 — pins the vendored SDK control-flow property that
/// the renderer's dispatch-failure recovery depends on.
///
/// `FrameUpscaler::record`'s recovery path (`crates/renderer/src/vulkan/
/// frame_upscaler.rs`) reacts to an `Err` from [`Context::dispatch`] by
/// recording depth-restore + native-blit barriers whose `old_layout` values
/// are the ones `record_fsr_barriers_before` established. That is correct
/// only if the SDK recorded nothing into the command buffer before it
/// reported the error. The audit filed this as a hypothesis because
/// `ExecuteGpuJobsVK` records every queued job and inspects `errorCode`
/// only *after* the loop — which would allow a partially-recorded
/// transition sequence alongside an error return.
///
/// Tracing the whole chain settles it for v1.1.4 as vendored. Every
/// reachable error return happens strictly before any recording:
///
/// 1. `byro_fsr3_context_dispatch` (native/byro_fsr3.cpp) validates its
///    parameters and returns before entering the SDK.
/// 2. `ffxDispatch` + `ffxProvider_FSR3Upscale::Dispatch` `VERIFY` their
///    pointers and desc type before calling into the effect.
/// 3. `ffxFsr3UpscalerContextDispatch` runs seven `FFX_RETURN_ON_ERROR`
///    guards, all before it calls `fsr3upscalerDispatch`.
/// 4. `fsr3upscalerDispatch` — the first function that schedules or records
///    anything — has exactly one return, `return FFX_OK`. It has no error
///    path at all, and it *discards* `fpExecuteGpuJobs`'s return code.
/// 5. Each of the four `executeGpuJob*` backend functions likewise has a
///    single `return FFX_OK`, so `ExecuteGpuJobsVK`'s post-loop error check
///    is unreachable in this version.
///
/// So the recovery path's assumption holds, and the secondary-command-buffer
/// restructure the issue sketched is not needed. That conclusion is a
/// property of the vendored sources, not of the API contract, so these two
/// tests pin it: an SDK bump that introduces an error return at or after
/// recording fails here instead of silently invalidating the recovery path's
/// declared layouts.
///
/// Point 4 also has an inverse worth stating: because the SDK swallows
/// `fpExecuteGpuJobs`'s status, a hypothetical backend job failure would be
/// reported as `FFX_OK` and take the *success* path. Point 5 is what rules
/// that out today, and is the reason it is pinned alongside point 4.
#[cfg(test)]
mod vendored_sdk_contract_tests {
    use std::path::PathBuf;

    fn vendored(relative: &str) -> String {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("fsr3-sys must remain under <workspace>/crates")
            .to_path_buf();
        let path = workspace
            .join("third_party/fidelityfx-sdk-v1.1.4")
            .join(relative);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("vendored SDK source {} unreadable: {e}", path.display()))
    }

    /// Body text of a top-level function, from its signature line to the
    /// first column-0 `}`. The vendored sources are Allman-braced
    /// throughout, so the closing brace of a top-level definition is the
    /// only `}` that starts a line at column 0.
    fn function_body(source: &str, signature_prefix: &str) -> String {
        let start = source.find(signature_prefix).unwrap_or_else(|| {
            panic!("`{signature_prefix}` not found — the vendored SDK changed shape")
        });
        let rest = &source[start..];
        let end = rest
            .find("\n}")
            .unwrap_or_else(|| panic!("no column-0 closing brace after `{signature_prefix}`"));
        rest[..end].to_string()
    }

    /// Returns every `return` statement in `body`, normalised to a single
    /// space, so a caller can assert on the exact set.
    fn returns(body: &str) -> Vec<String> {
        body.lines()
            .map(str::trim)
            .filter(|line| line.starts_with("return"))
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect()
    }

    /// Point 4 — the first SDK function that schedules or records GPU work
    /// cannot report an error. If a future SDK gains an error return here,
    /// the renderer's recovery path can no longer assume the command buffer
    /// is untouched.
    #[test]
    fn upscaler_dispatch_has_no_error_return_once_recording_can_have_started() {
        let source = vendored("sdk/src/components/fsr3upscaler/ffx_fsr3upscaler.cpp");
        let body = function_body(&source, "static FfxErrorCode fsr3upscalerDispatch(");
        assert_eq!(
            returns(&body),
            vec!["return FFX_OK;"],
            "fsr3upscalerDispatch gained a return other than `return FFX_OK;` — \
             it schedules and executes GPU jobs, so any error return it can now \
             reach may follow a partially-recorded command buffer. Re-audit \
             FrameUpscaler::record's dispatch-failure recovery (#2140) before \
             accepting the SDK bump.",
        );
        assert!(
            body.contains("fpExecuteGpuJobs"),
            "fsr3upscalerDispatch no longer calls fpExecuteGpuJobs — the \
             recording boundary moved, re-derive the chain in #2140",
        );
    }

    /// Point 3 — every guard in the public entry point must precede the call
    /// into `fsr3upscalerDispatch`, or an error could be reported after
    /// recording began.
    #[test]
    fn upscaler_entry_point_validates_before_it_can_record() {
        let source = vendored("sdk/src/components/fsr3upscaler/ffx_fsr3upscaler.cpp");
        let body = function_body(
            &source,
            "FfxErrorCode ffxFsr3UpscalerContextDispatch(FfxFsr3UpscalerContext* context,",
        );
        let call = body
            .find("fsr3upscalerDispatch(contextPrivate, dispatchParams)")
            .expect("entry point no longer delegates to fsr3upscalerDispatch");
        let last_guard = body
            .rfind("FFX_RETURN_ON_ERROR")
            .expect("entry point lost its FFX_RETURN_ON_ERROR guards");
        assert!(
            last_guard < call,
            "an FFX_RETURN_ON_ERROR guard now sits after the \
             fsr3upscalerDispatch call — the SDK can report an error after \
             recording started, invalidating the recovery path's declared \
             layouts (#2140)",
        );
    }

    /// Point 5 — none of the four backend job executors can fail, so
    /// `ExecuteGpuJobsVK`'s post-loop `errorCode` check is unreachable and
    /// there is no "recorded some jobs, then reported an error" path. Note
    /// the failure direction if this regresses: `fsr3upscalerDispatch`
    /// discards the executor's status, so a newly-fallible job would be
    /// swallowed into a `FFX_OK` return and take the success path with a
    /// partially-recorded buffer.
    #[test]
    fn vk_backend_gpu_job_executors_cannot_fail() {
        let source = vendored("sdk/src/backends/vk/ffx_vk.cpp");
        for name in [
            "executeGpuJobClearFloat",
            "executeGpuJobCopy",
            "executeGpuJobCompute",
            "executeGpuJobBarrier",
        ] {
            let body = function_body(&source, &format!("static FfxErrorCode {name}("));
            assert_eq!(
                returns(&body),
                vec!["return FFX_OK;"],
                "{name} gained a fallible return — ExecuteGpuJobsVK's post-loop \
                 errorCode check becomes live, and fsr3upscalerDispatch discards \
                 it, so the failure would surface as FFX_OK on the success path \
                 (#2140)",
            );
        }
    }
}
