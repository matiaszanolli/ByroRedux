//! Package-scene-action analysis: procedure trees and what each resolves to.

use super::*;

/// Audit one Package scene action: its stack depth, phase completion
/// coupling, procedure trees, and the execution kind each tree resolves to.
pub(super) fn analyze_package_action(
    index: &EsmIndex,
    scene: &ScenRecord,
    action: &SceneAction,
    inventory: &mut SceneInventory,
) {
    inventory.package_actions += 1;
    *inventory
        .package_action_stacks
        .entry(action.packages.len())
        .or_default() += 1;
    if scene
        .phases
        .get(action.end_phase as usize)
        .is_some_and(|phase| !phase.completion_conditions.is_empty())
    {
        inventory.package_actions_with_phase_completion_conditions += 1;
    }
    for package_form_id in &action.packages {
        let Some(package) = index.packages.get(package_form_id) else {
            continue;
        };
        *inventory
            .package_procedures
            .entry(package.procedure_type)
            .or_default() += 1;
        push_sample(
            &mut inventory.package_procedure_samples,
            package.procedure_type,
            format!(
                "{}:{} {}",
                scene.editor_id, action.index, package.editor_id
            ),
        );
        let Some(tree) = resolve_package_template(index, scene, action, package, inventory) else {
            continue;
        };
        for procedure in &tree.procedures {
            *inventory
                .package_tree_procedures
                .entry(procedure.procedure_type.clone())
                .or_default() += 1;
            push_sample(
                &mut inventory.package_tree_procedure_samples,
                procedure.procedure_type.clone(),
                format!(
                    "{}:{} {} via {}{}",
                    scene.editor_id,
                    action.index,
                    package.editor_id,
                    tree.editor_id,
                    if procedure.success_completes_package() {
                        " [completes]"
                    } else {
                        ""
                    }
                ),
            );
        }
        if let Some(procedure) = tree
            .procedures
            .iter()
            .find(|procedure| procedure.success_completes_package())
            .or_else(|| tree.procedures.first())
        {
            let kind = package_execution_kind(index, scene, action, package, procedure, inventory);
            *inventory
                .package_execution_kinds
                .entry(kind.to_owned())
                .or_default() += 1;
        }
    }
}

/// Resolve the procedure tree a package executes: its template PACK when
/// authored, its own procedures when it is a custom tree, else backlog.
pub(super) fn resolve_package_template<'a>(
    index: &'a EsmIndex,
    scene: &ScenRecord,
    action: &SceneAction,
    package: &'a PackRecord,
    inventory: &mut SceneInventory,
) -> Option<&'a PackRecord> {
    match package.package_template_form_id {
        Some(template_form_id) => match index.packages.get(&template_form_id) {
            Some(template) => {
                *inventory
                    .package_templates
                    .entry(format!("{:08X} {}", template.form_id, template.editor_id))
                    .or_default() += 1;
                Some(template)
            }
            None => {
                inventory.unresolved_package_templates.push(format!(
                    "{}:{} {} -> {template_form_id:08X}",
                    scene.editor_id, action.index, package.editor_id
                ));
                None
            }
        },
        None if !package.procedures.is_empty() => Some(package),
        None => {
            inventory.unresolved_package_templates.push(format!(
                "{}:{} {} has no template/tree",
                scene.editor_id, action.index, package.editor_id
            ));
            None
        }
    }
}

/// Bucket a procedure tree's dominant procedure into the runtime execution
/// plan vocabulary.
pub(super) fn package_execution_kind(
    index: &EsmIndex,
    scene: &ScenRecord,
    action: &SceneAction,
    package: &PackRecord,
    procedure: &PackProcedure,
    inventory: &mut SceneInventory,
) -> &'static str {
    match procedure.procedure_type.as_str() {
        "Travel" | "Patrol" | "Escort" | "FollowTo" => {
            travel_execution_kind(index, scene, action, package, procedure, inventory)
        }
        "Acquire" | "Activate" | "Shout" | "Sit" | "UseIdleMarker" | "UseWeapon" => "interaction",
        _ => "await-external",
    }
}

/// Classify a travel-family procedure by whether its movement target
/// resolves to a known reference, the player, or a dynamic alias input.
pub(super) fn travel_execution_kind(
    index: &EsmIndex,
    scene: &ScenRecord,
    action: &SceneAction,
    package: &PackRecord,
    procedure: &PackProcedure,
    inventory: &mut SceneInventory,
) -> &'static str {
    let target = procedure_data_inputs(package, procedure).find_map(package_target_form_id);
    let Some(target) = target else {
        if procedure_data_inputs(package, procedure).any(package_target_is_dynamic) {
            return "move-dynamic";
        }
        inventory.unresolved_movement_targets.push(format!(
            "{}:{} {} {} has no resolvable target input",
            scene.editor_id,
            action.index,
            package.editor_id,
            procedure.procedure_type,
        ));
        return "await-target";
    };
    if target == 0x14 {
        return "move-dynamic";
    }
    if reference_resolves(index, target) {
        return "move-to";
    }
    inventory.unresolved_movement_targets.push(format!(
        "{}:{} {} {} -> {target:08X}",
        scene.editor_id,
        action.index,
        package.editor_id,
        procedure.procedure_type,
    ));
    "await-target"
}

/// The package data inputs a procedure's sparse PDTO indexes select.
pub(super) fn procedure_data_inputs<'a>(
    package: &'a PackRecord,
    procedure: &'a PackProcedure,
) -> impl Iterator<Item = &'a PackDataInput> {
    procedure
        .data_input_indexes
        .iter()
        .filter_map(move |index| package.data_inputs.iter().find(|input| input.index == *index))
}

/// FormID of a statically targeted input, when it carries one.
pub(super) fn package_target_form_id(input: &PackDataInput) -> Option<u32> {
    match input.value {
        PackDataValue::Location(location) => match location.target {
            PackLocationTarget::NearReference(form_id) => Some(form_id),
            _ => None,
        },
        PackDataValue::Target(target) => match target.target {
            PackDataTargetKind::SpecificReference(form_id)
            | PackDataTargetKind::LinkedReference(form_id) => Some(form_id),
            _ => None,
        },
        _ => None,
    }
}

/// Whether an input targets the actor's current location or an alias slot
/// the runtime fills at execution time.
pub(super) fn package_target_is_dynamic(input: &PackDataInput) -> bool {
    match input.value {
        PackDataValue::Location(location) => matches!(location.location_type, 2 | 3 | 8 | 9),
        PackDataValue::Target(target) => matches!(
            target.target,
            PackDataTargetKind::ReferenceAlias(_) | PackDataTargetKind::SelfTarget
        ),
        _ => false,
    }
}

/// Whether a FormID matches any placed reference the ESM index knows.
pub(super) fn reference_resolves(index: &EsmIndex, target: u32) -> bool {
    index.cells.cells.values().any(|cell| {
        cell.references.iter().any(|placed| placed.form_id == target)
    }) || index
        .cells
        .exterior_cells
        .values()
        .any(|grids| grids.values().any(|cell| cell.references.iter().any(|placed| placed.form_id == target)))
        || index
            .cells
            .worldspace_persistent_cells
            .values()
            .any(|cell| cell.references.iter().any(|placed| placed.form_id == target))
}

/// Keep at most five example strings per sample bucket.
pub(super) fn push_sample<K: Ord>(samples: &mut BTreeMap<K, Vec<String>>, key: K, text: String) {
    let bucket = samples.entry(key).or_default();
    if bucket.len() < 5 {
        bucket.push(text);
    }
}
