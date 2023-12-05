// @generated automatically by Diesel CLI.

diesel::table! {
    defs (id) {
        id -> Int4,
        dict_name -> Text,
        spelling -> Text,
        reading -> Text,
        content -> Jsonb,
    }
}

diesel::table! {
    lemmas (id) {
        id -> Int4,
        spelling -> Varchar,
        reading -> Varchar,
    }
}

diesel::allow_tables_to_appear_in_same_query!(defs, lemmas,);
