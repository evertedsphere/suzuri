use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "review_grade")]
pub enum ReviewGrade {
    Fail,
    Hard,
    Okay,
    Easy,
}

impl ReviewGrade {
    fn as_factor(&self) -> f64 {
        match self {
            Self::Fail => -2.0,
            Self::Hard => -1.0,
            Self::Okay => 0.0,
            Self::Easy => 1.0,
        }
    }
}

pub struct Weights {
    /// Initial stability for a card that starts in the [`Grade::Fail`] state.
    /// w[0] in FSRS.
    init_stab_fail: f64,
    /// Initial stability for a card that starts in the [`Grade::Hard`] state.
    /// w[1] in FSRS.
    init_stab_hard: f64,
    /// Initial stability for a card that starts in the [`Grade::Okay`] state.
    /// w[2] in FSRS.
    init_stab_okay: f64,
    /// Initial stability for a card that starts in the [`Grade::Easy`] state.
    /// w[3] in FSRS.
    init_stab_easy: f64,
    /// w[4] in FSRS.
    diff_base: f64,
    /// w[5] in FSRS.
    init_diff_scale: f64,
    /// w[6] in FSRS.
    diff_upd_scale: f64,
    /// Mean reversion strength used when updating the difficulty parameter.
    /// w[7] in FSRS.
    diff_upd_mean_rev: f64,
    /// w[8] in FSRS.
    stab_upd_pass_scale: f64,
    /// w[9] in FSRS.
    stab_upd_pass_stab: f64,
    /// w[10] in FSRS.
    stab_upd_pass_retr: f64,
    /// w[15] in FSRS.
    stab_upd_pass_mult_hard: f64,
    /// w[16] in FSRS.
    stab_upd_pass_mult_easy: f64,
    /// w[11] in FSRS.
    stab_upd_fail_scale: f64,
    /// w[12] in FSRS.
    stab_upd_fail_diff: f64,
    /// w[13] in FSRS.
    stab_upd_fail_stab: f64,
    /// w[14] in FSRS.
    stab_upd_fail_retr: f64,
}

impl Weights {
    pub fn from_weight_vector(w: [f64; 17]) -> Self {
        Self {
            init_stab_fail: w[0],
            init_stab_hard: w[1],
            init_stab_okay: w[2],
            init_stab_easy: w[3],
            diff_base: w[4],
            init_diff_scale: w[5],
            diff_upd_scale: w[6],
            diff_upd_mean_rev: w[7],
            // there's no reason to not inline this
            stab_upd_pass_scale: f64::exp(w[8]),
            stab_upd_pass_stab: w[9],
            stab_upd_pass_retr: w[10],
            stab_upd_pass_mult_hard: w[15],
            stab_upd_pass_mult_easy: w[16],
            stab_upd_fail_scale: w[11],
            stab_upd_fail_diff: w[12],
            stab_upd_fail_stab: w[13],
            stab_upd_fail_retr: w[14],
        }
    }
}

pub struct Params {
    /// Whether or not theoretical intervals should be rounded to days.
    round_to_days: bool,
    /// TODO poor naming
    first_interval: Duration,
    /// TODO poor naming
    second_interval: Duration,
    /// TODO poor naming
    third_interval: Duration,
    /// TODO poor naming
    interval_step: Duration,
    /// A lower bound on the initial stability.
    min_initial_stability: f64,
    /// Used to clamp the difficulty.
    min_difficulty: f64,
    /// Used to clamp the difficulty.
    max_difficulty: f64,
    /// Used to bound the due date of a card to hopefully within the user's lifetime.
    max_interval: i64,
    /// The FSRS formulation we use is tuned for 90% by default.
    target_retention: f64,
    /// Used to compute parameter updates.
    weights: Weights,
}

