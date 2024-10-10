# 硯

Suzuri is a friendly panopticon for the committed immersion-minded language learner.

## Credits

* The morphological analyser is forked from notmecab-rs.
* The templating engine is forked from stpl.

## Setup

``` sh
docker run \
--name szr_pg \
-e POSTGRES_PASSWORD=admin \
-e POSTGRES_DB=szr \
-e POSTGRES_USER=admin \
-p 5432:5432 \
-d postgres
```

``` sh
psql 'postgresql://admin:admin@localhost:5432/szr'
```

## Notes

### Data modeling and SQL idioms

* Table names in plural, SQL in uppercase.

* Any table intended to be bulk inserted into should have its migrations
  structured as follows: one migration (reversible since the next one is),
  possibly with multiple statements, that creates the tables without adding any
  constraints or indexes on them except `NOT NULL` ones where needed; and a
  second (necessarily reversible) one in a `DO $$ BEGIN ... END $$` block that
  adds all of the constraints and creates all of the indexes. This will enable
  us to use [`szr_bulk_insert`](./szr_bulk_insert) to drop the constraints and
  indexes before doing a bulk insert and recreate them after, using the same SQL
  that we use for the migrations (see the `sqlx::query_file{,_as}!` macros).
  When adding a bunch of related tables that will necessarily be inserted into
  in tandem (e.g. `lemmas` / `variants` / `surface_forms`), consider doing them
  in the same migrations. This makes things easier while not actually reducing
  expressiveness at all under the current model.
   
### Rust errors

* For errors, when prototyping, prefer a minimal `snafu` setup with a
  `#[snafu(whatever)]` variant, with a FIXME on it indicating that it is to be
  removed as part of a refactor as soon as possible. This is a good starting
  point:

  ```rust
  type Result<T, E = Error> = std::result::Result<T, E>;

  #[derive(Debug, Snafu)]
  #[snafu(context(suffix(Error)))]
  pub enum Error {
      /// FIXME remove this
      #[snafu(whatever, display("{message}: {source:?}"))]
      CatchallError {
          message: String,
          #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
          source: Option<Box<dyn std::error::Error>>,
      },
  }
  ```

  This also means avoiding `snafu::Whatever`: it does not save any characters at
  use-sites (it's the same `.whatever_context()`), while making the barriers to
  gradually improving the errors in the module much higher, since every use of
  `Result` now has to be audited.

* The context selector suffix is of note: while unorthodox, I like making the
  context selectors have the same names as the variants in the actual error
  type. This makes renaming both at the same time a lot easier, as well as
  traversing a file when auditing multiple uses of the same "error type".
  
* The `Error` suffix in the names is something of a hack: ideally the name of
  the error should not just be an _area_ (e.g. `DatabaseError`) but rather
  indicate the actual _problem_ (e.g. `InsertFailed`). This is something I
  intend  to improve over time, with per-variant context selector changes (or
  perhaps a default `false` selector and opt-in `Error` as suffix).
