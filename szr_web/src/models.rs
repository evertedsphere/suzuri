use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use serde_tuple::Deserialize_tuple;
use snafu::{ResultExt, Snafu};
use sqlx::{
    postgres::PgArguments,
    query,
    query::Query,
    types::{Json, Uuid},
    PgPool, Postgres,
};
use szr_bulk_insert::PgBulkInsert;
use szr_dict::Def;
use szr_features::{
    FourthPos, MainPos, SecondPos, TermExtract, ThirdPos, UnidicLemmaId, UnidicSession,
    UnidicSurfaceFormId,
};
use szr_html::{Doc, DocRender, Z};
use szr_ruby::Span;
use tracing::{instrument, trace, trace_span};

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
    RubyFailure {
        source: szr_ruby::Error,
    },
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type, PartialOrd, Ord, Serialize)]
pub struct LemmaId(pub Uuid);

impl LemmaId {
    pub fn from_unidic(id: UnidicLemmaId) -> Self {
        Self(Uuid::from_u64_pair(0, id.0 as u64))
    }
}

impl ::std::fmt::Display for LemmaId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type, PartialOrd, Ord, Serialize)]
pub struct SurfaceFormId(pub Uuid);

impl ::std::fmt::Display for SurfaceFormId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl SurfaceFormId {
    pub fn from_unidic(id: UnidicSurfaceFormId) -> Self {
        Self(Uuid::from_u64_pair(0, id.0 as u64))
    }
}

#[derive(
    Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct VariantId(pub Uuid);

impl VariantId {
    pub fn from_u64(id: u64) -> Self {
        Self(Uuid::from_u64_pair(0, id))
    }
}

impl ::std::fmt::Display for VariantId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// Lemmas

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct Variant {
    pub id: VariantId,
    pub lemma_id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct SurfaceForm {
    pub id: SurfaceFormId,
    pub variant_id: VariantId,
    pub spelling: String,
    pub reading: Option<String>,
}

impl PgBulkInsert for Lemma {
    type InsertFields = Lemma;
    type SerializeAs = (
        LemmaId,
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
    type InsertFields = Variant;
    type SerializeAs = (VariantId, LemmaId, String, Option<String>);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!("COPY variants (id, lemma_id, spelling, reading) FROM STDIN WITH (FORMAT CSV)")
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.id, ins.lemma_id, ins.spelling, ins.reading))
    }
}

impl PgBulkInsert for SurfaceForm {
    type InsertFields = SurfaceForm;
    type SerializeAs = (SurfaceFormId, VariantId, String, Option<String>);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!(
            "COPY surface_forms (id, variant_id, spelling, reading) FROM STDIN WITH (FORMAT CSV)"
        )
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((ins.id, ins.variant_id, ins.spelling, ins.reading))
    }
}

impl PgBulkInsert for MorphemeOcc {
    type InsertFields = MorphemeOcc;
    type SerializeAs = (VariantId, i32, String, String, String);

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!("COPY morpheme_occs (variant_id, index, spelling, reading, underlying_reading) FROM STDIN WITH (FORMAT CSV)")
    }

    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs, szr_bulk_insert::Error> {
        Ok((
            ins.variant_id,
            ins.index,
            ins.spelling,
            ins.reading,
            ins.underlying_reading,
        ))
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize)]
pub struct MorphemeOcc {
    pub variant_id: VariantId,
    pub index: i32,
    pub spelling: String,
    pub reading: String,
    pub underlying_reading: String,
}

