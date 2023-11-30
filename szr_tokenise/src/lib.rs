pub struct AnnToken<'a> {
    pub token: &'a str,
    pub spelling: String,
    pub reading: String,
}

pub trait Tokeniser {
    type Error: std::error::Error;

    // Tokenise, possibly keeping some internal state.
    fn tokenise_mut<'a>(&mut self, input: &'a str) -> Result<Vec<AnnToken<'a>>, Self::Error>;
}
