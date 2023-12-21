CREATE TABLE docs (
  id int GENERATED ALWAYS AS IDENTITY,
  title text NOT NULL
);

CREATE TABLE lines (
  doc_id int NOT NULL, -- fk to docs (id)
  index int NOT NULL
);

CREATE TABLE tokens (
  doc_id int NOT NULL,
  line_index int NOT NULL, -- fk to lines
  index int NOT NULL,
  content text NOT NULL,
  surface_form_id bigint -- fk to surface_forms (id)
);
