//! Runs the shipping convolution shader against known HDR inputs. This is
//! separate from the windowed smoke test so unrelated scene validation
//! errors cannot hide errors in this pass.
use super::*;

#[test]
#[ignore = "requires a Vulkan device; run with --ignored --exact"]
fn gpu_filter_preserves_constant_radiance_and_broadens_a_lobe() -> Result<()> {
    // SAFETY: this isolated test owns the complete instance/device lifetime.
    // Every submission is waited before readback, reuse, or destruction.
    unsafe {
        let entry = ash::Entry::load()?;
        let instance = entry.create_instance(
            &vk::InstanceCreateInfo::default()
                .application_info(&vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1)),
            None,
        )?;
        let (physical, family) = instance
            .enumerate_physical_devices()?
            .into_iter()
            .find_map(|p| {
                instance
                    .get_physical_device_queue_family_properties(p)
                    .iter()
                    .enumerate()
                    .find(|(_, q)| {
                        q.queue_flags
                            .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                    })
                    .map(|(i, _)| (p, i as u32))
            })
            .expect("graphics+compute device");
        let device = instance.create_device(
            physical,
            &vk::DeviceCreateInfo::default().queue_create_infos(&[
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(family)
                    .queue_priorities(&[1.0]),
            ]),
            None,
        )?;
        // Keep resources in an inner scope so all GpuImage/GpuBuffer Drop
        // safety nets execute before the logical device is destroyed.
        let result = run_filter_cases(&instance, &device, physical, family);
        device.device_wait_idle()?;
        device.destroy_device(None);
        instance.destroy_instance(None);
        result
    }
}

