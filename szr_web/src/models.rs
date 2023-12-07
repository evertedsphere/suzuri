use csv::StringRecord;
use sqlx::{
    postgres::PgArguments,
    query::{Query, QueryScalar},
    Postgres,
};
use szr_dict::BulkCopyInsert;

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, sqlx::Type)]
pub struct LemmaId(pub i32);

#[doc = " Default wrapper"]
impl ::std::fmt::Display for LemmaId {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Lemma {
    pub id: LemmaId,
    pub spelling: String,
    pub reading: String,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct NewLemma {
    pub spelling: String,
    pub reading: String,
}

impl BulkCopyInsert for Lemma {
    type InsertFields = NewLemma;
    type Key = ();

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("COPY lemmas (spelling, reading) FROM STDIN WITH (FORMAT CSV)")
    }

    fn create_indexes_query() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("CREATE UNIQUE INDEX lemmas_spelling_reading ON lemmas (spelling, reading)")
    }

    fn drop_indexes_query() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("DROP INDEX IF EXISTS lemmas_spelling_reading")
    }

    fn analyze_query() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("ANALYZE lemmas")
    }

    fn exists_query(_: &Self::Key) -> QueryScalar<'static, Postgres, Option<bool>, PgArguments> {
        sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM LEMMAS)")
    }

    fn to_string_record(ins: Self::InsertFields) -> StringRecord {
        StringRecord::from(&[ins.spelling, ins.reading][..])
    }
}
