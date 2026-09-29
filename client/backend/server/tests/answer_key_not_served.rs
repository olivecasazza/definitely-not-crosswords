//! The answer key must never reach a client (DEF-243, AC 4).
//!
//! `activeGame.get` used to `SELECT answer` and serialise it, so any caller —
//! including an unauthenticated one, since `get` is public — could read the
//! solution to the puzzle they were playing, and then post every cell right in
//! one call. Authoritative server-side scoring is incompatible with shipping
//! the key, so the letters are gone from the wire.
//!
//! No database: [`active_game::question_view`] is the projection that builds
//! the response and takes a plain struct, so this asserts the actual shape that
//! goes out rather than a copy of it.

use crossword_server::routers::active_game::{question_view, ClueView};
use serde_json::Value;

fn clue() -> ClueView {
    ClueView {
        id: "q1".to_string(),
        kind: "Question".to_string(),
        number: 1,
        // CELL — the length is public, the letters are not.
        len: 4,
        question_text: "A room you can rent".to_string(),
        root_x: 0,
        root_y: 0,
        direction: "ACROSS".to_string(),
        game_id: "g1".to_string(),
    }
}

#[test]
fn the_wire_projection_carries_no_answer_text() {
    let out = question_view(&clue());
    assert!(
        out.get("answer").is_none(),
        "AC 4: activeGame.get must not serialise the answer key; got {}",
        out
    );
    // Belt and braces: no key whose name is even close to the answer.
    for k in out.as_object().expect("the projection is an object").keys() {
        assert_ne!(k, "answer", "the answer key must not be on the wire");
    }
}

#[test]
fn the_wire_projection_still_carries_the_grid_silhouette() {
    // Removing the answer must not break the board: the client lays the grid
    // out from number + root + direction + len, which is what `getStartDetails`
    // already shipped.
    let out = question_view(&clue());
    for field in [
        "id",
        "type",
        "number",
        "len",
        "questionText",
        "rootX",
        "rootY",
        "direction",
        "gameId",
    ] {
        assert!(
            out.get(field).is_some(),
            "the board still needs {field}; projection was {out}"
        );
    }
    assert_eq!(out.get("len").and_then(Value::as_i64), Some(4));
    assert_eq!(out.get("rootX").and_then(Value::as_i64), Some(0));
    assert_eq!(out.get("direction").and_then(Value::as_str), Some("ACROSS"));
}

#[test]
fn no_serialised_value_in_the_projection_holds_the_answer() {
    // The answer must not be smuggled through under another name either. The
    // only strings the projection emits are the ids, the discriminator, the
    // clue text and the direction — none of which is derived from the letters.
    let out = question_view(&clue());
    let mut strings: Vec<&str> = out
        .as_object()
        .expect("the projection is an object")
        .values()
        .filter_map(Value::as_str)
        .collect();
    strings.sort_unstable();
    assert_eq!(
        strings,
        vec!["ACROSS", "A room you can rent", "Question", "g1", "q1"]
            .into_iter()
            .collect::<std::collections::BTreeSet<&str>>()
            .into_iter()
            .collect::<Vec<&str>>()
    );
}
