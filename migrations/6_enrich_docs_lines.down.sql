DO $$
BEGIN
  DROP MATERIALIZED VIEW valid_context_lines CASCADE;

  ALTER TABLE morpheme_occs
    DROP CONSTRAINT morpheme_occs_pk;
  ALTER TABLE morpheme_occs
    DROP CONSTRAINT morpheme_occs_variants_fk;

  ALTER TABLE tokens
    DROP CONSTRAINT tokens_pk;
  ALTER TABLE tokens
    DROP CONSTRAINT tokens_lines_fk;
  ALTER TABLE tokens
    DROP CONSTRAINT tokens_surface_forms_fk;

  ALTER TABLE lines
    DROP CONSTRAINT lines_docs_fk;
  ALTER TABLE lines
    DROP CONSTRAINT lines_pk;

  ALTER TABLE docs
    DROP CONSTRAINT docs_title_unique;
  ALTER TABLE docs
    DROP CONSTRAINT docs_pk;
END
$$;
