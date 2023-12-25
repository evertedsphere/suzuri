use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use snafu::Snafu;
use sqlx::PgPool;
use szr_dict::Def;
use szr_html::{Doc, Render, Z};
use szr_textual::Line;
use szr_tokenise::{AnnToken, AnnTokens};
use uuid::Uuid;

use crate::models::{get_word_meanings, SurfaceFormId};

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

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal error: {}", self),
        )
            .into_response()
    }
}

async fn parse_book<'a>(pool: &PgPool, doc_id: i32) -> Result<AnnTokens> {
    let doc = szr_textual::get_doc(pool, doc_id).await.unwrap();
    let mut v = Vec::new();
    doc.lines.into_iter().for_each(
        |Line {
             doc_id: _,
             index: line_index,
         }| {
            let mut token_index = 0;
            while let Some(token) = doc.tokens.get(&(line_index, token_index)) {
                v.push(AnnToken {
                    token: token.content.clone(),
                    surface_form_id: token.surface_form_id,
                });
                token_index += 1;
            }

            v.push(AnnToken {
                token: "\n".to_owned(),
                surface_form_id: None,
            });
        },
    );
    let tokens = AnnTokens(v);
    Ok(tokens)
}

fn labelled_value_c<'a, V: Render>(label: &'a str, value: V, classes: &'static str) -> Doc {
    Z.div()
        .class("flex flex-row gap-4")
        .c(Z.span()
            .class("italic text-gray-600 shrink-0 whitespace-nowrap")
            .c(label))
        .c(Z.span().class(classes).c(value))
}

fn labelled_value<V: Render>(label: &str, value: V) -> Doc {
    labelled_value_c(label, value, "")
}

pub async fn handle_lemmas_view(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    // lmao
) -> Result<Doc> {
    let section = |title| {
        Z.div()
            .class("flex flex-col px-6 py-4")
            .c(Z.h2().class("text-2xl font-bold pb-3").c(title))
    };

    let meanings = get_word_meanings(&pool, SurfaceFormId(id)).await.unwrap();

    let mut header = Z.h1().class("text-4xl px-6 py-3");

    if let Some(Def {
        spelling, reading, ..
    }) = meanings.first()
    {
        header = header.c(Z.ruby().c(&**spelling).c(Z.rt().c(&**reading)));
    }

    // let any_links = false;

    let any_defs = !meanings.is_empty();

    let defs_section = Z.div().class("flex flex-col gap-2").cs(
        meanings,
        |Def {
             dict_name, content, ..
         }| {
            // intersperse with commas
            // bit ugly but it's fine
            let mut it = content.0.into_iter().peekable();

            labelled_value(
                &dict_name,
                Z.div().cv({
                    let mut v = Vec::new();
                    while let Some(def) = it.next() {
                        v.push(Z.span().c(def));
                        if it.peek().is_some() {
                            v.push(Z.span().c(", "));
                        }
                    }
                    v
                }),
            )
        },
    );

    let mut html = Z
        .div()
        .id("defs")
        .class("flex flex-col gap-2")
        // .c(word_header)
        // .c(section("Memory").c(memory_section))
        // .c(
        //     section("Stats").c(Z.div().class("flex flex-col").c(labelled_value_c(
        //         "frequency",
        //         freq_label,
        //         "font-bold",
        //     ))),
        // )
        ;

    // if any_links {
    //     html = html.c(section("Links").c(related_words));
    // }
    if any_defs {
        html = html.c(header);
        html = html.c(section("Definitions").c(defs_section));
    }

    Ok(html)
}

pub async fn handle_books_view(State(pool): State<PgPool>, Path(id): Path<i32>) -> Result<Doc> {
    let book = parse_book(&pool, id).await.unwrap();

    let unpoly_preamble = (
        Z.script()
            .src("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.js"),
        Z.stylesheet("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.css"),
    );
    let fonts_preamble = (
        Z.link()
            .rel("preconnect")
            .href("https://fonts.googleapis.com"),
        Z.link()
            .rel("preconnect")
            .href("https://fonts.gstatic.com")
            .crossorigin(),
        Z.stylesheet("https://fonts.googleapis.com/css2?family=Sawarabi+Gothic&display=swap"),
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

    for AnnToken {
        token,
        surface_form_id,
    } in book.0.into_iter()
    {
        if token == "\n" {
            words.push(Z.br());
        } else {
            let text = token;
            if let Some(id) = surface_form_id {
                words.push(
                    Z.a()
                        .href(format!("/words/view/{}", id))
                        .class(format!(
                            "decoration-2 decoration-solid underline underline-offset-4 decoration-transparent word-{}",
                            id
                        ))
                        .up_instant()
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

    let head = Z
        .head()
        .c(unpoly_preamble)
        .c(fonts_preamble)
        .c(tailwind_preamble);
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

    Ok(ret)
}
