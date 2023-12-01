// @generated automatically by Diesel CLI.

diesel::table! {
    defs (defs_id) {
        defs_id -> Int4,
        defs_dict_name -> Text,
        defs_spelling -> Text,
        defs_reading -> Text,
        defs_content -> Json,
    }
}

diesel::table! {
    terms (term_id) {
        term_id -> Int4,
        term_spelling -> Varchar,
        term_reading -> Varchar,
        term_data -> Jsonb,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    defs,
    terms,
);