#[instrument(skip(pool, path), err, level = "debug")]
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
    let mut variants = HashMap::new();
    let mut annotation_inputs = Vec::new();
    let mut variant_counter = 0;

    let kd =
        szr_ruby::read_kanjidic("/home/s/c/szr/data/system/readings.json").context(RubyFailure)?;

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

        let lemma_id = LemmaId::from_unidic(term.lemma_id);
        lemmas.entry(lemma_id).or_insert(Lemma {
            id: lemma_id,
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

        let variant_id = variants
            .entry((lemma_id, variant_spelling.clone(), variant_reading.clone()))
            .or_insert({
                variant_counter += 1;
                Variant {
                    id: VariantId::from_u64(variant_counter),
                    lemma_id,
                    spelling: variant_spelling.clone(),
                    reading: variant_reading.clone(),
                }
            })
            .id;

        // FIXME morpheme_occs should use surface forms
        if let Some(ref variant_reading) = variant_reading {
            annotation_inputs.push((
                variant_id,
                variant_spelling.clone(),
                variant_reading.clone(),
            ));
        }

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

        let surface_form_id = SurfaceFormId::from_unidic(term.lemma_guid);
        surface_forms.entry(surface_form_id).or_insert(SurfaceForm {
            id: surface_form_id,
            variant_id,
            spelling: surface_form_spelling,
            reading: surface_form_reading,
        });

        Ok(())
    })
    .context(TokeniseFailure)?;

    let morpheme_occs = trace_span!("produce morpheme data").in_scope(|| {
        annotation_inputs
            .into_par_iter()
            .filter_map(|(variant_id, variant_spelling, variant_reading)| {
                let r: HashSet<_> = szr_ruby::annotate(&variant_spelling, &variant_reading, &kd)
                    .ok()?
                    .valid()?
                    .iter()
                    .enumerate()
                    .map(|(index, span)| {
                        let (spelling, reading, underlying_reading) = match span {
                            Span::Kanji {
                                kanji,
                                yomi,
                                dict_yomi,
                                ..
                            } => (kanji.to_string(), yomi.to_owned(), dict_yomi.to_owned()),
                            Span::Kana {
                                kana, pron_kana, ..
                            } => (kana.to_string(), pron_kana.to_string(), kana.to_string()),
                        };
                        MorphemeOcc {
                            variant_id,
                            index: index as i32,
                            spelling,
                            reading,
                            underlying_reading,
                        }
                    })
                    .collect();
                Some(r)
            })
            .flatten()
            .collect::<HashSet<_>>()
    });

    let lemmas = lemmas.into_values().collect();
    let surface_forms = surface_forms.into_values().collect();
    let variants = variants.into_values().collect();
    let morpheme_occs = morpheme_occs.into_iter().collect();

    // Start the actual bulk insert.

    let mut tx = pool.begin().await.context(SqlxFailure)?;

    // Pre-copy phase
    trace_span!("dropping indexes and constraints")
        .in_scope(|| async { pre_queries.execute(&mut *tx).await.context(SqlxFailure) })
        .await?;

    // Copy phase
    // The foreign key target has to go in first, of course.
    trace_span!("copying records")
        .in_scope(|| async {
            Lemma::copy_records(&mut *tx, lemmas).await?;
            Variant::copy_records(&mut *tx, variants).await?;
            SurfaceForm::copy_records(&mut *tx, surface_forms).await?;
            MorphemeOcc::copy_records(&mut *tx, morpheme_occs).await?;
            Ok(())
        })
        .await
        .context(BulkInsertFailed)?;

    // Post-copy fixup phase
    trace_span!("recreating indexes and constraints")
        .in_scope(|| async { post_queries.execute(&mut *tx).await.context(SqlxFailure) })
        .await?;

    tx.commit().await.context(SqlxFailure)?;

    Ok(())
}

#[derive(Debug, Copy, Clone)]
pub enum LookupId {
    SurfaceForm(SurfaceFormId),
    Variant(VariantId),
}

pub async fn get_meanings(pool: &PgPool, id: LookupId) -> Result<Vec<Def>> {
    match id {
        LookupId::SurfaceForm(id) => get_surface_form_meanings(pool, id).await,
        LookupId::Variant(id) => get_variant_meanings(pool, id).await,
    }
}

#[instrument(skip(pool), err, level = "debug", fields(count))]
pub async fn get_variant_meanings(pool: &PgPool, id: VariantId) -> Result<Vec<Def>> {
    let query = sqlx::query_as!(
        Def,
        r#"
SELECT
    defs.id, defs.dict_name, defs.spelling, defs.reading,
    defs.content as "content: Json<Vec<String>>"
FROM defs
JOIN variants ON variants.spelling = defs.spelling AND variants.reading = defs.reading
WHERE variants.id = $1;
          "#,
        id.0
    );

    let ret = query.fetch_all(pool).await.context(SqlxFailure)?;
    tracing::Span::current().record("count", ret.len());

    Ok(ret)
}

