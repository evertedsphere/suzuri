CREATE TABLE lemmas (
  id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  spelling TEXT NOT NULL,
  reading TEXT NOT NULL
);

-- lol

CREATE UNIQUE INDEX lemmas_spelling_reading ON lemmas (spelling, reading) INCLUDE (id);

CREATE TABLE defs (
  id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  dict_name TEXT NOT NULL,
  spelling TEXT NOT NULL,
  reading TEXT NOT NULL,
  content JSONB NOT NULL
);

CREATE INDEX defs_spelling_reading ON defs (spelling, reading);
