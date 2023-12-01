CREATE TABLE defs (
  def_id SERIAL PRIMARY KEY,
  def_dict_name TEXT NOT NULL,
  def_spelling TEXT NOT NULL,
  def_reading TEXT NOT NULL,
  def_content JSONB NOT NULL
);

CREATE INDEX defs_spelling_reading ON defs (def_spelling, def_reading);
