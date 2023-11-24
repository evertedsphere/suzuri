-- Add up migration script here

CREATE TABLE freq_terms (
  dict TEXT NOT NULL,
  spelling TEXT NOT NULL,
  reading TEXT NOT NULL,
  frequency INTEGER NOT NULL
);
