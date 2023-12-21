DO $$
BEGIN
  ALTER TABLE docs
    ADD CONSTRAINT docs_pk PRIMARY KEY (id);
  ALTER TABLE docs
    ADD CONSTRAINT docs_title_unique UNIQUE (title);

  ALTER TABLE lines
    ADD CONSTRAINT lines_pk PRIMARY KEY (id);
  ALTER TABLE lines
    ADD CONSTRAINT lines_docs_fk FOREIGN KEY (doc_id) REFERENCES docs (id);
  CREATE UNIQUE INDEX lines_doc_id_index ON lines (doc_id, index) INCLUDE (id);

  ALTER TABLE tokens
    ADD CONSTRAINT tokens_pk PRIMARY KEY (id);
  ALTER TABLE tokens
    ADD CONSTRAINT tokens_lines_fk FOREIGN KEY (line_id) REFERENCES lines (id);
  ALTER TABLE tokens
    ADD CONSTRAINT tokens_surface_forms_fk FOREIGN KEY (surface_form_id) REFERENCES surface_forms (id);
  -- no real need for a unique index on (line_id, index)
END
$$;
