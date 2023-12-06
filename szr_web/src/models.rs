#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type)]
pub struct LemmaId(pub i32);

#[doc = " Default wrapper"]
impl ::std::fmt::Display for LemmaId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Lemma {
    pub id: LemmaId,
    pub spelling: String,
    pub reading: String,
}

pub struct NewLemma<'a> {
    pub spelling: &'a str,
    pub reading: &'a str,
}
