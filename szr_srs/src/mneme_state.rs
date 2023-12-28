use chrono::{DateTime, Utc};
use sqlx::{types::Uuid, PgPool};

use crate::{memory_status::MemoryStatus, review_grade::ReviewGrade};

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq, PartialOrd)]
pub struct MnemeState {
    pub id: Uuid,
    pub grade: ReviewGrade,
    pub status: MemoryStatus,
    pub due_at: DateTime<Utc>,
    pub reviewed_at: DateTime<Utc>,
    pub difficulty: f64,
    pub stability: f64,
    // TODO fk to params used to perform this review?
    // or do we decide to actually remove as much data as possible
    // and literally regenerate intermediate due dates and ... no, probably not
}

impl MnemeState {
    pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Self, ()> {
        let r = sqlx::query_as!(
            Self,
            r#"SELECT
id,
grade AS "grade: _",
status AS "status: _",
due_at,
reviewed_at,
 difficulty,
stability
FROM mneme_states WHERE id = $1"#,
            id
        )
        .fetch_one(pool)
        .await
        .unwrap();
        Ok(r)
    }

    pub(crate) async fn persist(self, pool: &PgPool) -> Result<(), ()> {
        let Self {
            id,
            grade,
            status,
            due_at,
            reviewed_at,
            difficulty,
            stability,
        } = self;
        sqlx::query!(
            r#"INSERT INTO mneme_states (id, grade, status, due_at, reviewed_at, difficulty, stability)
VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            id,
            // disable compile-time checking for the enum mappings
            // https://github.com/launchbadge/sqlx/issues/1004
            grade as _,
            status as _,
            due_at,
            reviewed_at,
            difficulty,
            stability
        )
        .execute(pool)
        .await
        .unwrap();
        Ok(())
    }
}