#[instrument(
    skip(pool),
    err,
    level = "debug",
    fields(fallback_used, primary_count, secondary_count)
)]
pub async fn get_surface_form_meanings(pool: &PgPool, id: SurfaceFormId) -> Result<Vec<Def>> {
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

    let fallback_query = sqlx::query_as!(
        Def,
        r#"
SELECT
    defs.id, defs.dict_name, defs.spelling, defs.reading,
    defs.content as "content: Json<Vec<String>>"
FROM defs
JOIN variants ON variants.spelling = defs.spelling AND variants.reading = defs.reading
JOIN lemmas ON variants.lemma_id = lemmas.id
-- widen the search to every "sibling" variant
JOIN variants v ON v.lemma_id = lemmas.id
JOIN surface_forms ON surface_forms.variant_id = v.id
WHERE surface_forms.id = $1;
          "#,
        // FIXME
        id.0
    );

    let ret = query.fetch_all(pool).await.context(SqlxFailure)?;
    tracing::Span::current().record("primary_count", ret.len());

    let use_fallback = !ret.iter().any(|d| d.dict_name != "JMnedict");
    tracing::Span::current().record("fallback_used", use_fallback);
    if !use_fallback {
        Ok(ret)
    } else {
        let sibling_words = fallback_query.fetch_all(pool).await.context(SqlxFailure)?;
        tracing::Span::current().record("secondary_count", sibling_words.len());
        Ok(sibling_words)
    }
}

#[derive(Debug)]
pub enum RubySpan {
    // TODO coalesce small kana
    Kana { kana: String },
    Kanji { spelling: String, reading: String },
}

#[derive(Debug)]
pub struct MatchedRubySpan {
    pub match_type: RubyMatchType,
    pub ruby_span: RubySpan,
}

impl RubySpan {
    #[allow(unused)]
    pub fn is_kana(&self) -> bool {
        matches!(self, Self::Kana { .. })
    }

    fn new(spelling: String, reading: String) -> Self {
        if spelling == reading {
            Self::Kana { kana: spelling }
        } else {
            Self::Kanji { spelling, reading }
        }
    }

    fn reading(&self) -> &str {
        match self {
            Self::Kana { kana } => kana,
            Self::Kanji { reading, .. } => reading,
        }
    }

    fn spelling(&self) -> &str {
        match self {
            Self::Kana { kana } => kana,
            Self::Kanji { spelling, .. } => spelling,
        }
    }
}

impl DocRender for RubySpan {
    fn to_doc(self) -> Doc {
        Z.ruby()
            .c(self.spelling())
            .c(Z.rt().class("relative top-1").c(self.reading()))
    }
}

#[derive(Debug)]
pub struct VariantLink {
    pub is_full_match: bool,
    pub variant_id: VariantId,
    pub ruby: Vec<MatchedRubySpan>,
}

#[derive(Debug)]
pub struct SpanLink {
    pub index: i32,
    pub ruby: RubySpan,
    pub examples: Option<Vec<VariantLink>>,
}

#[derive(Debug, Deserialize)]
pub enum RubyMatchType {
    #[serde(alias = "full_match")]
    /// Both the spelling and the reading match
    FullMatch,
    #[serde(alias = "alternate_reading")]
    /// The spelling matches, but not the reading
    /// TODO special category for rendaku etc differences
    AlternateReading,
    #[serde(alias = "other")]
    /// Character unrelated to the context heading
    NonMatch,
}

#[instrument(skip(pool), err, level = "debug")]
pub async fn get_related_words(
    pool: &PgPool,
    count: u32,
    extra_count: u32,
    id: LookupId,
) -> Result<Vec<SpanLink>> {
    let count = count as i32;
    let extra_count = extra_count as i32;

    // pg threatens to truncate the type if we write it out in the alias
    type Examples = Json<Vec<(bool, Uuid, Vec<(String, String, RubyMatchType)>)>>;
    pub struct RawSpanLink {
        idx: i32,
        span_spelling: String,
        span_reading: String,
        examples: Option<Examples>,
    }
    let q = match id {
        LookupId::SurfaceForm(id) => {
            sqlx::query_as!(
                RawSpanLink,
                "SELECT * FROM related_words_for_surface_form($1, $2, $3)",
                count,
                extra_count,
                id.0
            )
            .fetch_all(pool)
            .await
        }
        LookupId::Variant(id) => {
            sqlx::query_as!(
                RawSpanLink,
                "SELECT * FROM related_words_for_variant($1, $2, $3)",
                count,
                extra_count,
                id.0
            )
            .fetch_all(pool)
            .await
        }
    };

    let r: Vec<_> = q
        .unwrap()
        .into_iter()
        .map(
            |RawSpanLink {
                 idx,
                 span_spelling,
                 span_reading,
                 examples,
             }| SpanLink {
                index: idx,
                ruby: RubySpan::new(span_spelling, span_reading),
                examples: examples.map(|examples| {
                    examples
                        .0
                        .into_iter()
                        .map(|(is_full_match, variant_id, ruby)| VariantLink {
                            is_full_match,
                            variant_id: VariantId(variant_id),
                            ruby: ruby
                                .into_iter()
                                .map(|(s, r, match_type)| MatchedRubySpan {
                                    match_type,
                                    ruby_span: RubySpan::new(s, r),
                                })
                                .collect(),
                        })
                        .collect()
                }),
            },
        )
        .collect();
    Ok(r)
}

