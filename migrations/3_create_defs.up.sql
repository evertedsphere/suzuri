CREATE TABLE defs (
  id int GENERATED ALWAYS AS IDENTITY,
  -- TODO separate dicts table
  dict_name text NOT NULL,
  spelling text NOT NULL,
  reading text NOT NULL,
  content jsonb NOT NULL,
  tags jsonb NOT NULL
);
