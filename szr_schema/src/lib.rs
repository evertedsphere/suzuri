// @generated automatically by Diesel CLI.

diesel::table! {
    defs (def_id) {
        def_id -> Int4,
        def_dict_name -> Text,
        def_spelling -> Text,
        def_reading -> Text,
        def_content -> Jsonb,
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
