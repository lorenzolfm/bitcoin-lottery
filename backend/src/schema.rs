// @generated automatically by Diesel CLI.

diesel::table! {
    bets (id) {
        id -> Integer,
        round_id -> Integer,
        name -> Text,
        guess -> BigInt,
        created_at -> Timestamp,
    }
}

diesel::table! {
    rounds (id) {
        id -> Integer,
        title -> Text,
        status -> Text,
        target_height -> Nullable<Integer>,
        block_hash -> Nullable<Text>,
        nonce -> Nullable<BigInt>,
        created_at -> Timestamp,
        closed_at -> Nullable<Timestamp>,
        settled_at -> Nullable<Timestamp>,
    }
}

diesel::joinable!(bets -> rounds (round_id));

diesel::allow_tables_to_appear_in_same_query!(bets, rounds,);
