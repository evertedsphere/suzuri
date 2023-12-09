use std::path::Path;

use serde::Serialize;
use snafu::{ResultExt, Snafu};
use sqlx::{postgres::PgArguments, query::Query, types::Json, PgPool, Postgres};
use szr_bulk_insert::PgBulkInsert;
use szr_dict::Def;
use szr_features::UnidicSession;
use szr_ja_utils::kata_to_hira_str;
use tracing::{instrument, trace};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    #[snafu(display("Surface form {id} is not in the database: {source}"))]
    LemmaNotFound {
        id: SurfaceFormId,
        source: sqlx::Error,
    },
    #[snafu(display("Lemma {spelling} ({reading}) is not in the database: {source}"))]
    NoMatchingLemma {
        spelling: String,
        reading: String,
        source: sqlx::Error,
    },
    #[snafu(display("Lemma {spelling} ({reading}) already exists: {source}"))]
    LemmaAlreadyExists {
        spelling: String,
        reading: String,
        source: sqlx::Error,
    },
    #[snafu(display("Failed to bulk insert lemmas: {source}"))]
    BulkInsertFailed {
        source: szr_bulk_insert::Error,
    },
    /// FIXME remove this
    SqlxFailure {
        source: sqlx::Error,
    },
    TokeniseFailure {
        source: szr_features::Error,
    },
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type, PartialOrd, Ord, Serialize)]
pub struct LemmaId(pub i64);

impl ::std::fmt::Display for LemmaId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type, PartialOrd, Ord, Serialize)]
pub struct SurfaceFormId(pub i64);

impl ::std::fmt::Display for SurfaceFormId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Lemma {
    pub id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewLemma {
    pub id: Option<LemmaId>,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(Debug)]
pub struct SurfaceForm {
    pub id: SurfaceFormId,
    pub lemma_id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewSurfaceForm {
    pub id: Option<SurfaceFormId>,
    pub lemma_id: Option<LemmaId>,
    pub spelling: String,
    pub reading: Option<String>,
}

impl PgBulkInsert for SurfaceForm {
    type InsertFields = NewSurfaceForm;
    type SerializeAs = (
        Option<SurfaceFormId>,
        Option<LemmaId>,
        String,
        Option<String>,
    );

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!(
            "COPY surface_forms (id, lemma_id, spelling, reading) FROM STDIN WITH (FORMAT CSV)"
        )
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.id, ins.lemma_id, ins.spelling, ins.reading))
    }
}

impl PgBulkInsert for Lemma {
    type InsertFields = NewLemma;
    type SerializeAs = (Option<LemmaId>, String, Option<String>);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("COPY lemmas (id, spelling, reading) FROM STDIN WITH (FORMAT CSV)")
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.id, ins.spelling, ins.reading))
    }
}

