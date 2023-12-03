CREATE TABLE terms (
  id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  spelling VARCHAR NOT NULL,
  reading VARCHAR NOT NULL,
  data JSONB NOT NULL
);

CREATE UNIQUE INDEX terms_spelling_reading ON terms (spelling, reading);