impl Params {
    pub fn from_weight_vector(w: [f64; 17]) -> Self {
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

    fn initial_stability(&self, grade: ReviewGrade) -> f64 {
        match grade {
            ReviewGrade::Fail => self.weights.init_stab_fail,
            ReviewGrade::Hard => self.weights.init_stab_hard,
            ReviewGrade::Okay => self.weights.init_stab_okay,
            ReviewGrade::Easy => self.weights.init_stab_easy,
        }
    }

    fn stability_pass_update_bonus(&self, grade: ReviewGrade) -> f64 {
        match grade {
            ReviewGrade::Hard => self.weights.stab_upd_pass_mult_hard,
            ReviewGrade::Easy => self.weights.stab_upd_pass_mult_easy,
            _ => 1.0,
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Review {
    id: Uuid,
    grade: ReviewGrade,
    status: MemoryStatus,
    due_at: DateTime<Utc>,
    reviewed_at: DateTime<Utc>,
    difficulty: f64,
    stability: f64,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "memory_status")]
pub enum MemoryStatus {
    Learning,
    Reviewing,
    Relearning,
}

/// (State for) a unit of memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mneme {
    id: Uuid,
    created_at: DateTime<Utc>,
    next_due: DateTime<Utc>,
    status: Review,
    history: Vec<Review>,
}

impl Mneme {
    fn stability_pass_update_base(&self, params: &Params, retrievability: f64) -> f64 {
        let w = &params.weights;
        w.stab_upd_pass_scale
            * self.status.stability.powf(-w.stab_upd_pass_stab)
            * (w.stab_upd_pass_retr * (1.0 - retrievability)).exp_m1()
            * (1.0 + params.max_difficulty - self.status.difficulty)
    }

    fn stability_fail_update(&self, params: &Params, retrievability: f64) -> f64 {
        let w = &params.weights;
        w.stab_upd_fail_scale
            * self.status.difficulty.powf(-w.stab_upd_fail_diff)
            * (w.stab_upd_fail_retr * (1.0 - retrievability)).exp()
            * ((1.0 + self.status.stability).powf(w.stab_upd_fail_stab) - 1.0)
    }

    fn stability_for_grade(&self, params: &Params, grade: ReviewGrade, retrievability: f64) -> f64 {
        match grade {
            ReviewGrade::Fail => self.stability_fail_update(params, retrievability),
            _ => {
                let change_factor = 1.0
                    + self.stability_pass_update_base(params, retrievability)
                        * params.stability_pass_update_bonus(grade);
                self.status.stability * change_factor
            }
        }
    }

    fn interval_for_grade(
        &self,
        params: &Params,
        grade: ReviewGrade,
        retrievability: f64,
    ) -> Duration {
        Self::theoretical_interval(
            params,
            self.stability_for_grade(params, grade, retrievability),
        )
    }

    /// Point in time at which retrievability equals the target retention value.
    fn theoretical_interval(p: &Params, stab: f64) -> Duration {
        let d = 9.0 * stab * (-1.0 + 1.0 / p.target_retention);
        if p.round_to_days {
            Duration::days((d.round() as i64).clamp(1, p.max_interval as i64))
        } else {
            let d = 86400.0 * d.clamp(1.0, p.max_interval as f64);
            Duration::seconds(d.round() as i64)
        }
    }

    // We cleanly separate the creation of the initial card from reviews.
    // As a consequence of this, we do not have a `Status::New` state. A card
    // spawns with an initial review, which puts it into one of the other
    // states.
    pub fn init(params: &Params, grade: ReviewGrade) -> Self {
        Self::init_at(params, grade, Utc::now())
    }

    pub fn init_at(params: &Params, grade: ReviewGrade, now: DateTime<Utc>) -> Self {
        Self::init_at_with_id(params, grade, now, Uuid::new_v4(), Uuid::new_v4())
    }

    pub fn init_at_with_id(
        params: &Params,
        grade: ReviewGrade,
        now: DateTime<Utc>,
        id: Uuid,
        new_review_id: Uuid,
    ) -> Self {
        let difficulty = grade
            .as_factor()
            .mul_add(-params.weights.init_diff_scale, params.weights.diff_base)
            .clamp(params.min_difficulty, params.max_difficulty);
        let stability = params
            .initial_stability(grade)
            .max(params.min_initial_stability);
        let theoretical_interval = Self::theoretical_interval(params, stability);
        let interval = match grade {
            ReviewGrade::Fail => params.first_interval,
            ReviewGrade::Hard => params.second_interval,
            ReviewGrade::Okay => params.third_interval,
            ReviewGrade::Easy => theoretical_interval,
        };
        let next_due = now + interval;
        let state = match grade {
            ReviewGrade::Easy => MemoryStatus::Reviewing,
            _ => MemoryStatus::Learning,
        };
        let review = Review {
            id: new_review_id,
            grade,
            status: state,
            due_at: now,
            reviewed_at: now,
            difficulty,
            stability,
        };
        Self {
            id,
            created_at: now,
            next_due,
            status: review,
            history: Vec::new(),
        }
    }

    pub fn review(&self, params: &Params, grade: ReviewGrade) -> Self {
        self.review_at(params, grade, Utc::now())
    }

    pub fn review_at(&self, params: &Params, grade: ReviewGrade, now: DateTime<Utc>) -> Self {
        self.review_at_with_id(params, grade, now, Uuid::new_v4())
    }

    pub fn review_at_with_id(
        &self,
        params: &Params,
        grade: ReviewGrade,
        now: DateTime<Utc>,
        new_review_id: Uuid,
    ) -> Self {
        let w = &params.weights;
        let days_since = (now - self.status.reviewed_at).num_days() as f64;
        // It's an append-only log!
        let mut history = self.history.clone();
        history.push(self.status.clone());

        // Perform a transition on the state in case something unexpected happened.
        let state = match (self.status.status, grade) {
            (
                MemoryStatus::Learning | MemoryStatus::Relearning,
                ReviewGrade::Okay | ReviewGrade::Easy,
            ) => MemoryStatus::Reviewing,
            (MemoryStatus::Reviewing, ReviewGrade::Fail) => MemoryStatus::Relearning,
            (s, _) => s,
        };

        let retrievability = (1.0 + days_since / (9.0 * self.status.stability)).powi(-1);
        // These parameters will only be updated if we are on a review streak.
        let mut stability = self.status.stability;
        let mut difficulty = self.status.difficulty;

        let interval_for_grade =
            |grade| Self::interval_for_grade(self, params, grade, retrievability);
        // Note that we use the "current" state to choose our behaviour here.
        // TODO: deduplicate the nodes below where we recompute [`stability_update_base`] a few times?
        let interval = match self.status.status {
            MemoryStatus::Learning | MemoryStatus::Relearning => {
                let okay_interval = interval_for_grade(ReviewGrade::Okay);
                let min_easy_interval = params.interval_step + okay_interval;
                let easy_interval = min_easy_interval.max(interval_for_grade(ReviewGrade::Easy));
                match grade {
                    ReviewGrade::Fail => params.second_interval,
                    ReviewGrade::Hard => params.third_interval,
                    ReviewGrade::Okay => okay_interval,
                    ReviewGrade::Easy => easy_interval,
                }
            }
            MemoryStatus::Reviewing => {
                stability = Self::stability_for_grade(self, params, grade, retrievability);
                difficulty = w
                    .diff_upd_mean_rev
                    .mul_add(
                        w.diff_base,
                        (1.0 - w.diff_upd_mean_rev)
                            * grade
                                .as_factor()
                                .mul_add(-w.diff_upd_scale, self.status.difficulty),
                    )
                    .clamp(params.min_difficulty, params.max_difficulty);
                let theo_hard_interval = interval_for_grade(ReviewGrade::Hard);
                let theo_okay_interval = interval_for_grade(ReviewGrade::Okay);
                let theo_easy_interval = interval_for_grade(ReviewGrade::Easy);
                let hard_interval = theo_hard_interval.min(theo_okay_interval);
                let okay_interval = theo_okay_interval.max(params.interval_step + hard_interval);
                let easy_interval = theo_easy_interval.max(params.interval_step + okay_interval);
                match grade {
                    ReviewGrade::Fail => params.second_interval,
                    ReviewGrade::Hard => hard_interval,
                    ReviewGrade::Okay => okay_interval,
                    ReviewGrade::Easy => easy_interval,
                }
            }
        };

        let review = Review {
            id: new_review_id,
            stability,
            difficulty,
            status: state,
            due_at: self.next_due,
            reviewed_at: now,
            grade,
        };

        Self {
            id: self.id,
            status: review,
            history,
            next_due: now + interval,
            created_at: self.created_at.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use szr_golden::assert_golden_json;

    use super::*;

    static TEST_GRADES: [ReviewGrade; 12] = [
        ReviewGrade::Okay,
        ReviewGrade::Okay,
        ReviewGrade::Okay,
        ReviewGrade::Okay,
        ReviewGrade::Okay,
        ReviewGrade::Fail,
        ReviewGrade::Fail,
        ReviewGrade::Okay,
        ReviewGrade::Okay,
        ReviewGrade::Okay,
        ReviewGrade::Okay,
        ReviewGrade::Okay,
    ];

    static TEST_WEIGHTS: [f64; 17] = [
        1.14, 1.01, 5.44, 14.67, 5.3024, 1.5662, 1.2503, 0.0028, 1.5489, 0.1763, 0.9953, 2.7473,
        0.0179, 0.3105, 0.3976, 0.0, 2.0902,
    ];

    fn interval_history(item: &Mneme) -> Vec<i64> {
        let mut h: Vec<_> = item
            .history
            .windows(2)
            .map(|w| (w[1].due_at - w[0].reviewed_at).num_days())
            .collect();
        if let Some(prev) = item.history.last() {
            h.push((item.status.due_at - prev.reviewed_at).num_days());
            h.push((item.next_due - item.status.due_at).num_days());
        }
        h
    }

    #[cfg(test)]
    fn sample_mneme(p: &Params, grades: &[ReviewGrade], delay: Duration) -> Mneme {
        let mut now = DateTime::UNIX_EPOCH;
        let mut item = Mneme::init_at_with_id(
            p,
            ReviewGrade::Okay,
            now,
            Uuid::nil(),
            Uuid::from_u64_pair(0xf00f_f00f, 0),
        );
        for (n, &grade) in grades.into_iter().enumerate() {
            now = item.next_due + delay;
            let id = Uuid::from_u64_pair(0xffff_ffff, (n + 1) as u64);
            item = item.review_at_with_id(&p, grade, now, id);
        }
        item
    }

    // Taken from rs-fsrs.
    #[test]
    fn test_interval_history_on_time_rounded() {
        let mut p = Params::from_weight_vector(TEST_WEIGHTS);
        p.round_to_days = true;
        let item = sample_mneme(&p, &TEST_GRADES[..], Duration::zero());
        let history = interval_history(&item);
        let expected_history = [0, 5, 16, 43, 106, 236, 0, 0, 12, 25, 47, 85, 147];
        assert_eq!(history, expected_history);
        assert_golden_json!((history, item));
    }

    #[test]
    fn test_interval_history_delayed_rounded() {
        let p = Params::from_weight_vector(TEST_WEIGHTS);
        let item = sample_mneme(&p, &TEST_GRADES[..], Duration::days(1));
        let history = interval_history(&item);
        assert_golden_json!((history, item));
    }

    #[test]
    fn test_interval_history_on_time_true() {
        let p = Params::from_weight_vector(TEST_WEIGHTS);
        let item = sample_mneme(&p, &TEST_GRADES[..], Duration::zero());
        let history = interval_history(&item);
        assert_golden_json!((history, item));
    }

    #[test]
    fn test_interval_history_delayed_true() {
        let p = Params::from_weight_vector(TEST_WEIGHTS);
        let item = sample_mneme(&p, &TEST_GRADES[..], Duration::days(1));
        let history = interval_history(&item);
        assert_golden_json!((history, item));
    }
}
