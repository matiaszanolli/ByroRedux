//! Bounded spatial queries over live authored references.

use thiserror::Error;

use crate::identity::FormRef;

/// Maximum authored references retained in one live host snapshot.
pub const MAX_SPATIAL_REFERENCES: usize = 16_384;
/// Maximum references returned by one guest query.
pub const MAX_SPATIAL_QUERY_RESULTS: usize = 256;
/// Defensive upper bound for one radius query in renderer world units.
pub const MAX_SPATIAL_QUERY_RADIUS: f32 = 1_000_000.0;

/// One portable authored reference and its finite world position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialReference {
    form: FormRef,
    position: [f32; 3],
}

impl SpatialReference {
    pub fn new(form: FormRef, position: [f32; 3]) -> Result<Self, SpatialError> {
        if form.local() == 0 {
            return Err(SpatialError::NullForm);
        }
        if position.iter().any(|coordinate| !coordinate.is_finite()) {
            return Err(SpatialError::NonFinitePosition);
        }
        Ok(Self { form, position })
    }

    pub const fn form(self) -> FormRef {
        self.form
    }

    pub const fn position(self) -> [f32; 3] {
        self.position
    }
}

/// Immutable, deterministically ordered live-reference snapshot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpatialSnapshot {
    references: Vec<SpatialReference>,
    truncated: bool,
}

impl SpatialSnapshot {
    pub fn new(references: Vec<SpatialReference>, truncated: bool) -> Result<Self, SpatialError> {
        if references.len() > MAX_SPATIAL_REFERENCES {
            return Err(SpatialError::SnapshotBudgetExceeded {
                maximum: MAX_SPATIAL_REFERENCES,
            });
        }
        if references
            .windows(2)
            .any(|pair| pair[0].form() >= pair[1].form())
        {
            return Err(SpatialError::ReferencesNotStrictlySorted);
        }
        Ok(Self {
            references,
            truncated,
        })
    }

    pub fn references(&self) -> &[SpatialReference] {
        &self.references
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn nearby(
        &self,
        origin: [f32; 3],
        radius: f32,
        limit: usize,
    ) -> Result<SpatialQueryResult, SpatialError> {
        if origin.iter().any(|coordinate| !coordinate.is_finite()) {
            return Err(SpatialError::NonFinitePosition);
        }
        if !radius.is_finite() || !(0.0..=MAX_SPATIAL_QUERY_RADIUS).contains(&radius) {
            return Err(SpatialError::InvalidRadius);
        }
        if limit > MAX_SPATIAL_QUERY_RESULTS {
            return Err(SpatialError::ResultBudgetExceeded {
                maximum: MAX_SPATIAL_QUERY_RESULTS,
            });
        }
        let radius_squared = f64::from(radius).powi(2);
        let mut hits = self
            .references
            .iter()
            .filter_map(|reference| {
                let position = reference.position();
                let delta = [
                    f64::from(position[0]) - f64::from(origin[0]),
                    f64::from(position[1]) - f64::from(origin[1]),
                    f64::from(position[2]) - f64::from(origin[2]),
                ];
                let distance_squared = delta.into_iter().map(|value| value * value).sum::<f64>();
                (distance_squared <= radius_squared).then(|| SpatialHit {
                    reference: *reference,
                    distance: distance_squared.sqrt() as f32,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            left.distance
                .total_cmp(&right.distance)
                .then_with(|| left.reference.form().cmp(&right.reference.form()))
        });
        let truncated = self.truncated || hits.len() > limit;
        hits.truncate(limit);
        Ok(SpatialQueryResult { hits, truncated })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialHit {
    reference: SpatialReference,
    distance: f32,
}

impl SpatialHit {
    pub const fn reference(self) -> SpatialReference {
        self.reference
    }

    pub const fn distance(self) -> f32 {
        self.distance
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpatialQueryResult {
    hits: Vec<SpatialHit>,
    truncated: bool,
}

impl SpatialQueryResult {
    pub fn hits(&self) -> &[SpatialHit] {
        &self.hits
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum SpatialError {
    #[error("spatial reference form identity reserves local zero")]
    NullForm,
    #[error("spatial position contains a non-finite coordinate")]
    NonFinitePosition,
    #[error("spatial query radius must be finite and within the configured bound")]
    InvalidRadius,
    #[error("spatial snapshot exceeds the live-reference limit of {maximum}")]
    SnapshotBudgetExceeded { maximum: usize },
    #[error("spatial query exceeds the result limit of {maximum}")]
    ResultBudgetExceeded { maximum: usize },
    #[error("spatial references must be unique and strictly sorted by portable form identity")]
    ReferencesNotStrictlySorted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_is_distance_ordered_bounded_and_explicitly_truncated() {
        let near = SpatialReference::new(FormRef::new([1; 16], 1), [2.0, 0.0, 0.0]).unwrap();
        let far = SpatialReference::new(FormRef::new([2; 16], 1), [5.0, 0.0, 0.0]).unwrap();
        let snapshot = SpatialSnapshot::new(vec![near, far], false).unwrap();
        let result = snapshot.nearby([0.0; 3], 5.0, 1).unwrap();
        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].reference(), near);
        assert_eq!(result.hits()[0].distance(), 2.0);
        assert!(result.truncated());
        assert_eq!(
            snapshot.nearby([f32::NAN, 0.0, 0.0], 1.0, 1),
            Err(SpatialError::NonFinitePosition)
        );
    }
}
