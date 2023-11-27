// @generated automatically by Diesel CLI.

diesel::table! {
    terms (id) {
        id -> Int4,
        spelling -> Varchar,
        reading -> Varchar,
    }
}
