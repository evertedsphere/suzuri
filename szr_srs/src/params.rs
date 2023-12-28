use chrono::Duration;

use crate::{review_grade::ReviewGrade, weights::Weights};

pub struct Params {
    /// Whether or not theoretical intervals should be rounded to days.
    pub(crate) round_to_days: bool,
    /// TODO poor naming
    pub(crate) first_interval: Duration,
    /// TODO poor naming
    pub(crate) second_interval: Duration,
    /// TODO poor naming
    pub(crate) third_interval: Duration,
    /// TODO poor naming
    pub(crate) interval_step: Duration,
    /// A lower bound on the initial stability.
    pub(crate) min_initial_stability: f64,
    /// Used to clamp the difficulty.
    pub(crate) min_difficulty: f64,
    /// Used to clamp the difficulty.
    pub(crate) max_difficulty: f64,
    /// Used to bound the due date of a card to hopefully within the user's lifetime.
    pub(crate) max_interval: i64,
    /// The FSRS formulation we use is tuned for 90% by default.
    pub(crate) target_retention: f64,
    /// Used to compute parameter updates.
    pub(crate) weights: Weights,
}

impl Params {
    pub fn from_weight_vector(w: [f64; 17]) -> Self {
        // TODO builder
        Self {
            round_to_days: false,
            target_retention: 0.9,
            first_interval: Duration::minutes(1),
            second_interval: Duration::minutes(5),
            third_interval: Duration::minutes(10),
            interval_step: Duration::days(1),
            min_initial_stability: 0.1,
            min_difficulty: 1.0,
            max_difficulty: 10.0,
            max_interval: 36500,
            weights: Weights::from_weight_vector(w),
        }
    }

    pub(crate) fn initial_stability(&self, grade: ReviewGrade) -> f64 {
        match grade {
            ReviewGrade::Fail => self.weights.init_stab_fail,
            ReviewGrade::Hard => self.weights.init_stab_hard,
            ReviewGrade::Okay => self.weights.init_stab_okay,
            ReviewGrade::Easy => self.weights.init_stab_easy,
        }
    }

    pub(crate) fn stability_pass_update_bonus(&self, grade: ReviewGrade) -> f64 {
        match grade {
            ReviewGrade::Hard => self.weights.stab_upd_pass_mult_hard,
            ReviewGrade::Easy => self.weights.stab_upd_pass_mult_easy,
            _ => 1.0,
        }
    }
}
