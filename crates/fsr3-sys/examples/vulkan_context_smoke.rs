//! Creates and destroys an FSR 3.1 Vulkan context without dispatching a frame.
//!
//! Run with validation enabled:
//! `BYRO_VALIDATION=1 cargo run -p byroredux-fsr3-sys --example vulkan_context_smoke`

use ash::vk::{self, Handle};
use byroredux_fsr3_sys::{Context, VulkanCreateInfo};
use std::error::Error;
use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicUsize, Ordering};

static VALIDATION_ERRORS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn validation_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut c_void,
) -> vk::Bool32 {
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        VALIDATION_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    if !callback_data.is_null() {
        // SAFETY: Vulkan owns the callback data and message for this call.
        let message = unsafe {
            let pointer = (*callback_data).p_message;
            (!pointer.is_null()).then(|| CStr::from_ptr(pointer).to_string_lossy())
        };
        if let Some(message) = message {
            eprintln!("Vulkan validation: {message}");
        }
    }
    vk::FALSE
}

fn main() -> Result<(), Box<dyn Error>> {
    VALIDATION_ERRORS.store(0, Ordering::Relaxed);
    // SAFETY: the loader, instance, device, and FSR context lifetimes are
    // explicitly nested below and destroyed in reverse order.
    unsafe { run()? };
    let errors = VALIDATION_ERRORS.load(Ordering::Relaxed);
    if errors != 0 {
        return Err(
            format!("FSR context smoke test emitted {errors} Vulkan validation error(s)").into(),
        );
    }
    Ok(())
}

unsafe fn run() -> Result<(), Box<dyn Error>> {
    // SAFETY: dynamically loads the process Vulkan loader.
    let entry = unsafe { ash::Entry::load()? };
    let validation = std::env::var_os("BYRO_VALIDATION").is_some();

    let app_info = vk::ApplicationInfo::default()
        .application_name(c"ByroRedux FSR smoke")
        .engine_name(c"ByroRedux")
        .api_version(vk::API_VERSION_1_3);

    let mut extensions = Vec::new();
    let mut layers = Vec::new();
    if validation {
        let available = unsafe { entry.enumerate_instance_layer_properties()? };
        let has_validation = available.iter().any(|layer| unsafe {
            CStr::from_ptr(layer.layer_name.as_ptr()) == c"VK_LAYER_KHRONOS_validation"
        });
        if !has_validation {
            return Err(
                "BYRO_VALIDATION requested but VK_LAYER_KHRONOS_validation is unavailable".into(),
            );
        }
        extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        layers.push(c"VK_LAYER_KHRONOS_validation".as_ptr());
    }

    let enabled_validation = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
    let mut validation_features =
        vk::ValidationFeaturesEXT::default().enabled_validation_features(&enabled_validation);
    let mut instance_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layers);
    if validation {
        instance_info = instance_info.push_next(&mut validation_features);
    }
    let instance = unsafe { entry.create_instance(&instance_info, None)? };

    let debug = if validation {
        let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(validation_callback));
        let messenger = unsafe { loader.create_debug_utils_messenger(&info, None)? };
        Some((loader, messenger))
    } else {
        None
    };

    let result = unsafe { create_and_destroy_context(&instance) };

    if let Some((loader, messenger)) = debug {
        unsafe { loader.destroy_debug_utils_messenger(messenger, None) };
    }
    unsafe { instance.destroy_instance(None) };
    result
}

