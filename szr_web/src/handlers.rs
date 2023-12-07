use axum::{
    extract::{Path, State},
    response::Html,
};
use snafu::{ResultExt, Snafu};
use sqlx::PgPool;
use szr_features::UnidicSession;
use szr_html::{RenderExt, Z};
use szr_ja_utils::kata_to_hira;
use szr_tokenise::Tokeniser;
use tracing::debug;

use crate::{
    lemma::{get_lemma, get_lemma_meanings},
    models::LemmaId,
};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(whatever, display("{message}: {source:?}"))]
    CatchallError {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
        source: Option<Box<dyn std::error::Error>>,
    },
}

fn parse_book<'a>(
    session: &'a mut UnidicSession,
    epub_file: impl AsRef<std::path::Path>,
) -> Result<Vec<(String, String, String)>> {
    let r = szr_epub::parse(epub_file.as_ref()).whatever_context("parsing epub")?;
    let mut buf: Vec<&str> = Vec::new();
    let mut n = 0;
    for ch in r.chapters.iter() {
        for line in ch.lines.iter() {
            match line {
                szr_epub::Element::Line(content) => {
                    buf.push(content);
                    buf.push("\n");
                    n += 1;
                    if n == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let mut input = String::new();
    input.extend(buf);
    debug!("parsed epub");
    let mut tokens = session.tokenise_mut(&input).whatever_context("tokenise")?;
    debug!("analysed text");
    // SurfaceForm::insert_terms(pool, terms.clone().into_values()).await?;
    // debug!("inserted {} terms", terms.len());
    tokens.0.truncate(200);
    Ok(tokens
        .0
        .into_iter()
        .map(|x| {
            (
                x.token.to_owned(),
                x.spelling,
                x.reading.chars().map(kata_to_hira).collect(),
            )
        })
        .collect())
}

pub async fn handle_lemmas_view(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
    // lmao
) -> Result<Html<String>, String> {
    let r = get_lemma_meanings(&pool, LemmaId(id)).await.unwrap();

    debug!("{r:?}");

    Ok(Html("<div>lol</div>".to_owned()))
}

pub async fn handle_books_view(
    State(pool): State<PgPool>,
    Path(name): Path<String>,
) -> Result<Html<String>, String> {
    let mut session = UnidicSession::new().unwrap();

    let book = parse_book(&mut session, format!("input/{name}.epub")).unwrap();
    // debug!("{:?}", content);

    let unpoly_preamble = (
        Z.script()
            .src("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.js"),
        Z.stylesheet("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.css"),
    );
    let tailwind_preamble = Z.stylesheet("/static/output.css");

    let sidebar = Z
        .div()
        .id("sidebar")
        .class("w-4/12 grow-0 p-6 bg-gray-300 overflow-auto shadow-left-side")
        .c(Z.div()
            .id("defs")
            .c(Z.span().c("Click on a word to look it up")));

    let mut words = Vec::new();

    for (tok, s, r) in book.into_iter() {
        if s == "\n" {
            words.push(Z.br());
        } else {
            let text = tok;
            if let Ok(term) = get_lemma(&pool, &s, &r).await {
                words.push(
                    Z.a()
                        .href(format!("/lemmas/view/{}", term.id.0))
                        .class(format!(
                            "decoration-2 decoration-solid underline underline-offset-4 decoration-blue-600 word-{}",
                            term.id.0
                        ))
                        // .up_instant()
                        // .up_preload()
                        .up_target("#defs")
                        .up_cache("false")
                        .c(text),
                );
                continue;
            }
            // if let Some(term) = terms.get(&id) {
            //     if let (_spelling, Some(_reading)) = term.surface_form() {
            //         let card = SurfaceForm::get_by_id(&pool, id).await?.card;
            //         let state_classes = match card {
            //             None => "decoration-transparent",
            //             Some(card) => match card.state {
            //                 State::New => "decoration-blue-600",
            //                 State::Review => "decoration-green-600",
            //                 _ => "decoration-amber-600",
            //             },
            //         };
            //         words.push(
            //             Z.a()
            //                 .href(format!("/word_info/{}", id.0))
            //                 .class(format!(
            //                     "{state_classes} decoration-2 decoration-solid underline underline-offset-4 word-{}",
            //                     id.0
            //                 ))
            //                 // .up_instant()
            //                 // .up_preload()
            //                 .up_target("#defs")
            //                 .up_cache("false")
            //                 .c(text),
            //         );
            //         continue;
            //     }
            // }
            words.push(Z.span().c(text));
        }
    }

    let main = Z
        .div()
        .id("main")
        .class("w-6/12 grow-0 p-12 bg-gray-200 overflow-scroll text-2xl/10")
        .up_nav()
        .cv(words);

    let head = Z.head().c(unpoly_preamble).c(tailwind_preamble);
    let body = Z
        .body()
        .class("h-screen w-screen bg-gray-100 relative flex flex-row overflow-hidden")
        .c(Z.div().class("grow bg-gray-200").id("left-spacer"))
        .c(main)
        .c(sidebar)
        .c(Z.div().class("grow bg-gray-300").id("right-spacer"));
    let ret = Z
        .fragment()
        .c(Z.doctype("html"))
        .c(Z.meta().charset("UTF-8"))
        .c(Z.meta()
            .name("viewport")
            .content("width=device-width, initial-scale=1.0"))
        .c(Z.html().lang("ja").c(head).c(body));

    Ok(Html(ret.render_to_string()))
}
