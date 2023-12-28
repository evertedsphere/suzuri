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
#[sqlx(type_name = "review_grade")]
pub enum ReviewGrade {
    Fail,
    Hard,
    Okay,
    Easy,
}

impl ReviewGrade {
    pub(crate) fn as_factor(&self) -> f64 {
        match self {
            Self::Fail => -2.0,
            Self::Hard => -1.0,
            Self::Okay => 0.0,
            Self::Easy => 1.0,
        }
    }
}
