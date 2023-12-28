use chrono::{DateTime, Duration, SubsecRound, Utc};
use sqlx::{types::Uuid, PgPool};

use crate::{
    memory_status::MemoryStatus, mneme_state::MnemeState, params::Params, review_grade::ReviewGrade,
};

/// [`Utc::now()`] but truncated to six digits of precision
/// to guarantee equality when roundtripping through Postgres.
fn pg_compatible_now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}

/// (State for) a unit of memory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, PartialOrd)]
pub struct Mneme {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub next_due: DateTime<Utc>,
    pub state: MnemeState,
}

struct MnemeUpdate {
    /// Updated due date.
    next_due: DateTime<Utc>,
    /// Fresh review.
    new_state: MnemeState,
}

impl Mneme {
    pub(crate) fn stability_pass_update_base(&self, params: &Params, retrievability: f64) -> f64 {
        let w = &params.weights;
        w.stab_upd_pass_scale
            * self.state.stability.powf(-w.stab_upd_pass_stab)
            * (w.stab_upd_pass_retr * (1.0 - retrievability)).exp_m1()
            * (1.0 + params.max_difficulty - self.state.difficulty)
    }

    pub(crate) fn stability_fail_update(&self, params: &Params, retrievability: f64) -> f64 {
        let w = &params.weights;
        w.stab_upd_fail_scale
            * self.state.difficulty.powf(-w.stab_upd_fail_diff)
            * (w.stab_upd_fail_retr * (1.0 - retrievability)).exp()
            * ((1.0 + self.state.stability).powf(w.stab_upd_fail_stab) - 1.0)
    }

    pub(crate) fn stability_for_grade(
        &self,
        params: &Params,
        grade: ReviewGrade,
        retrievability: f64,
    ) -> f64 {
        match grade {
            ReviewGrade::Fail => self.stability_fail_update(params, retrievability),
            _ => {
                let change_factor = 1.0
                    + self.stability_pass_update_base(params, retrievability)
                        * params.stability_pass_update_bonus(grade);
                self.state.stability * change_factor
            }
        }
    }

