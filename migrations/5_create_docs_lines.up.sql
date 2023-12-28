CREATE TABLE docs (
  id int GENERATED ALWAYS AS IDENTITY,
  title text NOT NULL,
  is_finished boolean NOT NULL,
  progress int NOT NULL
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
  surface_form_id uuid -- fk to surface_forms (id)
);

CREATE TABLE morpheme_occs (
  variant_id uuid NOT NULL,
  index int NOT NULL,
  spelling text NOT NULL,
  reading text NOT NULL,
  underlying_reading text NOT NULL
);

CREATE FUNCTION related_words_for_variant (int, int, uuid)
  RETURNS TABLE (
    "idx!: i32" int,
    "span_spelling!: String" text,
    "span_reading!: String" text,
    "examples: Examples" jsonb
  )
  AS $$
  WITH input_morphemes AS (
    SELECT
      m.index,
      m.variant_id,
      m.spelling,
      m.reading,
      m.underlying_reading
    FROM
      morpheme_occs m
      JOIN variants v ON v.id = m.variant_id
    WHERE
      v.id = $3
),
links AS (
  SELECT
    i.index input_index,
    i.spelling input_spelling,
    i.reading input_reading,
    v.id variant_id,
    row_number() OVER w AS example_number,
      (m.reading = i.reading) AS is_full_match,
    (i.spelling = i.reading) AS is_kana
  FROM
    input_morphemes i
    LEFT JOIN morpheme_occs m ON m.spelling = i.spelling
    -- comment out to enable partial matches
    -- AND m.reading = i.reading
      AND m.variant_id <> i.variant_id
      -- AND m.spelling <> m.reading
      -- AND i.spelling <> i.reading
    LEFT JOIN variants v ON v.id = m.variant_id
    -- if there are multiple uses of one s-r pair, only keep the first
WINDOW w AS (PARTITION BY (m.spelling,
    m.reading = i.reading)
ORDER BY
  i.reading,
  i.index,
  v.id)
),
-- TODO handle long vowel marks etc correctly
examples_raw AS (
  SELECT
    links.*,
    jsonb_agg(jsonb_build_array(m.spelling, m.reading, --
        CASE WHEN m.spelling = links.input_spelling THEN
        (
          CASE WHEN m.reading = links.input_reading THEN
            'full_match'
          ELSE
            'alternate_reading'
          END)
      ELSE
        'other'
        END)
    ORDER BY m.index) ruby
  FROM
    links
    LEFT JOIN morpheme_occs m ON m.variant_id = links.variant_id
  WHERE (example_number <= $1
    AND links.is_full_match)
  OR (example_number <= $2
    AND NOT links.is_full_match)
GROUP BY
  links.input_index,
  links.input_spelling,
  links.input_reading,
  links.example_number,
  links.variant_id,
  links.is_full_match,
  is_kana
),
examples_agg AS (
  SELECT
    examples_raw.input_index AS idx,
    examples_raw.input_spelling AS span_spelling,
    examples_raw.input_reading AS span_reading,
    CASE
    -- FIXME don't fix this this late in the query!
    -- trying to filter out at the is_kana decl site kills the left join and
    -- deletes the kana rows fsr
    WHEN is_kana THEN
      NULL
    ELSE
      jsonb_agg_strict (
        CASE WHEN variant_id IS NULL THEN
          NULL
        ELSE
          jsonb_build_array(is_full_match, variant_id, ruby)
        END ORDER BY is_full_match DESC, example_number)
    END examples
  FROM
    examples_raw
  GROUP BY
    input_index,
    input_spelling,
    input_reading,
    is_kana
  ORDER BY
    input_index)
  -- end ctes ----------------------------------------------------------------
  SELECT
    idx,
    span_spelling,
    span_reading,
    CASE
    WHEN jsonb_array_length(examples) = 0 THEN
      NULL
    ELSE
      examples
    END examples
  FROM
    examples_agg
$$
LANGUAGE SQL;

CREATE FUNCTION related_words_for_surface_form (int, int, uuid)
  RETURNS TABLE (
    "idx!: i32" int,
    "span_spelling!: String" text,
    "span_reading!: String" text,
    "examples: Examples" jsonb
  )
  AS $$
  SELECT
    rel.*
  FROM
    surface_forms s
    JOIN variants v ON v.id = s.variant_id
    JOIN related_words_for_variant ($1, $2, v.id) rel ON TRUE
WHERE
  s.id = $3
$$
LANGUAGE SQL;
