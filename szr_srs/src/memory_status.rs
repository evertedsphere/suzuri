#[derive(
    Debug,
    Copy,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
)]
#[sqlx(type_name = "memory_status")]
pub enum MemoryStatus {
    Learning,
    Reviewing,
    Relearning,
}
