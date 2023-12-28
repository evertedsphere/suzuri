use chrono::{DateTime, Duration};
use sqlx::{types::Uuid, PgPool};
use szr_golden::assert_golden_json;

use super::*;
use crate::params::Params;

// Pure tests

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

fn interval_history(item: &MnemeWithHistory) -> Vec<i64> {
    let mut h: Vec<_> = item
        .history
        .windows(2)
        .map(|w| (w[1].due_at - w[0].reviewed_at).num_days())
        .collect();
    if let Some(prev) = item.history.last() {
        h.push((item.mneme.state.due_at - prev.reviewed_at).num_days());
        h.push((item.mneme.next_due - item.mneme.state.due_at).num_days());
    }
    h
}

fn sample_mneme(p: &Params, grades: &[ReviewGrade], delay: Duration) -> MnemeWithHistory {
    let mut now = DateTime::UNIX_EPOCH;
    let mut item = MnemeWithHistory::init_at_with_id(
        p,
        ReviewGrade::Okay,
        now,
        Uuid::nil(),
        Uuid::from_u64_pair(0xf00f_f00f, 0),
    );
    for (n, &grade) in grades.into_iter().enumerate() {
        now = item.mneme.next_due + delay;
        let id = Uuid::from_u64_pair(0xffff_ffff, (n + 1) as u64);
        item = item.add_review_with_id(&p, grade, now, id);
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

// SQL tests

#[sqlx::test(migrations = "../migrations")]
async fn can_create_mneme(pool: PgPool) -> sqlx::Result<()> {
    let p = Params::from_weight_vector(TEST_WEIGHTS);
    Mneme::create(&pool, &p, ReviewGrade::Hard).await.unwrap();
    Ok(())
}

#[sqlx::test(migrations = "../migrations")]
async fn create_preserves_mneme(pool: PgPool) -> sqlx::Result<()> {
    let p = Params::from_weight_vector(TEST_WEIGHTS);
    let new_mneme = Mneme::init(&p, ReviewGrade::Easy);
    let id = new_mneme.id;
    let _ = new_mneme.clone().persist(&pool).await.unwrap();
    let db_mneme = Mneme::get_by_id(&pool, id).await.unwrap();
    assert_eq!(new_mneme, db_mneme);
    Ok(())
}

#[sqlx::test(migrations = "../migrations")]
async fn can_review_mneme(pool: PgPool) -> sqlx::Result<()> {
    let p = Params::from_weight_vector(TEST_WEIGHTS);
    let id = Mneme::create(&pool, &p, ReviewGrade::Easy).await.unwrap();
    Mneme::review_by_id(&pool, id, &p, ReviewGrade::Hard)
        .await
        .unwrap();
    Ok(())
}
