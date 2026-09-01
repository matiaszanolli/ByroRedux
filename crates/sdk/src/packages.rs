//! Portable callback-local AI package state and reevaluation requests.

use thiserror::Error;

use crate::identity::{EntityRef, FormRef};

/// Maximum authored candidates retained for one package selection.
pub const MAX_PACKAGE_CANDIDATES: usize = 256;
/// Maximum ambient plus scene package selections exposed for one actor.
pub const MAX_PACKAGE_SELECTIONS_PER_ENTITY: usize = 64;
/// Maximum aggregate portable form references exposed for one actor.
pub const MAX_PACKAGE_REFERENCES_PER_ENTITY: usize = 1_024;

/// Engine subsystem that owns one active package selection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PackageSelectionSource {
    Ambient,
    Scene,
}

/// One ordered authored candidate stack and its current winner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageSelection {
    source: PackageSelectionSource,
    scene: Option<FormRef>,
    action_index: Option<u32>,
    candidates: Vec<FormRef>,
    active: Option<FormRef>,
    template: Option<FormRef>,
}

impl PackageSelection {
    pub fn ambient(
        candidates: Vec<FormRef>,
        active: Option<FormRef>,
    ) -> Result<Self, PackageError> {
        Self::new(
            PackageSelectionSource::Ambient,
            None,
            None,
            candidates,
            active,
            None,
        )
    }

    pub fn scene_action(
        scene: Option<FormRef>,
        action_index: u32,
        candidates: Vec<FormRef>,
        active: Option<FormRef>,
        template: Option<FormRef>,
    ) -> Result<Self, PackageError> {
        Self::new(
            PackageSelectionSource::Scene,
            scene,
            Some(action_index),
            candidates,
            active,
            template,
        )
    }

    fn new(
        source: PackageSelectionSource,
        scene: Option<FormRef>,
        action_index: Option<u32>,
        candidates: Vec<FormRef>,
        active: Option<FormRef>,
        template: Option<FormRef>,
    ) -> Result<Self, PackageError> {
        if candidates.len() > MAX_PACKAGE_CANDIDATES {
            return Err(PackageError::CandidateBudgetExceeded {
                maximum: MAX_PACKAGE_CANDIDATES,
            });
        }
        if candidates
            .iter()
            .chain(scene.iter())
            .chain(active.iter())
            .chain(template.iter())
            .any(|form| form.local() == 0)
        {
            return Err(PackageError::NullForm);
        }
        match source {
            PackageSelectionSource::Ambient
                if scene.is_some() || action_index.is_some() || template.is_some() =>
            {
                return Err(PackageError::InvalidSelectionShape)
            }
            PackageSelectionSource::Scene if action_index.is_none() => {
                return Err(PackageError::InvalidSelectionShape)
            }
            _ => {}
        }
        Ok(Self {
            source,
            scene,
            action_index,
            candidates,
            active,
            template,
        })
    }

    pub const fn source(&self) -> PackageSelectionSource {
        self.source
    }

    pub const fn scene(&self) -> Option<FormRef> {
        self.scene
    }

    pub const fn action_index(&self) -> Option<u32> {
        self.action_index
    }

    pub fn candidates(&self) -> &[FormRef] {
        &self.candidates
    }

    pub const fn active(&self) -> Option<FormRef> {
        self.active
    }

    pub const fn template(&self) -> Option<FormRef> {
        self.template
    }
}

/// Complete or explicitly truncated package state for one callback-visible actor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PackageSnapshot {
    selections: Vec<PackageSelection>,
    truncated: bool,
}

impl PackageSnapshot {
    pub fn new(selections: Vec<PackageSelection>, truncated: bool) -> Result<Self, PackageError> {
        if selections.len() > MAX_PACKAGE_SELECTIONS_PER_ENTITY {
            return Err(PackageError::SelectionBudgetExceeded {
                maximum: MAX_PACKAGE_SELECTIONS_PER_ENTITY,
            });
        }
        let references = selections
            .iter()
            .try_fold(0_usize, |total, selection| {
                total.checked_add(
                    selection.candidates.len()
                        + usize::from(selection.scene.is_some())
                        + usize::from(selection.active.is_some())
                        + usize::from(selection.template.is_some()),
                )
            })
            .ok_or(PackageError::ReferenceBudgetExceeded {
                maximum: MAX_PACKAGE_REFERENCES_PER_ENTITY,
            })?;
        if references > MAX_PACKAGE_REFERENCES_PER_ENTITY {
            return Err(PackageError::ReferenceBudgetExceeded {
                maximum: MAX_PACKAGE_REFERENCES_PER_ENTITY,
            });
        }
        let ambient_positions = selections
            .iter()
            .enumerate()
            .filter_map(|(index, selection)| {
                (selection.source == PackageSelectionSource::Ambient).then_some(index)
            })
            .collect::<Vec<_>>();
        if ambient_positions.len() > 1 {
            return Err(PackageError::MultipleAmbientSelections);
        }
        if ambient_positions.first().is_some_and(|&index| index != 0) {
            return Err(PackageError::AmbientSelectionMustComeFirst);
        }
        Ok(Self {
            selections,
            truncated,
        })
    }

    pub fn selections(&self) -> &[PackageSelection] {
        &self.selections
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Deferred request to rerun every package selector observing an actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluatePackageCommand {
    entity: EntityRef,
}

impl EvaluatePackageCommand {
    pub const fn new(entity: EntityRef) -> Self {
        Self { entity }
    }

    pub const fn entity(self) -> EntityRef {
        self.entity
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PackageError {
    #[error("package form identity reserves local zero")]
    NullForm,
    #[error("package selection source metadata has an invalid shape")]
    InvalidSelectionShape,
    #[error("package selection exceeds the candidate limit of {maximum}")]
    CandidateBudgetExceeded { maximum: usize },
    #[error("package snapshot exceeds the selection limit of {maximum}")]
    SelectionBudgetExceeded { maximum: usize },
    #[error("package snapshot exceeds the aggregate reference limit of {maximum}")]
    ReferenceBudgetExceeded { maximum: usize },
    #[error("package snapshot may contain at most one ambient selection")]
    MultipleAmbientSelections,
    #[error("an ambient package selection must precede scene selections")]
    AmbientSelectionMustComeFirst,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(local: u32) -> FormRef {
        FormRef::new([1; 16], local)
    }

    #[test]
    fn package_snapshots_preserve_priority_and_source_metadata() {
        let ambient = PackageSelection::ambient(vec![form(3), form(1)], Some(form(1))).unwrap();
        let scene = PackageSelection::scene_action(
            Some(form(9)),
            4,
            vec![form(7)],
            Some(form(7)),
            Some(form(8)),
        )
        .unwrap();
        let snapshot = PackageSnapshot::new(vec![ambient, scene], true).unwrap();
        assert_eq!(snapshot.selections()[0].candidates(), &[form(3), form(1)]);
        assert_eq!(snapshot.selections()[1].action_index(), Some(4));
        assert!(snapshot.truncated());
        assert!(matches!(
            PackageSelection::ambient(vec![form(1); MAX_PACKAGE_CANDIDATES + 1], None),
            Err(PackageError::CandidateBudgetExceeded { .. })
        ));
        let oversized = (0..5)
            .map(|index| {
                PackageSelection::scene_action(
                    Some(form(20 + index)),
                    index,
                    vec![form(1); MAX_PACKAGE_CANDIDATES],
                    None,
                    None,
                )
                .unwrap()
            })
            .collect();
        assert!(matches!(
            PackageSnapshot::new(oversized, false),
            Err(PackageError::ReferenceBudgetExceeded { .. })
        ));
    }
}
