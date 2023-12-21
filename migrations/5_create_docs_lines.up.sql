CREATE TABLE docs (
  id int GENERATED ALWAYS AS IDENTITY,
  title text NOT NULL
);

CREATE TABLE lines (
  id int GENERATED ALWAYS AS IDENTITY,
  doc_id int NOT NULL, -- fk to docs (id)
  index int NOT NULL,
  content text NOT NULL
);

CREATE TABLE tokens (
  id int GENERATED ALWAYS AS IDENTITY,
  line_id int NOT NULL, -- fk to lines (id)
  index int NOT NULL,
  content text NOT NULL,
  surface_form_id bigint NOT NULL -- fk to surface_forms (id)
);
