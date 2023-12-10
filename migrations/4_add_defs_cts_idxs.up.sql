DO $$ BEGIN
  ALTER TABLE defs ADD CONSTRAINT defs_pkey PRIMARY KEY (id);
  CREATE INDEX defs_spelling_reading ON defs (spelling, reading);
END $$;
