CREATE TABLE defs (
  id INT GENERATED ALWAYS AS IDENTITY,
  -- TODO separate dicts table
  dict_name TEXT NOT NULL,
  spelling TEXT NOT NULL,
  reading TEXT NOT NULL,
  content JSONB NOT NULL
);
