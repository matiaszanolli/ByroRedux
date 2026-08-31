//! What this machine can actually run, and which preset to suggest.
//!
//! The launcher renders on OpenGL precisely so it can survive a machine the
//! engine cannot run on. This module is what turns that survival into an
//! answer: it enumerates Vulkan adapters *without creating a logical device*,
//! so a machine that would fail `vkCreateDevice` still gets a readable
//! explanation instead of a failed launch.
//!
//! The probe and the decision are separated on purpose. [`probe`] touches the
//! driver and cannot be unit-tested; [`Capabilities::recommended_preset`] and
//! [`Capabilities::verdict`] are pure functions of what the probe found, and
//! are.

use std::ffi::CStr;

/// The engine's floor, from `crates/renderer/src/vulkan/instance.rs`.
const REQUIRED_API_MAJOR: u32 = 1;
const REQUIRED_API_MINOR: u32 = 3;

/// Ray tracing needs roughly this much device-local memory to hold the BLAS/TLAS
/// set for a real cell alongside textures. Below it the RT path thrashes, so the
/// launcher steers to the cheapest reconstruction rather than letting someone
/// pick Ultra and meet a device loss.
pub const RT_VRAM_FLOOR_BYTES: u64 = 6 * 1024 * 1024 * 1024;

/// Comfortable headroom for the engine's default (quality reconstruction).
pub const COMFORTABLE_VRAM_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// What kind of adapter this is.
///
/// Load-bearing for selection, not decoration. A software Vulkan
/// implementation (Mesa's lavapipe/llvmpipe, common on Linux and shipped by
/// some distros by default) reports *all of system RAM* as device-local — so a
/// "most memory wins" choice picks the CPU rasterizer over a discrete GPU and
/// then recommends Ultra off 30 GB of imaginary VRAM. Observed exactly that on
/// a machine with an RTX 4070 Ti installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceKind {
    /// Ranked lowest: it works, at seconds per frame.
    Cpu,
    Other,
    Virtual,
    Integrated,
    Discrete,
}

impl DeviceKind {
    fn from_vk(kind: ash::vk::PhysicalDeviceType) -> Self {
        match kind {
            ash::vk::PhysicalDeviceType::DISCRETE_GPU => Self::Discrete,
            ash::vk::PhysicalDeviceType::INTEGRATED_GPU => Self::Integrated,
            ash::vk::PhysicalDeviceType::VIRTUAL_GPU => Self::Virtual,
            ash::vk::PhysicalDeviceType::CPU => Self::Cpu,
            _ => Self::Other,
        }
    }
}

/// What one adapter offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub device_name: String,
    pub kind: DeviceKind,
    /// Highest API version the adapter supports, as `(major, minor)`.
    pub api_version: (u32, u32),
    /// Sum of device-local heaps.
    pub device_local_bytes: u64,
    pub ray_query: bool,
    pub acceleration_structure: bool,
}

/// Why the engine will not run, when it will not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    NoVulkan,
    NoAdapter,
    ApiTooOld {
        found: (u32, u32),
    },
    MissingExtension(&'static str),
    /// The only adapter is a software rasterizer. Reported as a blocker
    /// because it would run at seconds per frame, and because its reported
    /// memory is system RAM — so any preset recommendation from it is fiction.
    SoftwareOnly {
        device_name: String,
    },
}

impl Blocker {
    /// One sentence, aimed at someone who did not choose their GPU.
    pub fn explain(&self) -> String {
        match self {
            Blocker::NoVulkan => {
                "No Vulkan driver was found. Install or update your graphics drivers.".into()
            }
            Blocker::NoAdapter => "No graphics adapter reported Vulkan support.".into(),
            Blocker::ApiTooOld {
                found: (major, minor),
            } => format!(
                "This graphics driver reports Vulkan {major}.{minor}; the engine needs \
                 {REQUIRED_API_MAJOR}.{REQUIRED_API_MINOR}. Updating the driver often fixes this."
            ),
            Blocker::MissingExtension(name) => format!(
                "This graphics card does not support {name}, which the engine's lighting requires."
            ),
            Blocker::SoftwareOnly { device_name } => format!(
                "The only graphics device found is a software renderer ({device_name}). \
                 The engine would run, but far too slowly to play."
            ),
        }
    }
}

impl Capabilities {
    /// `Ok` when the engine can start here, `Err` with the reason when it
    /// cannot.
    pub fn verdict(&self) -> Result<(), Blocker> {
        if self.kind == DeviceKind::Cpu {
            return Err(Blocker::SoftwareOnly {
                device_name: self.device_name.clone(),
            });
        }
        if self.api_version < (REQUIRED_API_MAJOR, REQUIRED_API_MINOR) {
            return Err(Blocker::ApiTooOld {
                found: self.api_version,
            });
        }
        if !self.acceleration_structure {
            return Err(Blocker::MissingExtension("VK_KHR_acceleration_structure"));
        }
        if !self.ray_query {
            return Err(Blocker::MissingExtension("VK_KHR_ray_query"));
        }
        Ok(())
    }

