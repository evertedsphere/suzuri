DO $$
BEGIN
  ALTER TABLE defs
    ADD CONSTRAINT defs_pk PRIMARY KEY (id);
  CREATE INDEX defs_spelling_reading ON defs (spelling, reading);
END
$$;