#[instrument(skip(pool, path), err)]
pub async fn import_unidic(pool: &PgPool, path: impl AsRef<Path>) -> Result<()> {
    let already_exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM surface_forms) as "already_exists!: bool" "#
    )
    .fetch_one(pool)
    .await
    .context(SqlxFailure)?;

    if already_exists {
        trace!("unidic already imported, skipping");
        return Ok(());
    }

    // Defining all the ancillary queries in one place makes refactors easier

    let pre_queries = [
        sqlx::query!("DROP INDEX IF EXISTS surface_forms_spelling_reading"),
        sqlx::query!("DROP INDEX IF EXISTS lemmas_spelling_reading"),
    ];

    let post_queries = [
        sqlx::query!("CREATE INDEX lemmas_spelling_reading ON surface_forms (spelling, reading) INCLUDE (id)"),
        sqlx::query!("CREATE INDEX surface_forms_spelling_reading ON surface_forms (spelling, reading) INCLUDE (id)"),
        sqlx::query!("ANALYZE lemmas"),
        sqlx::query!("ANALYZE surface_forms"),
    ];

    let mut surface_forms = Vec::new();
    let mut lemmas = Vec::new();

    UnidicSession::with_terms(path, |term| {
        let (lemma_spelling, lemma_reading, spelling, reading) = term.surface_form();
        let lemma_id = LemmaId(term.lemma_id.0);
        let surface_form_id = SurfaceFormId(term.lemma_guid.0);
        surface_forms.push(NewSurfaceForm {
            id: Some(surface_form_id),
            lemma_id: Some(lemma_id),
            spelling: spelling.clone(),
            reading: reading.as_deref().map(kata_to_hira_str),
        });
        lemmas.push(NewLemma {
            id: Some(lemma_id),
            spelling: lemma_spelling,
            reading: lemma_reading.as_deref().map(kata_to_hira_str),
        });
        Ok(())
    })
    .context(TokeniseFailure)?;

    // We now deduplicate the vector of surface forms, only keeping one
    // representative for each "lemma GUID" value.
    //
    //
    // To see what these look like:
    //
    // ```bash
    //   cat data/system/unidic-cwj-3.1.0/lex_3_1.csv | \
    //     mlr --c2t -N cut -f 1,32 | \
    //     uniq -Df1
    // ````
    //
    // (This will yield interesting entries such as "Bosh" [sic] a few entries
    // before "Lomb", "dechirer" [sic], "RADEON" and "GeForce", "mol/l",
    // "NIMBY", "BOYS" but no "GIRLS", "Zivilgesellschaft", "Ｅｍａｃｓ", etc.)
    //
    // Trivia aside, all of these "duplicates" are identical modulo 全角・半角
    // differences. (I haven't checked this by actually normalising them or
    // anything, but I've stared at the list for ten minutes, which is enough.)
    //
    // Even if the parser actually returns the correct one of the two, there is
    // almost no point to keeping the right one. You could argue that this means
    // we can't completely do away with storing the source text, because
    // replacing it with a list of surface form IDs will now no longer be
    // invertible ... if you aren't satisfied with quotienting out by character
    // width. I certainly am.

    surface_forms.dedup_by_key(|s| s.id);

    // This one needs less explanation: we get our lemmas from the list of
    // surface forms, and each lemma corresponds to potentially /tons/ of those.

    lemmas.sort_by_key(|s| s.id);
    lemmas.dedup_by_key(|s| s.id);

    // Start the actual bulk insert.

    let mut tx = pool.begin().await.context(SqlxFailure)?;

    // Pre-copy phase
    for q in pre_queries.into_iter() {
        q.execute(&mut *tx).await.context(SqlxFailure)?;
    }

    // Copy phase
    // The foreign key target has to go in first, of course.
    Lemma::copy_records(&mut *tx, lemmas)
        .await
        .context(BulkInsertFailed)?;
    SurfaceForm::copy_records(&mut *tx, surface_forms)
        .await
        .context(BulkInsertFailed)?;

    // Post-copy fixup phase
    for q in post_queries.into_iter() {
        q.execute(&mut *tx).await.context(SqlxFailure)?;
    }

    tx.commit().await.context(SqlxFailure)?;

    Ok(())
}

#[instrument(skip(pool), ret, err)]
pub async fn get_word_by_id<C>(pool: &PgPool, id: SurfaceFormId) -> Result<SurfaceForm> {
    sqlx::query_as!(
        SurfaceForm,
        r#"SELECT id as "id: SurfaceFormId",
                  lemma_id as "lemma_id!: LemmaId",
                  spelling, reading
           FROM surface_forms WHERE id = $1
         "#,
        id.0
    )
    .fetch_one(pool)
    .await
    .context(LemmaNotFound { id })
}

#[instrument(skip(pool), err)]
pub async fn get_word_meanings(pool: &PgPool, id: SurfaceFormId) -> Result<Vec<Def>> {
    let ret = sqlx::query_as!(
        Def,
        r#"SELECT
             defs.id, defs.dict_name, defs.spelling, defs.reading,
             defs.content as "content: Json<Vec<String>>"
           FROM lemmas
           INNER JOIN surface_forms
             ON surface_forms.lemma_id = lemmas.id
           INNER JOIN defs
             ON lemmas.spelling = defs.spelling AND lemmas.reading = defs.reading
           WHERE surface_forms.id = $1
          "#,
        // FIXME
        id.0
    )
    .fetch_all(pool)
    .await
    .context(SqlxFailure)?;

    Ok(ret)
}
