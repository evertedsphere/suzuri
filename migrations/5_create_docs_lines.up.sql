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

CREATE TABLE morpheme_occs (
  variant_id bigint NOT NULL,
  index int NOT NULL,
  spelling text NOT NULL,
  reading text NOT NULL,
  underlying_reading text NOT NULL
);
