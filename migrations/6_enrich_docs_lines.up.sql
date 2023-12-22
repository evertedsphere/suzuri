DO $$
BEGIN
  ALTER TABLE docs
    ADD CONSTRAINT docs_pk PRIMARY KEY (id);
  ALTER TABLE docs
    ADD CONSTRAINT docs_title_unique UNIQUE (title);

  ALTER TABLE lines
    ADD CONSTRAINT lines_pk PRIMARY KEY (doc_id, index);
  ALTER TABLE lines ADD CONSTRAINT lines_docs_fk
    FOREIGN KEY (doc_id) REFERENCES docs (id)
    ON DELETE CASCADE;

  ALTER TABLE tokens
    ADD CONSTRAINT tokens_pk PRIMARY KEY (doc_id, line_index, index);
  ALTER TABLE tokens ADD CONSTRAINT tokens_lines_fk
    FOREIGN KEY (doc_id, line_index)
    REFERENCES lines (doc_id, index)
    ON DELETE CASCADE;
  ALTER TABLE tokens
    ADD CONSTRAINT tokens_surface_forms_fk FOREIGN KEY (surface_form_id) REFERENCES surface_forms (id);

  ALTER TABLE morpheme_occs
    ADD CONSTRAINT morpheme_occs_pk PRIMARY KEY (variant_id, index);
  ALTER TABLE morpheme_occs
    ADD CONSTRAINT morpheme_occs_variants_fk FOREIGN KEY (variant_id)
    REFERENCES variants (id)
    ON DELETE CASCADE;
END
$$;
