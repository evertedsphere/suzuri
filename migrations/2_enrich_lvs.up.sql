DO $$
BEGIN
  ALTER TABLE lemmas
    ADD CONSTRAINT lemmas_pk PRIMARY KEY (id);
  ALTER TABLE variants
    ADD CONSTRAINT variants_pk PRIMARY KEY (id);
  ALTER TABLE variants
    ADD CONSTRAINT variants_lemmas_fk FOREIGN KEY (lemma_id) REFERENCES lemmas (id);
  ALTER TABLE surface_forms
    ADD CONSTRAINT surface_forms_pk PRIMARY KEY (id);
  ALTER TABLE surface_forms
    ADD CONSTRAINT surface_forms_variants_fk FOREIGN KEY (variant_id) REFERENCES variants (id);
  CREATE INDEX lemma_spelling_reading ON lemmas (spelling, reading) INCLUDE (id);
  CREATE INDEX variants_spelling_reading ON variants (spelling, reading) INCLUDE (id, lemma_id);
  CREATE INDEX surface_forms_spelling_reading ON surface_forms (spelling, reading) INCLUDE (id, variant_id);
END
$$;

