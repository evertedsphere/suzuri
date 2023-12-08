use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct AnnToken<'a> {
    pub token: &'a str,
    pub surface_form_id: Option<i64>,
    pub spelling: String,
    pub reading: Option<String>,
    pub lemma_spelling: String,
    pub lemma_reading: Option<String>,
}

impl Display for AnnToken<'_> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        if self.token == &self.spelling {
            write!(
                f,
                "{}({})",
                self.token,
                self.reading.as_deref().unwrap_or("?")
            )
        } else {
            write!(
                f,
                "{}[{}({})]",
                self.token,
                self.spelling,
                self.reading.as_deref().unwrap_or("?")
            )
        }
    }
}

#[derive(Debug)]
pub struct AnnTokens<'a>(pub Vec<AnnToken<'a>>);

impl Display for AnnTokens<'_> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        self.0.iter().enumerate().try_for_each(|(i, tok)| {
            if i != 0 {
                write!(f, " {}", tok)
            } else {
                write!(f, "{}", tok)
            }
        })
    }
}

pub trait Tokeniser {
    type Error: std::error::Error;

    // Tokenise, possibly keeping some internal state.
    fn tokenise_mut<'a>(&mut self, input: &'a str) -> Result<AnnTokens<'a>, Self::Error>;
}
