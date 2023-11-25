# 硯

## a friendly immersion panopticon

### credits

* the morphological analyser is forked from notmecab-rs
* the templating engine is forked from stpl

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
