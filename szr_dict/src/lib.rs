use diesel::{
    connection::LoadConnection, deserialize::FromSqlRow as FS, dsl::exists,
    expression::AsExpression, pg::Pg, sql_types::Jsonb, Connection, ExpressionMethods, Insertable,
    QueryDsl, Queryable, RunQueryDsl, Selectable,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use sqlx::types::Json;
use szr_diesel_macros::impl_sql_as_jsonb;
use szr_schema::defs;
use tracing::{debug, instrument, warn};

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Definitions(pub Vec<String>);

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Def {
    pub id: i32,
    pub dict_name: String,
    pub spelling: String,
    pub reading: String,
    pub content: Definitions,
}

impl From<Json<Vec<String>>> for Definitions {
    fn from(value: Json<Vec<String>>) -> Self {
        Self(value.0)
    }
}

pub struct NewDef {
    pub dict_name: String,
    pub spelling: String,
    pub reading: String,
    pub content: Definitions,
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    InsertFailedError { source: diesel::result::Error },
}

pub trait DictionaryFormat {
    type Error: std::error::Error;

    fn read_from_path(path: &str, name: &str) -> Result<Vec<NewDef>, Self::Error>;

    #[instrument(skip(conn, dict))]
    fn save_dictionary<C>(conn: &mut C, name: &str, dict: Vec<NewDef>) -> Result<(), Error>
    where
        C: Connection<Backend = Pg> + LoadConnection,
    {
        // let max_arg_count = 200;
        // let already_exists = diesel::select(exists(defs::table.filter(defs::dict_name.eq(name))))
        //     .get_result(conn)
        //     .context(InsertFailedError)?;

        // if already_exists {
        //     warn!("dict {} already exists; not persisting to database", name);
        //     return Ok(());
        // }

        // let num_inserted = conn
        //     .transaction(|conn| {
        //         dict.into_iter()
        //             .chunks(max_arg_count)
        //             .into_iter()
        //             .try_fold(0, |n, input| {
        //                 let input = input.collect::<Vec<_>>();
        //                 let r = diesel::insert_into(defs::table)
        //                     .values(&input)
        //                     .execute(conn)?;
        //                 Ok(n + r)
        //             })
        //     })
        //     .context(InsertFailedError)?;
        // debug!("inserted {} dictionary items", num_inserted);

        Ok(())
    }
}
