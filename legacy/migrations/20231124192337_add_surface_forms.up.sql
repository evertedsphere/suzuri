-- Add up migration script here

CREATE TABLE surface_forms (
  id INTEGER PRIMARY KEY,
  data JSON NOT NULL
);
