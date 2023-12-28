pub struct Weights {
    /// Initial stability for a card that starts in the [`Grade::Fail`] state.
    /// w[0] in FSRS.
    pub(crate) init_stab_fail: f64,
    /// Initial stability for a card that starts in the [`Grade::Hard`] state.
    /// w[1] in FSRS.
    pub(crate) init_stab_hard: f64,
    /// Initial stability for a card that starts in the [`Grade::Okay`] state.
    /// w[2] in FSRS.
    pub(crate) init_stab_okay: f64,
    /// Initial stability for a card that starts in the [`Grade::Easy`] state.
    /// w[3] in FSRS.
    pub(crate) init_stab_easy: f64,
    /// w[4] in FSRS.
    pub(crate) diff_base: f64,
    /// w[5] in FSRS.
    pub(crate) init_diff_scale: f64,
    /// w[6] in FSRS.
    pub(crate) diff_upd_scale: f64,
    /// Mean reversion strength used when updating the difficulty parameter.
    /// w[7] in FSRS.
    pub(crate) diff_upd_mean_rev: f64,
    /// w[8] in FSRS.
    pub(crate) stab_upd_pass_scale: f64,
    /// w[9] in FSRS.
    pub(crate) stab_upd_pass_stab: f64,
    /// w[10] in FSRS.
    pub(crate) stab_upd_pass_retr: f64,
    /// w[15] in FSRS.
    pub(crate) stab_upd_pass_mult_hard: f64,
    /// w[16] in FSRS.
    pub(crate) stab_upd_pass_mult_easy: f64,
    /// w[11] in FSRS.
    pub(crate) stab_upd_fail_scale: f64,
    /// w[12] in FSRS.
    pub(crate) stab_upd_fail_diff: f64,
    /// w[13] in FSRS.
    pub(crate) stab_upd_fail_stab: f64,
    /// w[14] in FSRS.
    pub(crate) stab_upd_fail_retr: f64,
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
