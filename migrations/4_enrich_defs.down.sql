DO $$
BEGIN
  ALTER TABLE defs
    DROP CONSTRAINT defs_pk;
  DROP INDEX defs_spelling_reading;
END
$$;

