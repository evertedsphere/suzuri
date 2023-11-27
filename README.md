# 硯

Suzuri is a friendly panopticon for the committed immersion-minded language learner.

### Credits

* The morphological analyser is forked from notmecab-rs.
* The templating engine is forked from stpl.

### Setup

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
