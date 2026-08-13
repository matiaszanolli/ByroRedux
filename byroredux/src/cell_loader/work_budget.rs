//! Shared main-thread deadline budget for resumable cell application.
//!
//! The budget is cooperative: a unit that already started is allowed to
//! finish, then the next unit observes the deadline and yields. This guarantees
//! forward progress even when one NIF import or placed reference exceeds the
//! nominal frame allowance.

use std::time::Instant;

#[derive(Debug, Clone)]
pub(crate) struct FrameTimeBudget {
    deadline: Option<Instant>,
    completed_units: usize,
}

impl FrameTimeBudget {
    /// Budget bounded by an already-chosen instant.
    ///
    /// Takes the deadline rather than a duration so one frame's streaming
    /// step can compute a single instant and hand it to every stage —
    /// resumable cell apply *and* the distant-LOD providers — bounding the
    /// whole step once instead of each stage starting a fresh allowance from
    /// wherever the previous one happened to finish (EX-07 / #2376).
    pub(crate) fn until(deadline: Instant) -> Self {
        Self {
            deadline: Some(deadline),
            completed_units: 0,
        }
    }

    pub(crate) fn unlimited() -> Self {
        Self {
            deadline: None,
            completed_units: 0,
        }
    }

    /// Whether another work unit should be deferred to the next frame.
    ///
    /// The first unit is always admitted, even for a zero-duration budget, so
    /// an unexpectedly expensive indivisible unit cannot deadlock the queue.
    pub(crate) fn should_yield(&self) -> bool {
        self.completed_units > 0
            && self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub(crate) fn complete_unit(&mut self) {
        self.completed_units += 1;
    }

    pub(crate) fn completed_units(&self) -> usize {
        self.completed_units
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_duration_budget_still_admits_one_unit() {
        let mut budget = FrameTimeBudget::until(Instant::now());
        assert!(!budget.should_yield(), "first unit guarantees progress");
        budget.complete_unit();
        assert!(
            budget.should_yield(),
            "subsequent work yields after deadline"
        );
    }

    #[test]
    fn unlimited_budget_never_yields() {
        let mut budget = FrameTimeBudget::unlimited();
        for _ in 0..10_000 {
            assert!(!budget.should_yield());
            budget.complete_unit();
        }
        assert_eq!(budget.completed_units(), 10_000);
    }
}
