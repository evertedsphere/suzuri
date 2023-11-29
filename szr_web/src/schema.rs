// @generated automatically by Diesel CLI.

diesel::table! {
    terms (term_id) {
        term_id -> Int4,
        term_spelling -> Varchar,
        term_reading -> Varchar,
        term_data -> Jsonb,
    }
}
