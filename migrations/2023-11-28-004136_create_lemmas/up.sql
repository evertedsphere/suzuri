CREATE TABLE lemmas (
  id INT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  spelling VARCHAR NOT NULL,
  reading VARCHAR NOT NULL
);

CREATE UNIQUE INDEX lemmas_spelling_reading ON lemmas (spelling, reading) INCLUDE (id);
