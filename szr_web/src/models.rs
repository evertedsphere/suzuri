use std::{collections::HashMap, path::Path};

use serde::Serialize;
use snafu::{ResultExt, Snafu};
use sqlx::{postgres::PgArguments, query, query::Query, types::Json, PgPool, Postgres};
use szr_bulk_insert::PgBulkInsert;
use szr_dict::Def;
use szr_features::{FourthPos, MainPos, SecondPos, TermExtract, ThirdPos, UnidicSession};
use tracing::{debug, instrument, trace};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    #[snafu(display("Surface form {id} is not in the database: {source}"))]
    SurfaceFormNotFound {
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

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type, PartialOrd, Ord, Serialize)]
pub struct VariantId(pub i64);

impl ::std::fmt::Display for VariantId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// Lemmas

#[derive(Debug)]
pub struct Lemma {
    pub id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
    pub disambiguation: Option<String>,
    pub main_pos: MainPos,
    pub second_pos: SecondPos,
    pub third_pos: ThirdPos,
    pub fourth_pos: FourthPos,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewLemma {
    pub id: Option<LemmaId>,
    pub spelling: String,
    pub reading: Option<String>,
    pub disambiguation: Option<String>,
    pub main_pos: MainPos,
    pub second_pos: SecondPos,
    pub third_pos: ThirdPos,
    pub fourth_pos: FourthPos,
}

#[derive(Debug)]
pub struct Variant {
    pub id: VariantId,
    pub lemma_id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewVariant {
    pub id: VariantId,
    pub lemma_id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(Debug)]
pub struct SurfaceForm {
    pub id: SurfaceFormId,
    pub variant_id: VariantId,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewSurfaceForm {
    pub id: Option<SurfaceFormId>,
    pub variant_id: VariantId,
    pub spelling: String,
    pub reading: Option<String>,
}

impl PgBulkInsert for Lemma {
    type InsertFields = NewLemma;
    type SerializeAs = (
        Option<LemmaId>,
        String,
        Option<String>,
        Option<String>,
        MainPos,
        SecondPos,
        ThirdPos,
        FourthPos,
    );

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!(
            r#"
COPY lemmas (id, spelling, disambiguation, reading, main_pos, second_pos, third_pos, fourth_pos)
FROM STDIN WITH (FORMAT CSV)
"#
        )
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((
            ins.id,
            ins.spelling,
            ins.disambiguation,
            ins.reading,
            ins.main_pos,
            ins.second_pos,
            ins.third_pos,
            ins.fourth_pos,
        ))
    }
}

impl PgBulkInsert for Variant {
    type InsertFields = NewVariant;
    type SerializeAs = (VariantId, LemmaId, String, Option<String>);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!("COPY variants (id, lemma_id, spelling, reading) FROM STDIN WITH (FORMAT CSV)")
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.id, ins.lemma_id, ins.spelling, ins.reading))
    }
}

impl PgBulkInsert for SurfaceForm {
    type InsertFields = NewSurfaceForm;
    type SerializeAs = (Option<SurfaceFormId>, VariantId, String, Option<String>);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!(
            "COPY surface_forms (id, variant_id, spelling, reading) FROM STDIN WITH (FORMAT CSV)"
        )
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.id, ins.variant_id, ins.spelling, ins.reading))
    }
}

#[instrument(skip(pool, path), err)]
pub async fn import_unidic<T>(pool: &PgPool, path: T, user_dict_path: Option<T>) -> Result<()>
where
    T: AsRef<Path> + std::fmt::Debug,
{
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

    let pre_queries = sqlx::query_file!("../migrations/2_enrich_lvs.down.sql");

    let post_queries = sqlx::query_file!("../migrations/2_enrich_lvs.up.sql");

    let mut surface_forms = HashMap::new();
    let mut lemmas = HashMap::new();
    let mut variant_counter: i64 = 1;
    let mut variants = HashMap::new();

    UnidicSession::with_terms(path, user_dict_path, |term| {
        let TermExtract {
            lemma_spelling,
            lemma_reading,
            variant_spelling,
            variant_reading,
            surface_form_spelling,
            surface_form_reading,
        } = term.surface_form();

        let (main_spelling, disambiguation) = match lemma_spelling.split_once('-') {
            Some((l, r)) => (l.to_owned(), Some(r.to_owned())),
            None => (lemma_spelling, None),
        };

        let lemma_id = LemmaId(term.lemma_id.0);
        lemmas.entry(lemma_id).or_insert(NewLemma {
            id: Some(lemma_id),
            spelling: main_spelling,
            disambiguation,
            reading: lemma_reading,
            main_pos: term.main_pos,
            second_pos: term.second_pos,
            third_pos: term.third_pos,
            fourth_pos: term.fourth_pos,
        });

        // Variants don't exist within Unidic, so we have to handle the variant ID
        // ourselves.

        let current_variant_id = VariantId(variant_counter);
        let variant_id = variants
            .entry((lemma_id, variant_spelling.clone(), variant_reading.clone()))
            .or_insert({
                variant_counter += 1;
                NewVariant {
                    id: current_variant_id,
                    lemma_id,
                    spelling: variant_spelling,
                    reading: variant_reading,
                }
            })
            .id;

        // The map allows us to deduplicate the set of surface forms by ID.
        //
        // To see what the duplicates look like:
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

        let surface_form_id = SurfaceFormId(term.lemma_guid.0);
        surface_forms
            .entry(surface_form_id)
            .or_insert(NewSurfaceForm {
                id: Some(surface_form_id),
                variant_id,
                spelling: surface_form_spelling,
                reading: surface_form_reading,
            });

        Ok(())
    })
    .context(TokeniseFailure)?;

    let lemmas = lemmas.into_values().collect();
    let surface_forms = surface_forms.into_values().collect();
    let variants = variants.into_values().collect();

    // Start the actual bulk insert.

    let mut tx = pool.begin().await.context(SqlxFailure)?;

    // Pre-copy phase
    debug!("dropping indexes and constraints");
    pre_queries.execute(&mut *tx).await.context(SqlxFailure)?;

    // Copy phase
    // The foreign key target has to go in first, of course.
    Lemma::copy_records(&mut *tx, lemmas)
        .await
        .context(BulkInsertFailed)?;
    Variant::copy_records(&mut *tx, variants)
        .await
        .context(BulkInsertFailed)?;
    SurfaceForm::copy_records(&mut *tx, surface_forms)
        .await
        .context(BulkInsertFailed)?;

    // Post-copy fixup phase
    debug!("recreating indexes and constraints");
    post_queries.execute(&mut *tx).await.context(SqlxFailure)?;

    tx.commit().await.context(SqlxFailure)?;

    Ok(())
}

#[instrument(skip(pool), err)]
pub async fn get_word_meanings(pool: &PgPool, id: SurfaceFormId) -> Result<Vec<Def>> {
    let query = sqlx::query_as!(
        Def,
        r#"
SELECT
    defs.id, defs.dict_name, defs.spelling, defs.reading,
    defs.content as "content: Json<Vec<String>>"
FROM defs
JOIN variants ON variants.spelling = defs.spelling AND variants.reading = defs.reading
JOIN lemmas ON variants.lemma_id = lemmas.id
JOIN surface_forms ON surface_forms.variant_id = variants.id
WHERE surface_forms.id = $1;
          "#,
        // FIXME
        id.0
    );

    let ret = query.fetch_all(pool).await.context(SqlxFailure)?;

    Ok(ret)
}
