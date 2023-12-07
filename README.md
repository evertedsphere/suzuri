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

## Style guidelines

### Errors

* No `unwrap`, period. ~(Ideally.)~

* Avoid the use of `anyhow`. Apart from the issue of error provenance: modulo
  "using words you don't know to form sentences", the trait objects it uses tend
  to do poorly with being sent across `async` boundaries and such or in things
  like responder types for HTTP server libraries.

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
      OtherError {
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
  indicate the actual _problem_ (e.g. `InsertFailed`). This is something I intend 
  to improve over time, with per-variant context selector changes (or perhaps a default
  `false` selector and opt-in `Error` as suffix).