unsafe fn create_and_destroy_context(instance: &ash::Instance) -> Result<(), Box<dyn Error>> {
    let (physical_device, queue_family) = unsafe { instance.enumerate_physical_devices()? }
        .into_iter()
        .find_map(|physical_device| {
            let properties = unsafe { instance.get_physical_device_properties(physical_device) };
            if properties.api_version < vk::API_VERSION_1_3 {
                return None;
            }
            let queue_family = unsafe {
                instance
                    .get_physical_device_queue_family_properties(physical_device)
                    .iter()
                    .position(|queue| queue.queue_flags.contains(vk::QueueFlags::COMPUTE))
            }?;
            Some((physical_device, queue_family as u32))
        })
        .ok_or("no Vulkan 1.3 compute-capable physical device")?;

    let available_extensions =
        unsafe { instance.enumerate_device_extension_properties(physical_device)? };
    let supports_extension = |name: &CStr| {
        available_extensions
            .iter()
            .any(|property| unsafe { CStr::from_ptr(property.extension_name.as_ptr()) == name })
    };
    // v1.1.4 enumerates physical-device extension support, then assumes the
    // corresponding entry points/features were enabled on the logical device.
    // Mirror that contract for every backend capability FSR can select.
    let extension_candidates = [
        ash::khr::dedicated_allocation::NAME,
        ash::khr::get_memory_requirements2::NAME,
        ash::khr::synchronization2::NAME,
        ash::khr::shader_float16_int8::NAME,
        ash::ext::subgroup_size_control::NAME,
        ash::ext::descriptor_indexing::NAME,
        ash::amd::buffer_marker::NAME,
        ash::amd::device_coherent_memory::NAME,
    ];
    let enabled_extensions: Vec<_> = extension_candidates
        .into_iter()
        .filter(|name| supports_extension(name))
        .map(CStr::as_ptr)
        .collect();
    let coherent_memory_supported = supports_extension(ash::amd::device_coherent_memory::NAME);

    // The v1.1.4 Vulkan backend derives its shader permutation from physical
    // device capabilities. Enable every supported core feature in the same
    // Vulkan 1.1/1.2/1.3 feature families so its choice matches the logical
    // device. The production renderer will narrow this to the explicit FSR
    // contract when device integration lands.
    let mut vulkan11 = vk::PhysicalDeviceVulkan11Features::default();
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut coherent_memory = vk::PhysicalDeviceCoherentMemoryFeaturesAMD::default();
    let mut features = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut vulkan11)
        .push_next(&mut vulkan12)
        .push_next(&mut vulkan13);
    if coherent_memory_supported {
        features = features.push_next(&mut coherent_memory);
    }
    unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
    let base_features = features.features;
    vulkan11.p_next = std::ptr::null_mut();
    vulkan12.p_next = std::ptr::null_mut();
    vulkan13.p_next = std::ptr::null_mut();
    coherent_memory.p_next = std::ptr::null_mut();

    let priorities = [1.0];
    let queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family)
        .queue_priorities(&priorities);
    let mut device_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(std::slice::from_ref(&queue_info))
        .enabled_extension_names(&enabled_extensions)
        .enabled_features(&base_features)
        .push_next(&mut vulkan11)
        .push_next(&mut vulkan12)
        .push_next(&mut vulkan13);
    if coherent_memory_supported {
        device_info = device_info.push_next(&mut coherent_memory);
    }
    let device = unsafe { instance.create_device(physical_device, &device_info, None)? };

    let create_result = unsafe {
        Context::create(VulkanCreateInfo {
            device: device.handle().as_raw() as usize,
            physical_device: physical_device.as_raw() as usize,
            get_device_proc_addr: instance.fp_v1_0().get_device_proc_addr as *const ()
                as *const c_void,
            max_render_size: [1280, 720],
            max_upscale_size: [1920, 1080],
            high_dynamic_range: true,
            debug_checking: true,
        })
    };

    let result = match create_result {
        Ok(context) => {
            let wait = unsafe { device.device_wait_idle() };
            drop(context);
            wait.map_err(|error| Box::new(error) as Box<dyn Error>)
        }
        Err(error) => Err(Box::new(error) as Box<dyn Error>),
    };
    unsafe { device.destroy_device(None) };
    result?;

    println!(
        "FSR {} Vulkan context create/destroy passed",
        byroredux_fsr3_sys::version()?
    );
    Ok(())
}
