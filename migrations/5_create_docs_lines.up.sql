create table docs (
  id int generated always as identity,
  title text not null,
  is_finished boolean not null,
  progress int not null
);

create table lines (
  doc_id int not null, -- fk to docs (id)
  index int not null,
  is_favourite boolean not null
);

create table tokens (
  doc_id int not null,
  line_index int not null, -- fk to lines
  index int not null,
  content text not null,
  surface_form_id uuid -- fk to surface_forms (id)
);

create table morpheme_occs (
  variant_id uuid not null,
  index int not null,
  spelling text not null,
  reading text not null,
  underlying_reading text not null
);

create function related_words_for_variant (int, int, uuid)
  returns table (
    "idx!: i32" int,
    "span_spelling!: String" text,
    "span_reading!: String" text,
    "examples: Examples" jsonb
  )
  as $$
  with input_morphemes as (
    select
      m.index,
      m.variant_id,
      m.spelling,
      m.reading,
      m.underlying_reading
    from
      morpheme_occs m
    where
      m.variant_id = $3
),
links as (
  select distinct on (v.id)
    i.index input_index,
    i.spelling input_spelling,
    i.reading input_reading,
    v.id variant_id,
    row_number() over w as example_number,
      (m.reading = i.reading) as is_full_match,
    (i.spelling = i.reading) as is_kana
  from
    input_morphemes i
    -- preserves kana
    join morpheme_occs m on m.spelling = i.spelling
      and m.variant_id <> i.variant_id
    join variants v on v.id = m.variant_id
    join defs on defs.spelling = v.spelling
      and defs.reading = v.reading
      and defs.dict_name <> 'JMnedict'
window w as (partition by (m.spelling,
    m.reading = i.reading)
order by
  i.reading,
  i.index,
  v.id)
),
-- TODO handle long vowel marks etc correctly
examples_raw as (
  select
    links.*,
    jsonb_agg(jsonb_build_array(m.spelling, m.reading, --
        case when m.spelling = links.input_spelling then
        (
          case when m.reading = links.input_reading then
            'full_match'
          else
            'alternate_reading'
          end)
      else
        'other'
        end)
    order by m.index) ruby
  from
    links
  left join morpheme_occs m on m.variant_id = links.variant_id
  where (example_number <= $1
    and links.is_full_match)
  or (example_number <= $2
    and not links.is_full_match)
group by
  links.input_index,
  links.input_spelling,
  links.input_reading,
  links.example_number,
  links.variant_id,
  links.is_full_match,
  is_kana
),
examples_agg as (
  select
    examples_raw.input_index as idx,
    examples_raw.input_spelling as span_spelling,
    examples_raw.input_reading as span_reading,
    case
    -- FIXME don't fix this this late in the query!
    -- trying to filter out at the is_kana decl site kills the left join and
    -- deletes the kana rows fsr
    when is_kana then
      null
    else
      jsonb_agg_strict (
        case when variant_id is null then
          null
        else
          jsonb_build_array(is_full_match, variant_id, ruby)
        end order by is_full_match desc, example_number)
    end examples
  from
    examples_raw
  group by
    input_index,
    input_spelling,
    input_reading,
    is_kana
  order by
    input_index)
  -- end ctes ----------------------------------------------------------------
  select
    idx,
    span_spelling,
    span_reading,
    case when jsonb_array_length(examples) = 0 then
      null
    else
      examples
    end examples
  from
    examples_agg
$$
language SQL;