unsafe fn run_filter_cases(
    instance: &ash::Instance,
    device: &ash::Device,
    physical: vk::PhysicalDevice,
    family: u32,
) -> Result<()> {
    let allocator = crate::vulkan::allocator::create_allocator(instance, device, physical, false)?;
    let mut cube = GpuImage::create(
        device,
        &allocator,
        &GpuImageDesc {
            mip_levels: SKY_CUBE_MIP_LEVELS,
            ..GpuImageDesc::color_cube(
                "sky filter test",
                SKY_CUBE_FACE_SIZE,
                SKY_CUBE_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
        },
    )?;
    let mut readback = GpuBuffer::create_host_readback(
        device,
        &allocator,
        sky_cube_bytes_per_frame(),
        vk::BufferUsageFlags::TRANSFER_DST,
    )?;
    let sampler = device.create_sampler(
        &vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
        None,
    )?;
    let mut filter = SkyFilter::new(
        device,
        vk::PipelineCache::null(),
        std::slice::from_ref(&cube),
        sampler,
    )?;
    let mut irradiance = super::super::irradiance::SkyIrradiance::new(
        device,
        &allocator,
        vk::PipelineCache::null(),
        std::slice::from_ref(&cube),
        sampler,
    )?;
    let mut sh_readback = GpuBuffer::create_host_readback(
        device,
        &allocator,
        super::super::SKY_IRRADIANCE_BYTES,
        vk::BufferUsageFlags::TRANSFER_DST,
    )?;
    let pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let cmd = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1),
    )?[0];
    let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
    let queue = device.get_device_queue(family, 0);
    let base_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(CUBE_FACES);
    let mut case_outputs = Vec::new();
    let mut sh_outputs = Vec::new();
    for directional in [false, true] {
        device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        let begin = vk::ImageMemoryBarrier::default()
            .image(cube.image)
            .subresource_range(base_range)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[begin],
        );
        for face in 0..CUBE_FACES {
            // Exactly representable half floats, with values above 1 to
            // detect accidental clipping or tone mapping during convolution.
            let rgb = if directional && face != 0 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [2.0, 0.5, 4.0, 1.0]
            };
            device.cmd_clear_color_image(
                cmd,
                cube.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: rgb },
                &[vk::ImageSubresourceRange {
                    base_array_layer: face,
                    layer_count: 1,
                    ..base_range
                }],
            );
        }
        let ready = vk::ImageMemoryBarrier::default()
            .image(cube.image)
            .subresource_range(base_range)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[ready],
        );
        filter.record(device, cmd, cube.image, 0);
        irradiance.record(device, cmd, 0);
        let sh_to_copy = vk::BufferMemoryBarrier::default()
            .buffer(irradiance.buffers[0].buffer)
            .size(super::super::SKY_IRRADIANCE_BYTES)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[sh_to_copy],
            &[],
        );
        device.cmd_copy_buffer(
            cmd,
            irradiance.buffers[0].buffer,
            sh_readback.buffer,
            &[vk::BufferCopy::default().size(super::super::SKY_IRRADIANCE_BYTES)],
        );
        let to_copy = vk::ImageMemoryBarrier::default()
            .image(cube.image)
            .subresource_range(vk::ImageSubresourceRange {
                level_count: SKY_CUBE_MIP_LEVELS,
                ..base_range
            })
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_copy],
        );
        let mut offset = 0;
        let mut regions = Vec::new();
        for mip in 0..SKY_CUBE_MIP_LEVELS {
            let edge = SKY_CUBE_FACE_SIZE >> mip;
            regions.push(
                vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(mip)
                            .layer_count(CUBE_FACES),
                    )
                    .image_extent(vk::Extent3D {
                        width: edge,
                        height: edge,
                        depth: 1,
                    }),
            );
            offset += (edge * edge * CUBE_FACES * 8) as u64;
        }
        device.cmd_copy_image_to_buffer(
            cmd,
            cube.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            readback.buffer,
            &regions,
        );
        let host = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[host],
            &[],
            &[],
        );
        device.end_command_buffer(cmd)?;
        device.queue_submit(
            queue,
            &[vk::SubmitInfo::default().command_buffers(&[cmd])],
            fence,
        )?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        readback.invalidate_if_needed(device)?;
        sh_readback.invalidate_if_needed(device)?;
        case_outputs.push(readback.mapped_slice_mut()?.to_vec());
        sh_outputs.push(sh_readback.mapped_slice_mut()?.to_vec());
        device.reset_fences(&[fence])?;
        device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
    }
    // Tear down before assertions so a failed numeric check cannot strand
    // resources or obscure the assertion with a driver teardown failure.
    device.destroy_fence(fence, None);
    device.destroy_command_pool(pool, None);
    irradiance.destroy(device, &allocator);
    filter.destroy(device);
    device.destroy_sampler(sampler, None);
    sh_readback.destroy(device, &allocator);
    readback.destroy(device, &allocator);
    cube.destroy(device, &allocator);
    drop(allocator);

    let decode = |bytes: &[u8]| -> f32 {
        let bits = u16::from_ne_bytes([bytes[0], bytes[1]]);
        let exponent = ((bits >> 10) & 31) as i32;
        let mantissa = (bits & 1023) as f32;
        if exponent == 0 {
            mantissa * 2.0f32.powi(-24)
        } else {
            (1.0 + mantissa / 1024.0) * 2.0f32.powi(exponent - 15)
        }
    };
    let mut offset = 0;
    let mut peaks = Vec::new();
    for mip in 0..SKY_CUBE_MIP_LEVELS {
        let edge = (SKY_CUBE_FACE_SIZE >> mip) as usize;
        let count = edge * edge * CUBE_FACES as usize;
        for texel in 0..count {
            for (channel, expected) in [2.0, 0.5, 4.0, 1.0].iter().enumerate() {
                let start = offset + texel * 8 + channel * 2;
                let actual = decode(&case_outputs[0][start..start + 2]);
                assert!(
                    (actual - expected).abs() < 0.008,
                    "mip {mip} texel {texel}: {actual} != {expected}"
                );
            }
        }
        let center = offset + ((edge / 2) * edge + edge / 2) * 8;
        peaks.push(decode(&case_outputs[1][center..center + 2]));
        if mip == SKY_CUBE_MIP_LEVELS - 1 {
            let side = offset + 2 * edge * edge * 8; // +Y receives the broadened +X lobe.
            assert!(decode(&case_outputs[1][side..side + 2]) > 0.05);
        }
        offset += count * 8;
    }
    assert!(
        peaks[0] > 1.99 && peaks[7] < 1.5,
        "lobe failed to broaden: {peaks:?}"
    );
    // A 2x2 face has no texel on the major axis whereas the 1x1 face does.
    // Do not demand monotonic texel-centre samples across different directions.
    for pixel in case_outputs[1].chunks_exact(8) {
        let red = decode(pixel);
        assert!((0.0..=2.01).contains(&red), "filter created energy: {red}");
    }
    check_diffuse_projection(&sh_outputs);
    eprintln!("GPU GGX filter: constant HDR preserved across all mips; directional peak {peaks:?}");
    Ok(())
}