    /// Preset slug to suggest for this adapter.
    ///
    /// Deliberately conservative: someone whose first launch runs badly does not
    /// come back to try a lower setting.
    pub fn recommended_preset(&self) -> &'static str {
        if self.device_local_bytes >= COMFORTABLE_VRAM_BYTES {
            "high"
        } else if self.device_local_bytes >= RT_VRAM_FLOOR_BYTES {
            "medium"
        } else {
            "low"
        }
    }

    /// One line for the settings screen.
    pub fn summary(&self) -> String {
        let (major, minor) = self.api_version;
        format!(
            "{} · {:.1} GB · Vulkan {major}.{minor}{}",
            self.device_name,
            self.device_local_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            if self.ray_query {
                " · ray query"
            } else {
                " · no ray query"
            }
        )
    }

    /// Whether the RT path has enough memory to be worth enabling.
    pub fn meets_rt_floor(&self) -> bool {
        self.device_local_bytes >= RT_VRAM_FLOOR_BYTES
    }
}

/// Enumerate adapters and describe the best one.
///
/// Creates a Vulkan *instance* and queries physical devices; it never creates a
/// logical device, which is the step that fails on an unsupported machine. That
/// is what lets this report a blocker rather than become one.
pub fn probe() -> Result<Capabilities, Blocker> {
    // SAFETY: `Entry::load` dynamically loads the system Vulkan loader, and the
    // enumeration calls below are read-only queries against handles this
    // function owns for the duration of the call.
    unsafe {
        let entry = ash::Entry::load().map_err(|_| Blocker::NoVulkan)?;
        let app_info = ash::vk::ApplicationInfo::default().api_version(ash::vk::API_VERSION_1_3);
        let create_info = ash::vk::InstanceCreateInfo::default().application_info(&app_info);
        let instance = entry
            .create_instance(&create_info, None)
            .map_err(|_| Blocker::NoVulkan)?;

        let devices = instance
            .enumerate_physical_devices()
            .map_err(|_| Blocker::NoAdapter)?;

        let best = devices
            .into_iter()
            .map(|device| describe(&instance, device))
            // Order matters: usable first, then by device class, then by
            // memory. Class must outrank memory — a software rasterizer claims
            // all of system RAM as device-local and would otherwise beat every
            // real GPU on the machine.
            .max_by_key(|caps| (caps.verdict().is_ok(), caps.kind, caps.device_local_bytes));

        instance.destroy_instance(None);
        best.ok_or(Blocker::NoAdapter)
    }
}