#[derive(Debug, Deserialize)]
pub struct SentenceGroup {
    pub doc_id: i32,
    pub doc_title: String,
    pub sentences: Vec<ContextSentence>,
}

#[derive(Debug, Deserialize_tuple)]
pub struct ContextSentence {
    pub line_index: i32,
    pub tokens: Vec<ContextSentenceToken>,
}

#[derive(Debug, Deserialize_tuple)]
pub struct ContextSentenceToken {
    pub variant_id: Option<VariantId>,
    pub content: String,
    pub is_active_word: bool,
}

#[instrument(skip(pool), err, level = "debug")]
pub async fn get_sentences(
    pool: &PgPool,
    id: LookupId,
    num_per_book: u32,
    num_books: u32,
) -> Result<Vec<SentenceGroup>> {
    struct RawSentenceGroup {
        doc_id: i32,
        doc_title: String,
        sentences: Json<Vec<ContextSentence>>,
    }

    let variant_id = match id {
        LookupId::Variant(id) => id,
        LookupId::SurfaceForm(surface_form_id) => {
            let r = sqlx::query_scalar!(
                r#"
SELECT variants.id
FROM variants
JOIN surface_forms ON surface_forms.variant_id = variants.id
WHERE surface_forms.id = $1
"#,
                surface_form_id.0
            )
            .fetch_one(pool)
            .await
            .unwrap();
            VariantId(r)
        }
    };

    let q = sqlx::query_as!(
        RawSentenceGroup,
        r#"
WITH
j AS (
  SELECT
    docs.title doc_title,
    matches.doc_id,
    matches.line_index,
    jsonb_agg(jsonb_build_array(v.id, t.content, CASE WHEN v.id IS NULL THEN false ELSE v.id = $1 END)
    ORDER BY t.index ASC) AS sentence,
    -- hack: until we get proper sentence splitting, just bias towards shorter sentences
    row_number() OVER (PARTITION BY (matches.doc_id) ORDER BY count(t.index) ASC,
  matches.line_index) n
FROM valid_context_lines matches
JOIN tokens t ON t.doc_id = matches.doc_id
  AND matches.line_index = t.line_index
JOIN surface_forms s ON t.surface_form_id = s.id
JOIN variants v ON s.variant_id = v.id
JOIN docs ON docs.id = matches.doc_id
WHERE matches.variant_id = $1
GROUP BY matches.doc_id,
docs.title,
matches.line_index
),

k AS (SELECT
  doc_title,
  doc_id,
  jsonb_agg(jsonb_build_array(line_index, sentence) ORDER BY line_index ASC) sentences
FROM
  j
WHERE n <= $2
GROUP BY doc_title, doc_id
)

SELECT
  doc_title,
  doc_id "doc_id!: i32",
  sentences "sentences!: Json<Vec<ContextSentence>>"
 FROM k
ORDER BY doc_id LIMIT $3;
"#,
        variant_id.0,
        num_per_book as i64,
        num_books as i64,
    );

    let res = q.fetch_all(pool).await.unwrap();

    let ret = res
        .into_iter()
        .map(
            |RawSentenceGroup {
                 doc_id,
                 doc_title,
                 sentences,
             }| SentenceGroup {
                doc_id,
                doc_title,
                sentences: sentences.0,
            },
        )
        .collect::<Vec<_>>();

    Ok(ret)
}
