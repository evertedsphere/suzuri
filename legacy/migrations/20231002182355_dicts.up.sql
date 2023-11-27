CREATE TABLE terms (
  id SERIAL PRIMARY KEY,
  dict TEXT NOT NULL,
  spelling TEXT NOT NULL,
  reading TEXT NOT NULL,
  defs JSON NOT NULL
);