/// Read one adapter's properties. Caller holds the instance.
unsafe fn describe(instance: &ash::Instance, device: ash::vk::PhysicalDevice) -> Capabilities {
    let properties = instance.get_physical_device_properties(device);
    let memory = instance.get_physical_device_memory_properties(device);

    let device_local_bytes = memory.memory_heaps[..memory.memory_heap_count as usize]
        .iter()
        .filter(|heap| heap.flags.contains(ash::vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .sum();

    let extensions = instance
        .enumerate_device_extension_properties(device)
        .unwrap_or_default();
    let has = |wanted: &str| {
        extensions.iter().any(|extension| {
            CStr::from_ptr(extension.extension_name.as_ptr())
                .to_str()
                .is_ok_and(|name| name == wanted)
        })
    };

    Capabilities {
        device_name: CStr::from_ptr(properties.device_name.as_ptr())
            .to_string_lossy()
            .into_owned(),
        kind: DeviceKind::from_vk(properties.device_type),
        api_version: (
            ash::vk::api_version_major(properties.api_version),
            ash::vk::api_version_minor(properties.api_version),
        ),
        device_local_bytes,
        ray_query: has("VK_KHR_ray_query"),
        acceleration_structure: has("VK_KHR_acceleration_structure"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(api: (u32, u32), gigabytes: u64, rt: bool) -> Capabilities {
        Capabilities {
            device_name: "Test GPU".into(),
            kind: DeviceKind::Discrete,
            api_version: api,
            device_local_bytes: gigabytes * 1024 * 1024 * 1024,
            ray_query: rt,
            acceleration_structure: rt,
        }
    }

    /// Exercise the real `unsafe` enumeration path against this machine's
    /// driver. Ignored by default because it needs a Vulkan loader, which CI
    /// and the other tests here deliberately do not require — the decision
    /// logic above is what those cover. Run with:
    ///
    /// ```text
    /// cargo test -p byro-launcher -- --ignored --nocapture probe
    /// ```
    #[test]
    #[ignore = "needs a Vulkan driver"]
    fn probe_describes_this_machine() {
        match probe() {
            Ok(caps) => {
                println!("{}", caps.summary());
                println!("verdict: {:?}", caps.verdict());
                println!("recommended preset: {}", caps.recommended_preset());
                assert!(!caps.device_name.is_empty(), "adapter reported no name");
                assert!(
                    caps.device_local_bytes > 0,
                    "adapter reported no device-local memory"
                );
            }
            Err(blocker) => println!("no usable adapter: {}", blocker.explain()),
        }
    }

    #[test]
    fn a_capable_adapter_passes() {
        assert_eq!(caps((1, 3), 12, true).verdict(), Ok(()));
        assert_eq!(caps((1, 4), 12, true).verdict(), Ok(()));
    }

    /// Each blocker must be reported as itself, so the explanation names the
    /// actual problem — "update your driver" and "your card cannot do this" are
    /// very different messages to receive.
    /// Regression for a bug found only by running the probe on a real machine:
    /// llvmpipe was selected over an RTX 4070 Ti and reported 30 GB of "VRAM",
    /// because a software rasterizer maps all of system RAM as device-local.
    /// Device class must outrank memory size.
    #[test]
    fn a_software_rasterizer_never_outranks_a_real_gpu() {
        let mut software = caps((1, 4), 30, true);
        software.kind = DeviceKind::Cpu;
        software.device_name = "llvmpipe".into();
        let discrete = caps((1, 3), 12, true);

        let mut ranked = [software.clone(), discrete.clone()];
        ranked.sort_by_key(|caps| (caps.verdict().is_ok(), caps.kind, caps.device_local_bytes));
        assert_eq!(
            ranked.last().unwrap().device_name,
            "Test GPU",
            "the software rasterizer won on memory size"
        );

        // And on its own it is reported as unplayable rather than as a 30 GB
        // card that should run Ultra.
        assert_eq!(
            software.verdict(),
            Err(Blocker::SoftwareOnly {
                device_name: "llvmpipe".into()
            })
        );
    }

    /// An integrated GPU is a real device and must still be preferred over
    /// software, but never over a discrete one in the same machine.
    #[test]
    fn device_classes_rank_in_the_expected_order() {
        assert!(DeviceKind::Discrete > DeviceKind::Integrated);
        assert!(DeviceKind::Integrated > DeviceKind::Virtual);
        assert!(DeviceKind::Virtual > DeviceKind::Other);
        assert!(DeviceKind::Other > DeviceKind::Cpu);
    }

    #[test]
    fn each_blocker_is_reported_specifically() {
        assert_eq!(
            caps((1, 2), 12, true).verdict(),
            Err(Blocker::ApiTooOld { found: (1, 2) })
        );
        assert_eq!(
            caps((1, 3), 12, false).verdict(),
            Err(Blocker::MissingExtension("VK_KHR_acceleration_structure"))
        );

        let mut no_query = caps((1, 3), 12, true);
        no_query.ray_query = false;
        assert_eq!(
            no_query.verdict(),
            Err(Blocker::MissingExtension("VK_KHR_ray_query"))
        );
    }

    /// Every blocker explains itself in words a person who did not choose their
    /// GPU can act on — no bare enum name ever reaches the screen.
    #[test]
    fn every_blocker_has_a_plain_explanation() {
        for blocker in [
            Blocker::NoVulkan,
            Blocker::NoAdapter,
            Blocker::ApiTooOld { found: (1, 1) },
            Blocker::MissingExtension("VK_KHR_ray_query"),
            Blocker::SoftwareOnly {
                device_name: "llvmpipe".into(),
            },
        ] {
            let text = blocker.explain();
            assert!(text.len() > 20, "too terse: {text}");
            assert!(text.ends_with('.'), "not a sentence: {text}");
        }
    }

    /// The recommendation is conservative around the documented 6 GB RT floor:
    /// someone whose first launch runs badly does not come back to lower it.
    #[test]
    fn the_recommendation_steps_down_at_the_rt_floor() {
        assert_eq!(caps((1, 3), 12, true).recommended_preset(), "high");
        assert_eq!(caps((1, 3), 8, true).recommended_preset(), "high");
        assert_eq!(caps((1, 3), 6, true).recommended_preset(), "medium");
        assert_eq!(caps((1, 3), 4, true).recommended_preset(), "low");

        assert!(caps((1, 3), 6, true).meets_rt_floor());
        assert!(!caps((1, 3), 4, true).meets_rt_floor());
    }

    /// Every recommendation must name a preset the shipped file actually has,
    /// or the settings screen would suggest something it cannot select.
    #[test]
    fn every_recommendation_names_a_shipped_preset() {
        let path = std::path::Path::new("../../assets/graphics_presets.toml");
        if !path.exists() {
            return;
        }
        let file = byroredux_settings_io::presets::PresetFile::load(path);
        for gigabytes in [2, 4, 6, 8, 12, 24] {
            let slug = caps((1, 3), gigabytes, true).recommended_preset();
            assert!(
                file.presets.contains_key(slug),
                "{gigabytes} GB recommends `{slug}`, which is not in the shipped presets"
            );
        }
    }

    #[test]
    fn the_summary_names_the_device_memory_and_rt_support() {
        let text = caps((1, 3), 12, true).summary();
        assert!(text.contains("Test GPU"), "{text}");
        assert!(text.contains("12.0 GB"), "{text}");
        assert!(text.contains("Vulkan 1.3"), "{text}");
        assert!(text.contains("ray query"), "{text}");
    }
}
