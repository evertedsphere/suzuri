DO $$
BEGIN
  DROP INDEX lines_doc_id_index;
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
