-- Add up migration script here
CREATE INDEX terms_spelling_reading ON terms (spelling, reading);