    pub(crate) fn interval_for_grade(
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
    pub(crate) fn theoretical_interval(p: &Params, stab: f64) -> Duration {
        let d = 9.0 * stab * (-1.0 + 1.0 / p.target_retention);
        if p.round_to_days {
            Duration::days((d.round() as i64).clamp(1, p.max_interval as i64))
        } else {
            let d = 86400.0 * d.clamp(1.0, p.max_interval as f64);
            Duration::seconds(d.round() as i64)
        }
    }

    /// We cleanly separate the creation of the initial card from reviews.
    /// As a consequence of this, we do not have a `Status::New` state. A card
    /// spawns with an initial review, which puts it into one of the other
    /// states.
    pub(crate) fn init(params: &Params, grade: ReviewGrade) -> Self {
        Self::init_at(params, grade, pg_compatible_now())
    }

    pub(crate) fn init_at(params: &Params, grade: ReviewGrade, now: DateTime<Utc>) -> Self {
        Self::init_at_with_id(params, grade, now, Uuid::new_v4(), Uuid::new_v4())
    }

    pub(crate) fn init_at_with_id(
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
        let review = MnemeState {
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
            state: review,
        }
    }

    fn review_with_id(
        &self,
        params: &Params,
        grade: ReviewGrade,
        now: DateTime<Utc>,
        new_review_id: Uuid,
    ) -> MnemeUpdate {
        let w = &params.weights;
        let days_since = (now - self.state.reviewed_at).num_days() as f64;

        // Perform a transition on the state in case something unexpected happened.
        let state = match (self.state.status, grade) {
            (
                MemoryStatus::Learning | MemoryStatus::Relearning,
                ReviewGrade::Okay | ReviewGrade::Easy,
            ) => MemoryStatus::Reviewing,
            (MemoryStatus::Reviewing, ReviewGrade::Fail) => MemoryStatus::Relearning,
            (s, _) => s,
        };

        let retrievability = (1.0 + days_since / (9.0 * self.state.stability)).powi(-1);
        // These parameters will only be updated if we are on a review streak.
        let mut stability = self.state.stability;
        let mut difficulty = self.state.difficulty;

        let interval_for_grade =
            |grade| Self::interval_for_grade(self, params, grade, retrievability);
        // Note that we use the "current" state to choose our behaviour here.
        // TODO: deduplicate the nodes below where we recompute [`stability_update_base`] a few times?
        let interval = match self.state.status {
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
                                .mul_add(-w.diff_upd_scale, self.state.difficulty),
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

        let review = MnemeState {
            id: new_review_id,
            stability,
            difficulty,
            status: state,
            due_at: self.next_due,
            reviewed_at: now,
            grade,
        };

        MnemeUpdate {
            new_state: review,
            next_due: now + interval,
        }
    }
}

impl Mneme {
    pub async fn create(pool: &PgPool, params: &Params, grade: ReviewGrade) -> Result<Uuid, ()> {
        Self::init(params, grade).persist(pool).await
    }

    pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Self, ()> {
        struct RawMneme {
            id: Uuid,
            created_at: DateTime<Utc>,
            next_due: DateTime<Utc>,
            state_id: Uuid,
        }
        let RawMneme {
            id,
            created_at,
            next_due,
            state_id,
        } = sqlx::query_as!(
            RawMneme,
            "SELECT id, created_at, next_due, state_id FROM mnemes WHERE id = $1",
            id
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let state = MnemeState::get_by_id(pool, state_id).await.unwrap();

        Ok(Mneme {
            id,
            created_at,
            next_due,
            state,
        })
    }

    pub(crate) async fn persist(self, pool: &PgPool) -> Result<Uuid, ()> {
        let Self {
            id,
            created_at,
            next_due,
            state,
        } = self;

        let state_id = state.id;
        state.persist(pool).await.unwrap();

        let new_id = sqlx::query_scalar!(
            "INSERT INTO mnemes (id, created_at, next_due, state_id) VALUES ($1, $2, $3, $4) RETURNING id",
            id,
            created_at,
            next_due,
            state_id
        )
        .fetch_one(pool)
        .await
        .unwrap();

        Ok(new_id)
    }

    pub async fn review_by_id(
        pool: &PgPool,
        id: Uuid,
        params: &Params,
        grade: ReviewGrade,
    ) -> Result<(), ()> {
        let mneme = Self::get_by_id(pool, id).await.unwrap();
        mneme.review(pool, params, grade).await
    }

    // Placed here so the parallels with the next function are clearer.
    pub async fn review(
        &self,
        pool: &PgPool,
        params: &Params,
        grade: ReviewGrade,
    ) -> Result<(), ()> {
        let MnemeUpdate {
            next_due,
            new_state,
        } = self.review_with_id(params, grade, pg_compatible_now(), Uuid::new_v4());
        let new_state_id = new_state.id;
        new_state.persist(pool).await.unwrap();
        sqlx::query!(
            r#"
UPDATE mnemes
SET state_id = $2, next_due = $3
WHERE id = $1
"#,
            self.id,
            new_state_id,
            next_due
        )
        .execute(pool)
        .await
        .unwrap();
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MnemeWithHistory {
    pub mneme: Mneme,
    pub history: Vec<MnemeState>,
}

impl MnemeWithHistory {
    #[cfg(test)]
    pub(crate) fn init_at_with_id(
        params: &Params,
        grade: ReviewGrade,
        now: DateTime<Utc>,
        id: Uuid,
        new_review_id: Uuid,
    ) -> Self {
        Self {
            mneme: Mneme::init_at_with_id(params, grade, now, id, new_review_id),
            history: Vec::new(),
        }
    }

    // fn add_review_now(&self, params: &Params, grade: ReviewGrade) -> Self {
    //     self.add_review(params, grade, Utc::now())
    // }

    // fn add_review(&self, params: &Params, grade: ReviewGrade, now: DateTime<Utc>) -> Self {
    //     self.add_review_with_id(params, grade, now, Uuid::new_v4())
    // }

    #[cfg(test)]
    pub(crate) fn add_review_with_id(
        &self,
        params: &Params,
        grade: ReviewGrade,
        now: DateTime<Utc>,
        new_review_id: Uuid,
    ) -> Self {
        // It's an append-only log!
        let mut history = self.history.clone();
        history.push(self.mneme.state.clone());
        let MnemeUpdate {
            next_due,
            new_state,
        } = self.mneme.review_with_id(params, grade, now, new_review_id);
        Self {
            mneme: Mneme {
                id: self.mneme.id,
                state: new_state,
                next_due,
                created_at: self.mneme.created_at.clone(),
            },
            history,
        }
    }

    pub async fn persist(self, pool: &PgPool) -> Result<(), ()> {
        let Self { mneme, history } = self;
        mneme.persist(pool).await.unwrap();
        for s in history {
            s.persist(pool).await.unwrap();
        }
        Ok(())
    }
}
