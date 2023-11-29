CREATE TABLE terms (
       term_id SERIAL PRIMARY KEY,
       term_spelling VARCHAR NOT NULL,
       term_reading VARCHAR NOT NULL,
       term_data JSONB NOT NULL
);

CREATE UNIQUE INDEX terms_spelling_reading ON terms (term_spelling, term_reading);
