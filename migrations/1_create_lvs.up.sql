CREATE TABLE lemmas (
  id uuid DEFAULT gen_random_uuid(),
  spelling text NOT NULL,
  -- empty for e.g. punctuation
  disambiguation text,
  reading text,
  main_pos text NOT NULL,
  second_pos text NOT NULL,
  third_pos text NOT NULL,
  fourth_pos text NOT NULL,
  is_custom bool NOT NULL
);

CREATE TABLE variants (
  id uuid DEFAULT gen_random_uuid(),
  -- a variant is always a variant of some lemma
  lemma_id uuid NOT NULL,
  spelling text NOT NULL,
  -- empty for e.g. punctuation
  reading text,
  CONSTRAINT variants_pk PRIMARY KEY (id)
);


CREATE TABLE surface_forms (
  -- allow overriding
  id uuid DEFAULT gen_random_uuid(),
  -- may not be known
  variant_id uuid NOT NULL,
  spelling text NOT NULL,
  -- may not have a known one
  reading text,
  CONSTRAINT surface_forms_pk PRIMARY KEY (id) INCLUDE (variant_id)
);
