DO $$ BEGIN
    ALTER TABLE surface_forms DROP CONSTRAINT surface_forms_variants_fkey;
    ALTER TABLE surface_forms DROP CONSTRAINT surface_forms_pkey;
    ALTER TABLE variants DROP CONSTRAINT variants_lemmas_fkey;
    ALTER TABLE variants DROP CONSTRAINT variants_pkey;
    ALTER TABLE lemmas DROP CONSTRAINT lemmas_pkey;
    DROP INDEX lemma_spelling_reading;
    DROP INDEX variants_spelling_reading;
    DROP INDEX surface_forms_spelling_reading;
END $$;
