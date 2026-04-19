//! SPIR-V → descriptor layout reflection (issue #427).
//!
//! Hand-written `DescriptorSetLayoutBinding` arrays in pipeline modules drift
//! silently against `layout(set=N, binding=M)` declarations in the GLSL
//! source, because Vulkan only validates count/type overlap at pipeline
//! creation — a binding-index mismatch passes `vkCreateDescriptorSetLayout`
//! without warning and only surfaces as a wrong-read at draw time.
//!
//! This module parses the SPIR-V at startup and cross-checks every descriptor
//! declared in the shader against the hand-written layout. Every pipeline
//! calls [`validate_set_layout`] before `vkCreateDescriptorSetLayout`; a
//! mismatch returns a descriptive `anyhow::Error` which the caller `.expect()`s
//! so startup fails loudly.
//!
//! Supported descriptor types (the full set the engine uses today):
//! - `UNIFORM_BUFFER` — `OpTypeStruct` with `Block`, storage class `Uniform`
//! - `STORAGE_BUFFER` — `OpTypeStruct` with `BufferBlock` or storage class
//!   `StorageBuffer`
//! - `COMBINED_IMAGE_SAMPLER` — `OpTypeSampledImage`
//! - `SAMPLED_IMAGE` — `OpTypeImage` with `sampled=1`
//! - `STORAGE_IMAGE` — `OpTypeImage` with `sampled=2`
//! - `ACCELERATION_STRUCTURE_KHR` — `OpTypeAccelerationStructureKHR`
//! - `SAMPLER` — `OpTypeSampler`
//!
//! A `descriptor_count` of 0 in the reflected output means the shader declared
//! an `OpTypeRuntimeArray` (bindless); the Vulkan layout can declare any
//! count in that case.

use anyhow::{anyhow, bail, Result};
use ash::vk;
use rspirv::binary;
use rspirv::dr::{Instruction, Loader};
use rspirv::spirv::{Decoration, Op, StorageClass};
use std::collections::HashMap;

/// A single descriptor declaration extracted from a SPIR-V shader module.
#[derive(Debug, Clone, Copy)]
pub struct ReflectedBinding {
    pub set: u32,
    pub binding: u32,
    pub descriptor_type: vk::DescriptorType,
    /// 0 = `OpTypeRuntimeArray` (bindless); otherwise the fixed array length
    /// (or 1 for scalar descriptors).
    pub count: u32,
}

/// One shader stage passed to [`validate_set_layout`].
pub struct ReflectedShader<'a> {
    pub name: &'a str,
    pub spirv: &'a [u8],
}

#[derive(Default)]
struct DecoInfo {
    set: Option<u32>,
    binding: Option<u32>,
    is_block: bool,
    is_buffer_block: bool,
}

