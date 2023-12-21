use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use sqlx::{postgres::PgArguments, query, query::Query, types::Json, PgPool, Postgres};
use szr_bulk_insert::PgBulkInsert;
use szr_tokenise::AnnTokens;

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
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewDoc {
    pub title: String,
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
    pub id: i32,
    pub doc_id: i32,
    pub index: i32,
    pub content: Element,
}

#[derive(Clone, Serialize)]
pub struct NewLine {
    pub doc_id: i32,
    pub index: i32,
    pub content: Element,
}

impl PgBulkInsert for Line {
    type InsertFields = NewLine;
    type SerializeAs = (i32, i32, String);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!("COPY lines (doc_id, index, content) FROM STDIN WITH (FORMAT CSV)")
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        let content_json = serde_json::to_string(&ins.content)
            .map_err(|source| szr_bulk_insert::Error::SerialisationError { source })?;
        Ok((ins.doc_id, ins.index, content_json))
    }
}

pub struct NewDocData {
    pub title: String,
    pub content: Vec<Element>,
}

pub async fn persist_doc(pool: &PgPool, data: NewDocData) -> Result<()> {
    let mut tx = pool.begin().await.context(SqlxFailure)?;

    let doc_id: i32 = sqlx::query_scalar!(
        "INSERT INTO docs (title) VALUES ($1) RETURNING id",
        data.title
    )
    .fetch_one(&mut *tx)
    .await
    .context(InsertDoc)?;

    let lines = data
        .content
        .into_iter()
        .enumerate()
        .map(|(index, content)| NewLine {
            doc_id,
            index: index as i32,
            content,
        })
        .collect::<Vec<_>>();

    Line::copy_records(&mut *tx, lines)
        .await
        .context(BulkInsertFailed)?;

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
        r#"SELECT id, doc_id, index, content as "content!: Json<Element>" FROM lines WHERE doc_id = $1"#,
        id
    )
    .fetch_all(pool)
    .await
    .context(SqlxFailure)?;
    Ok(Doc { id, title, lines })
}
