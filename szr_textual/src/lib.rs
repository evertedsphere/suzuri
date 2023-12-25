use std::{collections::HashMap, fs::File, io::Read};

use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use sqlx::{
    postgres::PgArguments,
    query,
    query::Query,
    types::{Json, Uuid},
    PgPool, Postgres,
};
use szr_bulk_insert::PgBulkInsert;
use szr_features::UnidicSession;
use szr_tokenise::{AnnTokens, Tokeniser};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    InsertDoc {
        source: sqlx::Error,
    },
    SqlxFailure {
        source: sqlx::Error,
    },
    #[snafu(display("Failed to bulk insert data: {source}"))]
    BulkInsertFailed {
        source: szr_bulk_insert::Error,
    },
}

#[derive(Debug)]
pub struct Doc {
    pub id: i32,
    pub title: String,
    pub lines: Vec<Line>,
    pub tokens: HashMap<(i32, i32), Token>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewDoc {
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub doc_id: i32,
    pub line_index: i32,
    pub index: i32,
    pub content: String,
    pub surface_form_id: Option<Uuid>,
}

// pub struct TempToken { .. }
// // temporary table
// impl PgBulkInsert for TempToken { .. }
// then insert into tokens from temptokens on conflict do nothing
// also make tokenise depend on this crate
// struct Token { .. } replaces AnnToken

impl PgBulkInsert for Token {
    type InsertFields = Token;
    type SerializeAs = (i32, i32, i32, String, Option<String>);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!(
            "COPY tokens (doc_id, line_index, index, content, surface_form_id) FROM STDIN WITH (FORMAT CSV)"
        )
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        let Token {
            doc_id,
            line_index,
            index,
            content,
            surface_form_id,
        } = ins;
        Ok((
            doc_id,
            line_index,
            index,
            content,
            surface_form_id.map(|x| x.to_string()),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Element {
    Image(String),
    #[serde(untagged)]
    Line(AnnTokens),
}

impl From<Json<Element>> for Element {
    fn from(value: Json<Self>) -> Self {
        value.0
    }
}

#[derive(Debug)]
pub struct Line {
    pub doc_id: i32,
    pub index: i32,
}

impl PgBulkInsert for Line {
    type InsertFields = Line;
    type SerializeAs = (i32, i32);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!("COPY lines (doc_id, index) FROM STDIN WITH (FORMAT CSV)")
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.doc_id, ins.index))
    }
}

pub struct NewDocData {
    pub title: String,
    pub content: Vec<Element>,
}

pub async fn persist_doc(pool: &PgPool, data: NewDocData) -> Result<()> {
    persist_docs(pool, vec![data]).await
}

pub async fn persist_docs(pool: &PgPool, data: Vec<NewDocData>) -> Result<()> {
    let mut tx = pool.begin().await.context(SqlxFailure)?;

    // not at all worth COPY

    let mut doc_ids = Vec::new();

    for doc in data.iter() {
        let doc_id = sqlx::query_scalar!(
            "INSERT INTO docs (title) VALUES ($1) RETURNING id",
            doc.title
        )
        .fetch_one(&mut *tx)
        .await
        .context(InsertDoc)?;
        doc_ids.push(doc_id);
    }

    let mut lines = Vec::new();
    let mut tokens = Vec::new();

    for (doc_id, doc) in doc_ids.into_iter().zip(data.into_iter()) {
        doc.content
            .into_iter()
            .enumerate()
            .for_each(|(line_index, content)| {
                let line_index = line_index as i32;
                lines.push(Line {
                    doc_id,
                    index: line_index,
                });

                match content {
                    Element::Image(_) => {
                        // FIXME
                    }
                    Element::Line(AnnTokens(v)) => {
                        v.into_iter().enumerate().for_each(|(token_index, token)| {
                            let index = token_index as i32;
                            tokens.push(Token {
                                doc_id,
                                line_index,
                                index,
                                content: token.token,
                                surface_form_id: token.surface_form_id,
                            })
                        })
                    }
                };
            });
    }

    sqlx::query_file!("../migrations/6_enrich_docs_lines.down.sql")
        .execute(&mut *tx)
        .await
        .context(SqlxFailure)?;

    Line::copy_records(&mut *tx, lines)
        .await
        .context(BulkInsertFailed)?;

    Token::copy_records(&mut *tx, tokens)
        .await
        .context(BulkInsertFailed)?;

    sqlx::query_file!("../migrations/6_enrich_docs_lines.up.sql")
        .execute(&mut *tx)
        .await
        .context(SqlxFailure)?;

    tx.commit().await.context(SqlxFailure)?;

    Ok(())
}

pub async fn get_doc(pool: &PgPool, id: i32) -> Result<Doc> {
    // This is a bit silly now, but it won't be when Doc spawns more fields
    struct DocMeta {
        id: i32,
        title: String,
    }
    let DocMeta { id, title } =
        sqlx::query_as!(DocMeta, "SELECT id, title FROM docs WHERE id = $1", id)
            .fetch_one(pool)
            .await
            .context(SqlxFailure)?;
    let lines = sqlx::query_as!(
        Line,
        r#"SELECT doc_id, index FROM lines WHERE doc_id = $1"#,
        id
    )
    .fetch_all(pool)
    .await
    .context(SqlxFailure)?;
    let tokens_vec: Vec<Token> = sqlx::query_as!(
        Token,
        r#"SELECT doc_id, line_index, index, content, surface_form_id FROM tokens WHERE doc_id = $1"#,
        id
    )
    .fetch_all(pool)
    .await
    .context(SqlxFailure)?;
    let tokens = tokens_vec
        .into_iter()
        .map(|token| ((token.line_index, token.index), token))
        .collect();
    Ok(Doc {
        id,
        title,
        lines,
        tokens,
    })
}

pub struct TextFile {
    pub title: String,
    pub content: String,
}

pub trait Textual {
    fn to_text(&mut self) -> TextFile;
}

pub fn to_doc<T: Textual>(mut t: T, session: &mut UnidicSession) -> NewDocData {
    let TextFile {
        title,
        content: raw_content,
    } = t.to_text();
    let tokens = session.tokenise_mut(&raw_content).unwrap();
    let content = tokens
        .0
        .split(|v| v.token == "\n")
        .map(|v| Element::Line(AnnTokens(v.to_vec())))
        .collect::<Vec<_>>();

    NewDocData { title, content }
}

impl Textual for File {
    fn to_text(&mut self) -> TextFile {
        let mut buf = String::new();
        self.read_to_string(&mut buf).unwrap();
        TextFile {
            title: "??".to_owned(),
            content: buf,
        }
    }
}