/// Parse the SPIR-V module and return every `(set, binding, type, count)` it
/// declares. Non-descriptor `OpVariable`s (inputs, outputs, push constants,
/// private workgroup data) are skipped.
pub fn reflect_bindings(spirv_bytes: &[u8]) -> Result<Vec<ReflectedBinding>> {
    if spirv_bytes.len() % 4 != 0 {
        bail!(
            "SPIR-V byte length {} is not a multiple of 4",
            spirv_bytes.len()
        );
    }
    let mut loader = Loader::new();
    binary::parse_bytes(spirv_bytes, &mut loader)
        .map_err(|e| anyhow!("SPIR-V parse failed: {e:?}"))?;
    let module = loader.module();

    // Pass 1: collect DescriptorSet/Binding/Block/BufferBlock decorations.
    let mut decos: HashMap<u32, DecoInfo> = HashMap::new();
    for inst in &module.annotations {
        if inst.class.opcode != Op::Decorate {
            continue;
        }
        if inst.operands.len() < 2 {
            continue;
        }
        let target = inst.operands[0].unwrap_id_ref();
        let deco = inst.operands[1].unwrap_decoration();
        let entry = decos.entry(target).or_default();
        match deco {
            Decoration::DescriptorSet => {
                if let Some(op) = inst.operands.get(2) {
                    entry.set = Some(op.unwrap_literal_bit32());
                }
            }
            Decoration::Binding => {
                if let Some(op) = inst.operands.get(2) {
                    entry.binding = Some(op.unwrap_literal_bit32());
                }
            }
            Decoration::Block => entry.is_block = true,
            Decoration::BufferBlock => entry.is_buffer_block = true,
            _ => {}
        }
    }

    // Pass 2: index every type / constant / global variable by id.
    let mut types: HashMap<u32, Instruction> = HashMap::new();
    for inst in &module.types_global_values {
        if let Some(id) = inst.result_id {
            types.insert(id, inst.clone());
        }
    }

    // Pass 3: resolve each decorated OpVariable to a ReflectedBinding.
    let mut out = Vec::new();
    for inst in &module.types_global_values {
        if inst.class.opcode != Op::Variable {
            continue;
        }
        let Some(var_id) = inst.result_id else { continue };
        let Some(deco) = decos.get(&var_id) else {
            continue;
        };
        let (Some(set), Some(binding)) = (deco.set, deco.binding) else {
            continue;
        };

        let storage_class = inst.operands[0].unwrap_storage_class();
        let ptr_type_id = inst
            .result_type
            .ok_or_else(|| anyhow!("OpVariable id={var_id} has no result_type"))?;
        let ptr_ty = types
            .get(&ptr_type_id)
            .ok_or_else(|| anyhow!("pointer type id={ptr_type_id} not found"))?;
        if ptr_ty.class.opcode != Op::TypePointer {
            bail!(
                "OpVariable id={var_id} result_type is not OpTypePointer (got {:?})",
                ptr_ty.class.opcode
            );
        }
        // OpTypePointer operands: [storage_class, pointee_type_id]
        let pointee_id = ptr_ty.operands[1].unwrap_id_ref();
        let (descriptor_type, count) =
            resolve_descriptor_type(pointee_id, storage_class, &types, &decos)?;

        out.push(ReflectedBinding {
            set,
            binding,
            descriptor_type,
            count,
        });
    }
    Ok(out)
}

