use std::{collections::HashMap, path::Path};

use serde::Serialize;
use snafu::{ResultExt, Snafu};
use sqlx::{postgres::PgArguments, query, query::Query, types::Json, PgPool, Postgres};
use szr_bulk_insert::PgBulkInsert;
use szr_dict::Def;
use szr_features::{TermExtract, UnidicSession};
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
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct NewLemma {
    pub id: Option<LemmaId>,
    pub spelling: String,
    pub reading: Option<String>,
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
    type SerializeAs = (Option<LemmaId>, String, Option<String>);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!("COPY lemmas (id, spelling, reading) FROM STDIN WITH (FORMAT CSV)")
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.id, ins.spelling, ins.reading))
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

    let pre_queries = query!(
        "DO $$ BEGIN
        ALTER TABLE surface_forms DROP CONSTRAINT surface_forms_variants_fkey;
        ALTER TABLE surface_forms DROP CONSTRAINT surface_forms_pkey;
        ALTER TABLE variants DROP CONSTRAINT variants_lemmas_fkey;
        ALTER TABLE variants DROP CONSTRAINT variants_pkey;
        ALTER TABLE lemmas DROP CONSTRAINT lemmas_pkey;
        DROP INDEX lemma_spelling_reading;
        DROP INDEX variants_spelling_reading;
        DROP INDEX surface_forms_spelling_reading;
      END$$;
    "
    );

    let post_queries = query!(
        "DO $$ BEGIN
        ALTER TABLE lemmas ADD CONSTRAINT lemmas_pkey PRIMARY KEY (id);
        ALTER TABLE variants ADD CONSTRAINT variants_pkey PRIMARY KEY (id);
        ALTER TABLE variants ADD CONSTRAINT variants_lemmas_fkey FOREIGN KEY (lemma_id) REFERENCES lemmas (id);
        ALTER TABLE surface_forms ADD CONSTRAINT surface_forms_pkey PRIMARY KEY (id);
        ALTER TABLE surface_forms ADD CONSTRAINT surface_forms_variants_fkey FOREIGN KEY (variant_id) REFERENCES variants (id);
        CREATE INDEX lemma_spelling_reading ON lemmas (spelling, reading) INCLUDE (id);
        CREATE INDEX variants_spelling_reading ON variants (spelling, reading) INCLUDE (id, lemma_id);
        CREATE INDEX surface_forms_spelling_reading ON surface_forms (spelling, reading) INCLUDE (id, variant_id);
        ANALYZE lemmas;
        ANALYZE variants;
        ANALYZE surface_forms;
      END$$;
     ");

    let mut surface_forms = HashMap::new();
    let mut lemmas = HashMap::new();
    let mut variant_counter: i64 = 1;
    let mut variants = HashMap::new();

    UnidicSession::with_terms(path, |term| {
        let TermExtract {
            lemma_spelling,
            lemma_reading,
            variant_spelling,
            variant_reading,
            surface_form_spelling,
            surface_form_reading,
        } = term.surface_form();

        // We get our lemmas from the list of surface forms, and each lemma
        // corresponds to potentially *tons* of those.

        let lemma_id = LemmaId(term.lemma_id.0);
        lemmas.entry(lemma_id).or_insert(NewLemma {
            id: Some(lemma_id),
            spelling: lemma_spelling,
            reading: lemma_reading,
        });

        // Variants don't exist within Unidic, so we have to handle the variant ID
        // ourselves.

        let current_variant_id = VariantId(variant_counter);
        let variant_id = variants
            .entry(current_variant_id)
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
FROM lemmas
JOIN variants ON variants.lemma_id = lemmas.id
JOIN surface_forms ON surface_forms.variant_id = variants.id
JOIN defs ON variants.spelling = defs.spelling AND variants.reading = defs.reading
WHERE surface_forms.id = $1
          "#,
        // FIXME
        id.0
    );

    let ret = query.fetch_all(pool).await.context(SqlxFailure)?;

    Ok(ret)
}
