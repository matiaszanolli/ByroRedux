//! Narrow Rust ABI for AMD FidelityFX FSR 3.1's Vulkan upscaler provider.
//!
//! This crate intentionally exposes context creation/destruction and pure
//! queries only. Frame dispatch resources are added when the renderer's input
//! contracts are implemented; no frame-generation provider is compiled.

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
}

impl Drop for Context {
    fn drop(&mut self) {
        let mut raw = self.raw.as_ptr();
        // SAFETY: this is the unique pointer returned by context creation. The
        // Vulkan-idle requirement is part of `Context::create`'s contract.
        let code = unsafe { byro_fsr3_context_destroy(&mut raw) };
        if code != 0 {
            eprintln!("failed to destroy FSR context: {}", Error { code });
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
}
