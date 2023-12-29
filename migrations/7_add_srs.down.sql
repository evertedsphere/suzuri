DROP TABLE mnemes;
DROP TABLE mneme_states;
DROP TYPE review_grade;
DROP TYPE memory_status;

ALTER TABLE variants
  DROP COLUMN mneme_id;