fn resolve_descriptor_type(
    type_id: u32,
    storage_class: StorageClass,
    types: &HashMap<u32, Instruction>,
    decos: &HashMap<u32, DecoInfo>,
) -> Result<(vk::DescriptorType, u32)> {
    let inst = types
        .get(&type_id)
        .ok_or_else(|| anyhow!("type id={type_id} not found"))?;
    match inst.class.opcode {
        Op::TypeArray => {
            // OpTypeArray: [element_type_id, length_const_id]
            let elem_id = inst.operands[0].unwrap_id_ref();
            let len_const_id = inst.operands[1].unwrap_id_ref();
            let len_inst = types
                .get(&len_const_id)
                .ok_or_else(|| anyhow!("array length const id={len_const_id} not found"))?;
            // OpConstant operands: [literal_value] (result_type is on the instruction).
            let len = len_inst.operands[0].unwrap_literal_bit32();
            let (descriptor_type, _) =
                resolve_descriptor_type(elem_id, storage_class, types, decos)?;
            Ok((descriptor_type, len))
        }
        Op::TypeRuntimeArray => {
            let elem_id = inst.operands[0].unwrap_id_ref();
            let (descriptor_type, _) =
                resolve_descriptor_type(elem_id, storage_class, types, decos)?;
            // 0 = bindless / VARIABLE_DESCRIPTOR_COUNT. Layout can pick any size.
            Ok((descriptor_type, 0))
        }
        Op::TypeStruct => {
            let d = decos.get(&type_id);
            let is_block = d.is_some_and(|d| d.is_block);
            let is_buffer_block = d.is_some_and(|d| d.is_buffer_block);
            match (storage_class, is_buffer_block, is_block) {
                // Legacy SPIR-V 1.0: uniform buffers are Block+Uniform,
                // storage buffers are BufferBlock+Uniform.
                (StorageClass::Uniform, false, true) => {
                    Ok((vk::DescriptorType::UNIFORM_BUFFER, 1))
                }
                (StorageClass::Uniform, true, _) => Ok((vk::DescriptorType::STORAGE_BUFFER, 1)),
                // SPIR-V 1.3+: storage buffers use StorageBuffer storage class + Block.
                (StorageClass::StorageBuffer, _, _) => Ok((vk::DescriptorType::STORAGE_BUFFER, 1)),
                _ => bail!(
                    "unsupported struct descriptor at type id={type_id}: storage_class={storage_class:?}, block={is_block}, buffer_block={is_buffer_block}"
                ),
            }
        }
        Op::TypeSampledImage => Ok((vk::DescriptorType::COMBINED_IMAGE_SAMPLER, 1)),
        Op::TypeImage => {
            // OpTypeImage: sampled_type, dim, depth, arrayed, ms, sampled, format, [access_qualifier]
            let sampled = inst.operands[5].unwrap_literal_bit32();
            match sampled {
                1 => Ok((vk::DescriptorType::SAMPLED_IMAGE, 1)),
                2 => Ok((vk::DescriptorType::STORAGE_IMAGE, 1)),
                other => bail!(
                    "OpTypeImage id={type_id} has unsupported `sampled` value {other} (expected 1 or 2)"
                ),
            }
        }
        Op::TypeAccelerationStructureKHR => {
            Ok((vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, 1))
        }
        Op::TypeSampler => Ok((vk::DescriptorType::SAMPLER, 1)),
        other => bail!(
            "unsupported descriptor type opcode {other:?} at type id={type_id}"
        ),
    }
}

