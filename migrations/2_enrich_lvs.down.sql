DO $$
BEGIN
  ALTER TABLE surface_forms
    DROP CONSTRAINT surface_forms_variants_fk;
  ALTER TABLE surface_forms
    DROP CONSTRAINT surface_forms_pk;
  ALTER TABLE variants
    DROP CONSTRAINT variants_lemmas_fk;
  ALTER TABLE variants
    DROP CONSTRAINT variants_pk;
  ALTER TABLE lemmas
    DROP CONSTRAINT lemmas_pk;
  DROP INDEX lemma_spelling_reading;
  DROP INDEX variants_spelling_reading;
  DROP INDEX surface_forms_spelling_reading;
END
$$;

