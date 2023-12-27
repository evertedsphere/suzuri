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

  CREATE MATERIALIZED VIEW valid_context_lines AS (
    SELECT
      v.id variant_id,
      t.doc_id,
      t.line_index
    FROM
      tokens t
      JOIN (
        SELECT DISTINCT ON (doc_id, line_index)
          doc_id, line_index, t.content AS last_token
        FROM tokens t
        ORDER BY doc_id, line_index, t.index DESC
      ) el ON el.doc_id = t.doc_id
        AND el.line_index = t.line_index
        AND el.last_token IN ('。', '」','）')
      JOIN surface_forms s ON s.id = t.surface_form_id
      JOIN variants v ON v.id = s.variant_id
    ORDER BY v.id, t.doc_id, t.line_index);

  REFRESH MATERIALIZED VIEW valid_context_lines;

  CREATE INDEX valid_context_lines_search ON valid_context_lines
    (variant_id ASC, doc_id ASC, line_index ASC);
END
$$;
