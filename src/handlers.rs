use crate::config::CONFIG;
use crate::epub;
use crate::furi;
use crate::furi::MatchKind;
use crate::furi::Ruby;
use crate::morph;
use crate::morph::features::ExtraPos;
use crate::morph::features::LemmaId;
use crate::morph::features::TertiaryPos;
use actix_web::get;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::web::Json;
use actix_web::HttpResponse;
use actix_web::ResponseError;
use actix_web::{web, App, HttpServer};
use anyhow::Context;
use anyhow::Result;
use furi::KanjiDic;
use furi::Span;
pub use hashbrown::HashMap;
pub use hashbrown::HashSet;
use morph::features::AnalysisResult;
use morph::features::UnidicSession;
use serde::Serialize;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::ConnectOptions;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::log::warn;

#[derive(Debug)]
pub struct WrapError {
    err: anyhow::Error,
}

impl Serialize for WrapError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        pub struct H {
            error: String,
        }
        let h = H {
            error: format!("{:?}", self),
        };
        h.serialize(serializer)
    }
}

impl std::fmt::Display for WrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ResponseError for WrapError {
    fn error_response(&self) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::json())
            .json(self)
    }
    fn status_code(&self) -> actix_web::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl From<anyhow::Error> for WrapError {
    fn from(err: anyhow::Error) -> WrapError {
        WrapError { err }
    }
}

#[get("/parse_file/{filename}")]
pub async fn handle_parse_book(
    path: web::Path<String>,
) -> Result<web::Json<Vec<(String, u64)>>, WrapError> {
    let p = path.into_inner();
    let kd = furi::read_kanjidic()?;
    let mut session = morph::features::UnidicSession::new()?;
    let r = parse_book(&kd, &mut session, &format!("input/{}", p))?;
    Ok(Json(r))
}

//////

fn parse_book(
    kd: &KanjiDic,
    session: &mut UnidicSession,
    epub_file: impl AsRef<Path>,
) -> Result<Vec<(String, u64)>> {
    let mut yomi_freq: HashMap<furi::Span, u64> = HashMap::new();
    let mut yomi_uniq_freq: HashMap<furi::Span, u64> = HashMap::new();
    let mut lemma_freq: HashMap<LemmaId, u64> = HashMap::new();
    let mut name_count = 0;

    let r = epub::parse(epub_file.as_ref())?;
    let mut buf: Vec<&str> = Vec::new();
    for ch in r.chapters.iter() {
        for line in ch.lines.iter() {
            match line {
                epub::Element::Line(content) => {
                    buf.push(content);
                }
                _ => {}
            }
        }
    }
    let mut s = String::new();
    s.extend(buf);
    let AnalysisResult { tokens, terms } = session.analyse_with_cache(&s)?;

    // let mut after = 0;
    for (text, term_id) in tokens.iter() {
        // if term_id.0 == 8235625660686848 {
        //     print!("\nstart");
        //     after = 1;
        // }
        // if after >= 1 && after < 20 {
        //     after += 1;
        //     print!("{}", text);
        // }
        *lemma_freq.entry(*term_id).or_default() += 1;
    }
    // println!("done");

    for (term_id, term) in terms.iter() {
        let (spelling, reading) = term.surface_form();
        if term.extra_pos == ExtraPos::Myou || term.extra_pos == ExtraPos::Sei {
            debug!("skipping name term {} ({:?})", term, term_id);
            name_count += 1;
            continue;
        }
        if let Some(reading) = reading {
            let furi =
                furi::annotate(spelling, reading, &kd).context("failed to parse unidic term");
            if let Ok(Ruby::Valid { spans }) = furi {
                for span in spans.into_iter() {
                    if let Span::Kanji { .. } = span {
                        *yomi_uniq_freq.entry(span.clone()).or_default() += 1;
                        *yomi_freq.entry(span).or_default() += lemma_freq[term_id];
                    }
                }
            }
        }
    }

    debug!("skipped {} name terms", name_count);

    debug!("yomi freq (coverage)");
    let mut freqs = yomi_freq.into_iter().collect::<Vec<_>>();
    freqs.sort_by(|x, y| x.1.cmp(&y.1).reverse());
    freqs.truncate(100);
    for (span, freq) in freqs.iter() {
        print!("{}: {}x, ", span, freq);
    }
    println!();

    debug!("yomi freq (unique)");
    let mut uniq_freqs = yomi_uniq_freq.into_iter().collect::<Vec<_>>();
    uniq_freqs.sort_by(|x, y| x.1.cmp(&y.1).reverse());
    uniq_freqs.truncate(100);
    for (span, freq) in uniq_freqs.iter() {
        print!("{}: {}x, ", span, freq);
    }
    println!();

    Ok(tokens.iter().map(|(x, y)| (x.to_string(), y.0)).collect())
}
