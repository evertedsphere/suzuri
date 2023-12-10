DO $$
BEGIN
  ALTER TABLE defs
    DROP CONSTRAINT defs_pkey;
  DROP INDEX defs_spelling_reading;
END
$$;

