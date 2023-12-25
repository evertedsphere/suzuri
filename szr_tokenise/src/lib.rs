use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct AnnToken {
    pub token: String,
    pub surface_form_id: Option<Uuid>,
}

impl Display for AnnToken {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.token)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnnTokens(pub Vec<AnnToken>);

impl Display for AnnTokens {
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
    fn tokenise_mut<'a>(&mut self, input: &'a str) -> Result<AnnTokens, Self::Error>;
}
