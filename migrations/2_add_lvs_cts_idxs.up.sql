DO $$
BEGIN
  ALTER TABLE lemmas
    ADD CONSTRAINT lemmas_pkey PRIMARY KEY (id);
  ALTER TABLE variants
    ADD CONSTRAINT variants_pkey PRIMARY KEY (id);
  ALTER TABLE variants
    ADD CONSTRAINT variants_lemmas_fkey FOREIGN KEY (lemma_id) REFERENCES lemmas (id);
  ALTER TABLE surface_forms
    ADD CONSTRAINT surface_forms_pkey PRIMARY KEY (id);
  ALTER TABLE surface_forms
    ADD CONSTRAINT surface_forms_variants_fkey FOREIGN KEY (variant_id) REFERENCES variants (id);
  CREATE INDEX lemma_spelling_reading ON lemmas (spelling, reading) INCLUDE (id);
  CREATE INDEX variants_spelling_reading ON variants (spelling, reading) INCLUDE (id, lemma_id);
  CREATE INDEX surface_forms_spelling_reading ON surface_forms (spelling, reading) INCLUDE (id, variant_id);
END
$$;