/// Cross-check every binding in `expected` (for descriptor set `set`) against
/// the SPIR-V declarations in `shaders`. Called from pipeline modules right
/// before `vkCreateDescriptorSetLayout`.
///
/// Fails if:
/// - a shader declares a binding at `(set, binding)` that is not in `expected`
///   (unless that binding is listed in `optional_shader_bindings`)
/// - an `expected` binding is not declared in any shader
/// - a shader declares a binding with a different `VkDescriptorType` than `expected`
/// - a shader declares a fixed-length array with a length different from `expected.descriptor_count`
///
/// `SPIR-V descriptor_count == 0` (runtime array / bindless) is compatible
/// with any `expected.descriptor_count`.
///
/// `optional_shader_bindings` is for shader bindings that are always declared
/// in source but only populated by the engine at runtime (e.g. the TLAS
/// binding in `triangle.frag` when ray tracing is disabled at device level).
pub fn validate_set_layout(
    set: u32,
    expected: &[vk::DescriptorSetLayoutBinding],
    shaders: &[ReflectedShader],
    layout_name: &str,
    optional_shader_bindings: &[u32],
) -> Result<()> {
    let mut declared: HashMap<u32, Vec<(ReflectedBinding, String)>> = HashMap::new();
    for shader in shaders {
        let reflected = reflect_bindings(shader.spirv).map_err(|e| {
            anyhow!(
                "{layout_name}: reflection failed for shader '{}': {e}",
                shader.name
            )
        })?;
        for b in reflected {
            if b.set != set {
                continue;
            }
            declared
                .entry(b.binding)
                .or_default()
                .push((b, shader.name.to_string()));
        }
    }

    for e in expected {
        let Some(found) = declared.get(&e.binding) else {
            bail!(
                "{layout_name}: expected binding set={set} binding={} is not declared in any shader",
                e.binding
            );
        };
        for (b, shader_name) in found {
            if b.descriptor_type != e.descriptor_type {
                bail!(
                    "{layout_name}: shader '{shader_name}' declares set={set} binding={} as {:?}, but layout specifies {:?}",
                    e.binding,
                    b.descriptor_type,
                    e.descriptor_type
                );
            }
            if b.count != 0 && b.count != e.descriptor_count {
                bail!(
                    "{layout_name}: shader '{shader_name}' declares set={set} binding={} with count {}, but layout specifies {}",
                    e.binding,
                    b.count,
                    e.descriptor_count
                );
            }
        }
    }

    for (binding, found) in &declared {
        if expected.iter().any(|e| e.binding == *binding) {
            continue;
        }
        if optional_shader_bindings.contains(binding) {
            continue;
        }
        let shader_name = &found[0].1;
        bail!(
            "{layout_name}: shader '{shader_name}' declares set={set} binding={binding} but the hand-written layout has no such entry"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tiny helper: build a trivial SPIR-V module exposing exactly one descriptor.
    // Not reproduced here — real tests use the actual engine shaders via
    // crates/renderer/shaders/*.spv loaded at test time. See the validation
    // test below.
    const SSAO_SPV: &[u8] = include_bytes!("../../shaders/ssao.comp.spv");

    #[test]
    fn reflect_ssao_bindings() {
        // ssao.comp:
        //   set=0 binding=0 uniform sampler2D depthTex
        //   set=0 binding=1, r8 uniform writeonly image2D aoOutput
        //   set=0 binding=2 uniform SSAOParams { ... }
        let bindings = reflect_bindings(SSAO_SPV).expect("reflect ssao.comp");
        let mut by_binding: HashMap<u32, ReflectedBinding> =
            bindings.iter().map(|b| (b.binding, *b)).collect();
        let depth = by_binding.remove(&0).expect("binding 0 present");
        assert_eq!(depth.set, 0);
        assert_eq!(
            depth.descriptor_type,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER
        );
        let ao = by_binding.remove(&1).expect("binding 1 present");
        assert_eq!(ao.descriptor_type, vk::DescriptorType::STORAGE_IMAGE);
        let params = by_binding.remove(&2).expect("binding 2 present");
        assert_eq!(params.descriptor_type, vk::DescriptorType::UNIFORM_BUFFER);
        assert!(by_binding.is_empty(), "unexpected extra bindings");
    }

    #[test]
    fn validate_ssao_layout_matches() {
        let expected = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &expected,
            &[ReflectedShader {
                name: "ssao.comp",
                spirv: SSAO_SPV,
            }],
            "ssao",
            &[],
        )
        .expect("ssao layout should match its shader");
    }

    #[test]
    fn validate_rejects_wrong_descriptor_type() {
        // Swap binding 1 from STORAGE_IMAGE to STORAGE_BUFFER — the synthetic
        // mismatch the issue calls for.
        let mismatched = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER) // wrong
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let err = validate_set_layout(
            0,
            &mismatched,
            &[ReflectedShader {
                name: "ssao.comp",
                spirv: SSAO_SPV,
            }],
            "ssao",
            &[],
        )
        .expect_err("swapped descriptor type must fail");
        let msg = format!("{err}");
        assert!(msg.contains("ssao.comp"), "message names the shader: {msg}");
        assert!(
            msg.contains("binding=1"),
            "message names the wrong binding: {msg}"
        );
        assert!(
            msg.contains("STORAGE_IMAGE"),
            "message mentions declared type: {msg}"
        );
        assert!(
            msg.contains("STORAGE_BUFFER"),
            "message mentions expected type: {msg}"
        );
    }

    #[test]
    fn validate_rejects_missing_layout_entry() {
        // Layout drops binding 2 — shader still declares it.
        let short = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let err = validate_set_layout(
            0,
            &short,
            &[ReflectedShader {
                name: "ssao.comp",
                spirv: SSAO_SPV,
            }],
            "ssao",
            &[],
        )
        .expect_err("dropped binding must fail");
        let msg = format!("{err}");
        assert!(msg.contains("binding=2"), "message names missing binding: {msg}");
    }
}
