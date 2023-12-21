DO $$
BEGIN
  ALTER TABLE docs
    ADD CONSTRAINT docs_pk PRIMARY KEY (id);
  ALTER TABLE docs
    ADD CONSTRAINT docs_title_unique UNIQUE (title);

  ALTER TABLE lines
    ADD CONSTRAINT lines_pk PRIMARY KEY (doc_id, index);
  ALTER TABLE lines
    ADD CONSTRAINT lines_docs_fk FOREIGN KEY (doc_id) REFERENCES docs (id);

  ALTER TABLE tokens
    ADD CONSTRAINT tokens_pk PRIMARY KEY (doc_id, line_index, index);
  ALTER TABLE tokens
    ADD CONSTRAINT tokens_lines_fk FOREIGN KEY (doc_id, line_index) REFERENCES lines (doc_id, index);
  ALTER TABLE tokens
    ADD CONSTRAINT tokens_surface_forms_fk FOREIGN KEY (surface_form_id) REFERENCES surface_forms (id);
END
$$;
