CREATE TABLE defs (
  id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  dict_name TEXT NOT NULL,
  spelling TEXT NOT NULL,
  reading TEXT NOT NULL,
  content JSONB NOT NULL
);

CREATE INDEX defs_spelling_reading ON defs (spelling, reading);