fn check_diffuse_projection(outputs: &[Vec<u8>]) {
    assert_eq!(outputs.len(), 2);
    let coefficients = |bytes: &[u8]| -> [[f64; 3]; 9] {
        std::array::from_fn(|i| {
            std::array::from_fn(|channel| {
                let begin = i * 16 + channel * 4;
                f32::from_ne_bytes(bytes[begin..begin + 4].try_into().unwrap()) as f64
            })
        })
    };
    let constant = coefficients(&outputs[0]);
    for normal in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        let actual = evaluate_irradiance(&constant, normal);
        for (value, expected) in actual.into_iter().zip([2.0, 0.5, 4.0]) {
            assert!(
                (value - expected).abs() < 0.004,
                "constant sky irradiance {value} != {expected}"
            );
        }
    }

    // The input lobe is one constant +X cube face. Compare the GPU's
    // low-order SH reconstruction with an independent cosine integral over
    // that face; this catches coefficient order, face orientation, and the
    // E/pi normalization instead of merely checking non-zero output.
    let directional = coefficients(&outputs[1]);
    for x in -2..=2 {
        for y in -2..=2 {
            for z in -2..=2 {
                if x == 0 && y == 0 && z == 0 {
                    continue;
                }
                let length = ((x * x + y * y + z * z) as f64).sqrt();
                let normal = [x as f64 / length, y as f64 / length, z as f64 / length];
                let reference = plus_x_face_irradiance(normal);
                let actual = evaluate_irradiance(&directional, normal);
                for (value, radiance) in actual.into_iter().zip([2.0, 0.5, 4.0]) {
                    let expected = radiance * reference;
                    assert!(
                        // A one-face step function has substantial energy
                        // above l=2, so its nine-coefficient reconstruction
                        // rings at the lobe boundary. This checks mapping and
                        // normalization, not exact encoding of a hard edge.
                        (value - expected).abs() < radiance * 0.04,
                        "directional sky normal {normal:?}: {value} != {expected}"
                    );
                }
            }
        }
    }
}

fn evaluate_irradiance(coefficients: &[[f64; 3]; 9], normal: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = normal;
    let basis = [
        0.2820947918,
        0.4886025119 * y,
        0.4886025119 * z,
        0.4886025119 * x,
        1.0925484306 * x * y,
        1.0925484306 * y * z,
        0.3153915653 * (3.0 * z * z - 1.0),
        1.0925484306 * x * z,
        0.5462742153 * (x * x - y * y),
    ];
    std::array::from_fn(|channel| {
        coefficients
            .iter()
            .zip(basis)
            .map(|(coefficient, basis)| coefficient[channel] * basis)
            .sum::<f64>()
            .max(0.0)
    })
}

fn plus_x_face_irradiance(normal: [f64; 3]) -> f64 {
    const EDGE: usize = 128;
    let mut sum = 0.0;
    for y in 0..EDGE {
        for x in 0..EDGE {
            let u = 2.0 * (x as f64 + 0.5) / EDGE as f64 - 1.0;
            let v = 2.0 * (y as f64 + 0.5) / EDGE as f64 - 1.0;
            let length = (1.0 + u * u + v * v).sqrt();
            // Matches the +X face mapping in sky_cube_direction.glsl.
            let direction = [1.0 / length, -v / length, -u / length];
            let cosine =
                (normal[0] * direction[0] + normal[1] * direction[1] + normal[2] * direction[2])
                    .max(0.0);
            sum += cosine / (length * length * length);
        }
    }
    // du * dv is 4 / EDGE^2 over the [-1, 1]^2 face domain.
    4.0 * sum / (std::f64::consts::PI * (EDGE * EDGE) as f64)
}
