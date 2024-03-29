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
use szr_dict::DefContent;
use szr_features::{
    FourthPos, LemmaSource, MainPos, SecondPos, TermExtract, ThirdPos, UnidicLemmaId,
    UnidicSession, UnidicSurfaceFormId,
};
use szr_html::{Doc, DocRender, Z};
use szr_ruby::Span;
use szr_srs::{MemoryStatus, Mneme};
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

#[derive(Debug, Clone)]
pub struct Lemma {
    pub id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
    pub disambiguation: Option<String>,
    pub main_pos: MainPos,
    pub second_pos: SecondPos,
    pub third_pos: ThirdPos,
    pub fourth_pos: FourthPos,
    pub comes_from: String,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub id: VariantId,
    pub lemma_id: LemmaId,
    pub spelling: String,
    pub reading: Option<String>,
}

#[derive(Debug, Clone)]
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
        String,
    );

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        query!(
            r#"
COPY lemmas (id, spelling, disambiguation, reading, main_pos, second_pos, third_pos, fourth_pos, comes_from)
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
            ins.comes_from,
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
    // TODO import only custom / only base
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

    UnidicSession::with_terms(path, user_dict_path, |lemma_type, term| {
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

        let comes_from = match lemma_type {
            LemmaSource::Custom => "custom",
            LemmaSource::Unidic => "unidic",
        }
        .to_string();

        lemmas.entry(lemma_id).or_insert(Lemma {
            id: lemma_id,
            spelling: main_spelling,
            disambiguation,
            reading: lemma_reading,
            main_pos: term.main_pos,
            second_pos: term.second_pos,
            third_pos: term.third_pos,
            fourth_pos: term.fourth_pos,
            comes_from,
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

#[derive(Deserialize)]
pub struct TagDefGroup {
    pub tags: Vec<String>,
    pub contents: Vec<DefContent>,
}

pub struct DefGroup {
    pub dict_name: String,
    pub groups_by_tag: Json<Vec<TagDefGroup>>,
}

#[instrument(skip(pool), err, level = "debug", fields(count))]
async fn get_meanings(pool: &PgPool, id: VariantId) -> Result<Vec<DefGroup>> {
    let query = sqlx::query_as!(
        DefGroup,
        r#"
WITH
  candidate_variants AS (
    SELECT variants.id, variants.spelling, variants.reading
    FROM variants WHERE variants.id = $1),
  candidate_lemmas AS (
    SELECT lemmas.id, lemmas.spelling, lemmas.reading
    FROM variants
    JOIN lemmas ON lemmas.id = variants.lemma_id
    WHERE variants.id = $1),
  candidates AS (
    (SELECT spelling, reading FROM candidate_variants) UNION
    (SELECT spelling, reading FROM candidate_lemmas)),

  results AS (
    SELECT DISTINCT ON (defs.content)
        defs.dict_name,
        defs.tags,
        defs.id,
        defs.content
    FROM defs
    JOIN candidates
    ON candidates.spelling = defs.spelling AND candidates.reading = defs.reading
    GROUP BY defs.dict_name, defs.tags, defs.id, defs.content),

  g AS (
    SELECT
      dict_name, tags,
      row_number() OVER (PARTITION BY (dict_name, tags)) AS n,
      jsonb_agg (content ORDER BY id) contents
    FROM results
    GROUP BY dict_name, tags),

  h AS (
    SELECT dict_name,
    jsonb_agg(jsonb_build_object('tags', tags, 'contents', contents)
      ORDER BY n)
      "groups_by_tag!: Json<Vec<TagDefGroup>>"
    FROM g
    GROUP BY dict_name
  )

select * from h
;
          "#,
        id.0
    );

    let ret = query.fetch_all(pool).await.context(SqlxFailure)?;
    tracing::Span::current().record("count", ret.len());

    Ok(ret)
}

#[derive(Debug)]
pub enum RubySpan {
    // TODO coalesce small kana
    Kana { kana: String },
    Kanji { spelling: String, reading: String },
}

#[derive(Debug)]
/// A ruby span seen in the context of another, which it may match fully,
/// partially, or not at all.
pub struct RelativeRubySpan {
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
        Z.ruby(self.spelling(), Some(self.reading()), None)
    }
}

#[derive(Debug)]
pub struct VariantLink {
    pub is_full_match: bool,
    pub variant_id: VariantId,
    pub ruby: Vec<RelativeRubySpan>,
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
    id: VariantId,
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
    let q = sqlx::query_as!(
        RawSpanLink,
        "SELECT * FROM related_words_for_variant($1, $2, $3)",
        count,
        extra_count,
        id.0
    )
    .fetch_all(pool)
    .await;

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
                                .map(|(s, r, match_type)| RelativeRubySpan {
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
    pub num_hits: i64,
    pub doc_title: String,
    pub sentences: Vec<ContextBlock>,
}

#[derive(Debug, Deserialize_tuple)]
pub struct ContextBlock {
    pub line_index: i32,
    pub is_favourite: bool,
    pub hit_pre_context: Vec<Vec<ContextSentenceToken>>,
    pub hit_context: Vec<Vec<ContextSentenceToken>>,
    pub hit_post_context: Vec<Vec<ContextSentenceToken>>,
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
    variant_id: VariantId,
    num_per_book: u32,
    num_books: u32,
) -> Result<Vec<SentenceGroup>> {
    struct RawSentenceGroup {
        doc_id: i32,
        doc_title: String,
        num_hits: i64,
        sentences: Json<Vec<ContextBlock>>,
    }

    let q = sqlx::query_as!(
        RawSentenceGroup,
        r#"
WITH
  eligible_docs
    AS (
      SELECT
        v.doc_id, count(*) AS num_hits,
        count(*) FILTER (WHERE is_favourite) AS num_fav_hits,
        is_finished
      FROM
        valid_context_lines AS v
        JOIN docs ON docs.id = v.doc_id
        JOIN lines ON lines.doc_id = v.doc_id AND lines.index = v.line_index
      WHERE
        variant_id = $1
        OR (CASE WHEN docs.progress = 0 THEN false ELSE v.line_index <= docs.progress END)
      GROUP BY
        v.doc_id, docs.is_finished
      ORDER BY
        count(*) FILTER (WHERE is_favourite) DESC,
        count(*) DESC
      LIMIT
        $3
    ),
  matches
    AS (
      SELECT
        valid_context_lines.doc_id,
        line_index,
        line_length,
        is_favourite,
        row_number() OVER (PARTITION BY valid_context_lines.doc_id ORDER BY is_favourite DESC, line_index) AS length_rank
      FROM
        valid_context_lines
        JOIN eligible_docs USING (doc_id)
        JOIN lines ON lines.index = line_index AND lines.doc_id = valid_context_lines.doc_id
      WHERE
        variant_id = $1 AND (is_favourite OR is_finished)
    ),
  matching_lines_json_flat
    AS (
      SELECT
        matches.doc_id,
        matches.line_index match_line_index,
        docs.title AS doc_title,
        max(eligible_docs.num_hits) AS num_hits,
        max(eligible_docs.num_fav_hits) AS num_fav_hits,
        t.line_index line_index,
        length_rank,
        -- to show shortest sentences first
        jsonb_agg(
            jsonb_build_array(
                v.id,
                t.content,
                CASE
                WHEN v.id IS NULL THEN false
                ELSE v.id = $1
                END
            ) ORDER BY t.index ASC
        )
          AS sentence,
        is_favourite
      FROM
        matches
        JOIN tokens AS t ON t.doc_id = matches.doc_id
             AND t.line_index >= matches.line_index - $4
             AND t.line_index <= matches.line_index + $5
        JOIN surface_forms AS s ON t.surface_form_id = s.id
        JOIN variants AS v ON s.variant_id = v.id
        JOIN docs ON docs.id = matches.doc_id
        JOIN eligible_docs ON docs.id = eligible_docs.doc_id
      WHERE
        length_rank <= $2
      GROUP BY
        matches.doc_id, matches.line_index, t.line_index, matches.line_length, docs.title, length_rank, is_favourite
    ),
  matching_lines_json
    AS (
      SELECT
        doc_id, match_line_index, length_rank, doc_title, num_hits, num_fav_hits, is_favourite,
        -- might be the first line in the book!
        coalesce(jsonb_agg(sentence ORDER BY line_index ASC) FILTER (WHERE line_index < match_line_index), '[]'::jsonb) pre_context_block,
        -- let's not bother with producing a single element. who knows, maybe we want closer and farther context
        jsonb_agg(sentence ORDER BY line_index ASC) FILTER (WHERE line_index = match_line_index) context_block,
        coalesce(jsonb_agg(sentence ORDER BY line_index ASC) FILTER (WHERE line_index > match_line_index), '[]'::jsonb) post_context_block
      FROM matching_lines_json_flat
      GROUP BY
        doc_id, match_line_index, doc_title, num_hits, num_fav_hits, length_rank, is_favourite
    )
SELECT
  doc_title,
  doc_id AS "doc_id!: i32",
  num_hits AS "num_hits!: i64",
  jsonb_agg(jsonb_build_array(match_line_index, is_favourite, pre_context_block, context_block, post_context_block) ORDER BY length_rank ASC)
    AS "sentences!: Json<Vec<ContextBlock>>"
FROM
  matching_lines_json
GROUP BY
  doc_title, num_hits, num_fav_hits, doc_id
ORDER BY
  num_fav_hits DESC,
  num_hits DESC;
"#,
        variant_id.0,
        num_per_book as i64,
        num_books as i64,
        1 as i64,
        1 as i64,
    );

    let res = q.fetch_all(pool).await.unwrap();

    let ret = res
        .into_iter()
        .map(
            |RawSentenceGroup {
                 doc_id,
                 doc_title,
                 sentences,
                 num_hits,
             }| SentenceGroup {
                doc_id,
                doc_title,
                num_hits,
                sentences: sentences.0,
            },
        )
        .collect::<Vec<_>>();

    Ok(ret)
}

pub struct LookupData {
    pub variant_id: VariantId,
    pub meanings: Vec<DefGroup>,
    pub ruby: Option<Vec<RubySpan>>,
    pub sibling_variants_ruby: Vec<VariantRuby>,
    pub mneme: Option<Mneme>,
}

pub struct VariantRuby {
    pub variant_id: VariantId,
    pub ruby: Vec<RubySpan>,
}

impl LookupData {
    #[instrument(skip(pool), err)]
    pub async fn get_by_id(pool: &PgPool, variant_id: VariantId) -> Result<LookupData> {
        let meanings = get_meanings(&pool, variant_id).await?;

        let ruby: Option<Vec<RubySpan>> = sqlx::query_scalar!(
            r#"
select jsonb_agg(jsonb_build_array(m.spelling, m.reading) order by m.index asc)
  "ruby: Json<Vec<(String, String)>>"
from variants v
join morpheme_occs m on m.variant_id = v.id
where v.id = $1;
"#,
            variant_id.0
        )
        .fetch_one(pool)
        .await
        .unwrap()
        .map(|v| v.0.into_iter().map(|(s, r)| RubySpan::new(s, r)).collect());

        let sibling_variants_ruby: Vec<VariantRuby> = sqlx::query_scalar!(
            r#"
select jsonb_build_array(v.id,
  jsonb_agg(jsonb_build_array(m.spelling, m.reading) order by m.index asc))
  "variant_ruby!: Json<(VariantId, Vec<(String, String)>)>"
from variants v
join morpheme_occs m on m.variant_id = v.id
join lemmas on lemmas.id = v.lemma_id
join variants on lemmas.id = variants.lemma_id
where variants.id = $1
AND v.id <> $1
group by v.id
;
"#,
            variant_id.0
        )
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|Json((variant_id, ruby))| VariantRuby {
            variant_id,
            ruby: ruby.into_iter().map(|(s, r)| RubySpan::new(s, r)).collect(),
        })
        .collect();

        let mneme_id = sqlx::query_scalar!(
            r#"SELECT mneme_id "mneme_id: Uuid" FROM variants where id = $1"#,
            variant_id.0
        )
        .fetch_one(pool)
        .await
        .expect(&format!("get mneme for variant {}", variant_id.0));

        let mneme = if let Some(mneme_id) = mneme_id {
            Some(Mneme::get_by_id(pool, mneme_id).await.unwrap())
        } else {
            None
        };

        let r = Self {
            variant_id,
            meanings,
            ruby,
            mneme,
            sibling_variants_ruby,
        };

        Ok(r)
    }
}

#[derive(Deserialize)]
pub struct MnemeRefreshDatum {
    pub variant_id: VariantId,
    pub is_due: bool,
    pub status: MemoryStatus,
}

pub struct MnemeRefreshBatch {
    pub next_refresh_in_sec: Option<i32>,
    pub mneme_refresh_data: Json<Vec<MnemeRefreshDatum>>,
}

// TODO "since book was loaded"?
#[instrument(level = "debug", skip_all)]
pub async fn get_mneme_refresh_batch(pool: &PgPool) -> Result<MnemeRefreshBatch> {
    let data = sqlx::query_as!(
        MnemeRefreshBatch,
        r#"
select
  extract('epoch' from
    min(next_due - current_timestamp)
      filter (where next_due > current_timestamp)
    )::integer
    "next_refresh_in_sec?: i32",
  jsonb_agg(jsonb_build_object(
    'variant_id', variants.id,
    'is_due', current_timestamp > next_due,
    'status', status))
    "mneme_refresh_data!: Json<Vec<MnemeRefreshDatum>>"
from mnemes
join mneme_states on state_id = mneme_states.id
join variants on variants.mneme_id = mnemes.id;
"#
    )
    .fetch_one(pool)
    .await
    .unwrap_or(MnemeRefreshBatch {
        next_refresh_in_sec: None,
        mneme_refresh_data: Json(Default::default()),
    });

    Ok(data)
}
