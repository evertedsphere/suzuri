CREATE TYPE memory_status AS ENUM (
  'Learning',
  'Reviewing',
  'Relearning'
);

CREATE TYPE review_grade AS ENUM (
  'Fail',
  'Hard',
  'Okay',
  'Easy'
);

CREATE TABLE mneme_states (
  id uuid PRIMARY KEY,
  -- index int NOT NULL,
  -- mneme_id NOT NULL REFERENCES mnemes (id),
  grade review_grade NOT NULL,
  status memory_status NOT NULL,
  due_at timestamptz NOT NULL,
  reviewed_at timestamptz NOT NULL,
  difficulty float8 NOT NULL,
  stability float8 NOT NULL
);

CREATE TABLE mnemes (
  id uuid PRIMARY KEY,
  created_at timestamptz NOT NULL,
  next_due timestamptz NOT NULL,
  state_id uuid NOT NULL REFERENCES mneme_states (id)
);
