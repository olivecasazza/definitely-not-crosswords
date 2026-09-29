//! Authoritative grid scoring — DEF-243.
//!
//! `activeGame.addActions` used to persist whatever `actionType` the client
//! declared, and `activeGame.complete` scored a member by *counting* the
//! `correctGuess` rows it found. A caller that never solved anything could
//! POST N fabricated `{"actionType":"correctGuess","cordX":0,"cordY":0,"state":"X"}`
//! actions and land an arbitrary score on the public leaderboard. No answer key
//! was required and no solving was required.
//!
//! The fix inverts the trust: the client sends *letters*, the server decides
//! what they mean. Every classification here is against the stored
//! `Question.answer` key, and every path that reads or writes a score goes
//! through it. Nothing in this module trusts a client-declared `actionType`.
//!
//! Kept free of sqlx/axum so the property that matters — "a fabricated
//! correctGuess cannot score" — is unit-testable without a database.

use std::collections::HashMap;

/// The stored answer key, laid onto the grid: every letter cell of a game
/// mapped to the letter that belongs there.
pub type AnswerKey = HashMap<(i32, i32), char>;

/// One clue's geometry plus its answer, as the `Question` table stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clue {
    pub root_x: i32,
    pub root_y: i32,
    /// `"ACROSS"` or `"DOWN"`, matching `direction::text`.
    pub direction: String,
    pub answer: String,
}

/// Build the key by walking every clue's answer from its root.
///
/// A crossing cell is owned by two clues. The generator guarantees they agree;
/// if they do not, the first clue in the order given wins, so the result is
/// deterministic for a given input rather than dependent on SQL row order.
/// Callers pass clues ordered by `number ASC`.
pub fn answer_key(clues: &[Clue]) -> AnswerKey {
    let mut key = AnswerKey::new();
    for clue in clues {
        for (i, ch) in clue.answer.chars().enumerate() {
            let (x, y) = if clue.direction == "ACROSS" {
                (clue.root_x + i as i32, clue.root_y)
            } else {
                (clue.root_x, clue.root_y + i as i32)
            };
            key.entry((x, y)).or_insert(ch);
        }
    }
    key
}

/// What the server decided a submitted letter *means*. The client does not get
/// a vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The submitted letter is the answer at that cord.
    Correct,
    /// A real letter, but not the answer at that cord.
    Incorrect,
    /// Not a letter that can score: an erase, or a cord the grid does not cover.
    Placeholder,
}

impl Verdict {
    /// The `GameActionTypeEnum` label persisted for this verdict.
    pub fn as_action_type(self) -> &'static str {
        match self {
            Verdict::Correct => "correctGuess",
            Verdict::Incorrect => "incorrectGuess",
            Verdict::Placeholder => "placeholder",
        }
    }

    /// Read a stored `actionType` back. `None` for a value the enum does not
    /// know — which cannot be written by this module, so it means the row came
    /// from somewhere else and must not be scored.
    pub fn from_action_type(text: &str) -> Option<Verdict> {
        match text {
            "correctGuess" => Some(Verdict::Correct),
            "incorrectGuess" => Some(Verdict::Incorrect),
            "placeholder" => Some(Verdict::Placeholder),
            _ => None,
        }
    }
}

/// Decide what one submitted letter means, against the key.
///
/// An empty submission is an erase, not a wrong answer: it must not score and
/// must not cost a -2. A cord the key does not cover is not a cell, so it is
/// stored as a placeholder — the action log stays faithful — but can never
/// contribute to a score.
pub fn classify(state: &str, expected: Option<&char>) -> Verdict {
    if state.is_empty() {
        return Verdict::Placeholder;
    }
    match (state.chars().next(), expected) {
        (Some(_), None) => Verdict::Placeholder,
        (Some(got), Some(want)) => {
            if got.eq_ignore_ascii_case(want) {
                Verdict::Correct
            } else {
                Verdict::Incorrect
            }
        }
        (None, _) => Verdict::Placeholder,
    }
}

/// One validated submission. `previous_state` is filled in by the server from
/// what the cell actually held, never from the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    pub x: i32,
    pub y: i32,
    pub state: String,
    pub verdict: Verdict,
}

/// Validate one element of the `actions` array against the key.
///
/// Returns a client-safe reason naming the offending field. The message
/// deliberately says nothing about the key: a caller probing for the answer
/// must not be able to tell a "wrong letter" refusal from a "not a cell"
/// refusal.
pub fn validate_submission(
    action: &serde_json::Value,
    index: usize,
    key: &AnswerKey,
) -> Result<Submission, String> {
    let at = |field: &str| format!("actions[{index}].{field}");

    let x = action
        .get("cordX")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("{} must be an integer", at("cordX")))?;
    let y = action
        .get("cordY")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("{} must be an integer", at("cordY")))?;

    let state = action
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{} must be a string", at("state")))?;

    // A cell holds one letter. A longer string is not a letter and is refused
    // rather than truncated, so the stored log cannot be padded with junk.
    if state.chars().count() > 1 {
        return Err(format!("{} must be a single letter", at("state")));
    }

    let expected = key.get(&(x as i32, y as i32));
    if expected.is_none() {
        return Err(format!(
            "actions[{index}] targets a cell that is not part of the grid"
        ));
    }

    Ok(Submission {
        x: x as i32,
        y: y as i32,
        state: state.to_string(),
        verdict: classify(state, expected),
    })
}

/// The grid's fill state, computed from the key and the latest verdict in each
/// cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Completion {
    pub filled: i64,
    pub correct: i64,
    pub total: i64,
}

impl Completion {
    /// A grid with no cells cannot be solved, so it is never complete. This is
    /// what stops `complete` minting a CompletedGame for an empty grid.
    pub fn is_solved(&self) -> bool {
        self.total > 0 && self.correct == self.total
    }

    /// The client-facing reason a grid is not yet solved. Names the numbers so
    /// a player can see how far along they are.
    pub fn unsolved_reason(&self) -> String {
        format!(
            "grid is not solved: {}/{} cells correct, {} of {} filled",
            self.correct, self.total, self.filled, self.total
        )
    }
}

/// Fold the latest verdict per cell into a [`Completion`].
pub fn completion(key: &AnswerKey, latest: &HashMap<(i32, i32), Verdict>) -> Completion {
    let mut c = Completion {
        filled: 0,
        correct: 0,
        total: key.len() as i64,
    };
    for cell in key.keys() {
        match latest.get(cell) {
            Some(Verdict::Correct) => {
                c.filled += 1;
                c.correct += 1;
            }
            Some(Verdict::Incorrect) => {
                c.filled += 1;
            }
            _ => {}
        }
    }
    c
}

/// One player's standing on the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberProgress {
    pub correct: i32,
    pub incorrect: i32,
}

/// Points per cell. A correct cell is worth 10, a wrong one costs 2, floored
/// at 0 — the same arithmetic the old row counter used, now bounded by the grid.
pub const CORRECT_POINTS: i32 = 10;
pub const INCORRECT_POINTS: i32 = 2;

impl MemberProgress {
    pub fn score(&self) -> i32 {
        (self.correct * CORRECT_POINTS - self.incorrect * INCORRECT_POINTS).max(0)
    }
}

/// Reduce one player's action log to their standing, counting each cell once.
///
/// `actions` is the raw log in ascending `(submittedAt, id)` order — the same
/// order the read path in `game_list.rs` uses to pick a cell's latest state —
/// and the *last* verdict seen for a cell is the one that stands.
///
/// Counting rows instead of cells is the second half of the original bug: a
/// client that posts the same right letter a thousand times would otherwise
/// mint a thousand points. Bounding the score by the grid is the whole point.
pub fn member_progress(actions: &[(i32, i32, Verdict)]) -> MemberProgress {
    let mut latest: HashMap<(i32, i32), Verdict> = HashMap::new();
    for (x, y, verdict) in actions {
        latest.insert((*x, *y), *verdict);
    }
    let mut p = MemberProgress {
        correct: 0,
        incorrect: 0,
    };
    for verdict in latest.values() {
        match verdict {
            Verdict::Correct => p.correct += 1,
            Verdict::Incorrect => p.incorrect += 1,
            Verdict::Placeholder => {}
        }
    }
    p
}

/// The end-to-end shape of a `complete` call, in the terms the router uses.
///
/// Kept here so the AC-2/AC-3 property can be stated once and checked end to
/// end: whatever a caller posts, the score that lands comes from the verdicts
/// this module derived, never from a client-declared `actionType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionAttempt {
    pub member_id: String,
    pub score: i32,
    pub correct: i32,
    pub incorrect: i32,
}

/// One member's slice of the action log, as `complete` receives it.
pub type MemberLog = (String, Vec<(i32, i32, Verdict)>);

/// Decide a `complete` call against the grid as it stands.
///
/// `Err` is the AC-2 refusal and its message is what the client sees, so it
/// names the numbers rather than just saying "unsolved". `Ok` is the attempt to
/// record.
pub fn settle_completion(
    key: &AnswerKey,
    latest: &HashMap<(i32, i32), Verdict>,
    members: &[MemberLog],
) -> Result<Vec<CompletionAttempt>, String> {
    let c = completion(key, latest);
    if !c.is_solved() {
        return Err(c.unsolved_reason());
    }
    Ok(members
        .iter()
        .map(|(member_id, log)| {
            let p = member_progress(log);
            CompletionAttempt {
                member_id: member_id.clone(),
                score: p.score(),
                correct: p.correct,
                incorrect: p.incorrect,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn clue(answer: &str, x: i32, y: i32, dir: &str) -> Clue {
        Clue {
            root_x: x,
            root_y: y,
            direction: dir.to_string(),
            answer: answer.to_string(),
        }
    }

    /// CAT across at (0,0) and CAR down at (0,0) share the C.
    fn crossing_key() -> AnswerKey {
        answer_key(&[clue("CAT", 0, 0, "ACROSS"), clue("CAR", 0, 0, "DOWN")])
    }

    #[test]
    fn the_key_lays_letters_across_and_down() {
        let key = crossing_key();
        assert_eq!(key.get(&(0, 0)), Some(&'C'));
        assert_eq!(key.get(&(1, 0)), Some(&'A'));
        assert_eq!(key.get(&(2, 0)), Some(&'T'));
        assert_eq!(key.get(&(0, 1)), Some(&'A'));
        assert_eq!(key.get(&(0, 2)), Some(&'R'));
        // 5 distinct cells: the crossing counts once.
        assert_eq!(key.len(), 5);
    }

    #[test]
    fn a_right_letter_is_correct() {
        let key = crossing_key();
        assert_eq!(classify("C", key.get(&(0, 0))), Verdict::Correct);
        // Case-insensitive, matching the old client-side comparison.
        assert_eq!(classify("c", key.get(&(0, 0))), Verdict::Correct);
    }

    #[test]
    fn a_wrong_letter_is_incorrect() {
        let key = crossing_key();
        assert_eq!(classify("X", key.get(&(0, 0))), Verdict::Incorrect);
    }

    #[test]
    fn an_erase_is_a_placeholder_not_a_wrong_answer() {
        let key = crossing_key();
        assert_eq!(classify("", key.get(&(0, 0))), Verdict::Placeholder);
    }

    #[test]
    fn a_cord_off_the_grid_cannot_score() {
        let key = crossing_key();
        // (9, 9) is not part of this grid.
        assert_eq!(classify("C", key.get(&(9, 9))), Verdict::Placeholder);
    }

    #[test]
    fn a_fabricated_correct_guess_with_a_wrong_letter_scores_zero() {
        // The DEF-243 attack, in miniature: the client declares
        // `correctGuess` but sends a letter that is not the answer. The server
        // reclassifies it, so the row that lands is an incorrectGuess and the
        // member's score is 0.
        let key = crossing_key();
        let v = classify("X", key.get(&(0, 0)));
        assert_eq!(v, Verdict::Incorrect);
        assert_eq!(member_progress(&[(0, 0, v)]).score(), 0);
    }

    #[test]
    fn a_fabricated_correct_guess_with_an_empty_state_scores_zero() {
        let key = crossing_key();
        let v = classify("", key.get(&(0, 0)));
        assert_eq!(v, Verdict::Placeholder);
        assert_eq!(member_progress(&[(0, 0, v)]).score(), 0);
    }

    #[test]
    fn a_fabricated_correct_guess_at_a_cord_off_the_grid_scores_zero() {
        let key = crossing_key();
        let v = classify("X", key.get(&(9, 9)));
        assert_eq!(v, Verdict::Placeholder);
        assert_eq!(member_progress(&[(9, 9, v)]).score(), 0);
    }

    #[test]
    fn posting_the_same_right_letter_a_thousand_times_scores_once() {
        let key = crossing_key();
        let v = classify("C", key.get(&(0, 0)));
        assert_eq!(v, Verdict::Correct);
        let log: Vec<(i32, i32, Verdict)> = (0..1000).map(|_| (0, 0, v)).collect();
        let p = member_progress(&log);
        assert_eq!(p.correct, 1);
        assert_eq!(p.score(), CORRECT_POINTS);
    }

    #[test]
    fn a_later_erase_unscores_a_cell() {
        let log = vec![(0, 0, Verdict::Correct), (0, 0, Verdict::Placeholder)];
        let p = member_progress(&log);
        assert_eq!(p.correct, 0);
        assert_eq!(p.score(), 0);
    }

    #[test]
    fn a_later_wrong_guess_overwrites_a_right_one() {
        let log = vec![(0, 0, Verdict::Correct), (0, 0, Verdict::Incorrect)];
        let p = member_progress(&log);
        assert_eq!((p.correct, p.incorrect), (0, 1));
        assert_eq!(p.score(), 0);
    }

    #[test]
    fn an_empty_grid_is_never_solved() {
        let key = AnswerKey::new();
        let c = completion(&key, &HashMap::new());
        assert_eq!(c.total, 0);
        assert!(!c.is_solved());
        assert_eq!(
            c.unsolved_reason(),
            "grid is not solved: 0/0 cells correct, 0 of 0 filled"
        );
    }

    #[test]
    fn a_partially_filled_grid_is_not_solved() {
        let key = crossing_key();
        let mut latest = HashMap::new();
        latest.insert((0, 0), Verdict::Correct);
        latest.insert((1, 0), Verdict::Incorrect);
        let c = completion(&key, &latest);
        assert_eq!((c.filled, c.correct, c.total), (2, 1, 5));
        assert!(!c.is_solved());
    }

    #[test]
    fn a_fully_correct_grid_is_solved() {
        let key = crossing_key();
        let latest: HashMap<(i32, i32), Verdict> =
            key.keys().map(|cell| (*cell, Verdict::Correct)).collect();
        let c = completion(&key, &latest);
        assert_eq!((c.filled, c.correct, c.total), (5, 5, 5));
        assert!(c.is_solved());
    }

    #[test]
    fn a_filled_but_wrong_grid_is_not_solved() {
        let key = crossing_key();
        let latest: HashMap<(i32, i32), Verdict> =
            key.keys().map(|cell| (*cell, Verdict::Incorrect)).collect();
        let c = completion(&key, &latest);
        assert_eq!((c.filled, c.correct, c.total), (5, 0, 5));
        assert!(!c.is_solved());
    }

    #[test]
    fn the_unsolved_reason_names_the_numbers() {
        let key = crossing_key();
        let mut latest = HashMap::new();
        latest.insert((0, 0), Verdict::Correct);
        let c = completion(&key, &latest);
        assert_eq!(
            c.unsolved_reason(),
            "grid is not solved: 1/5 cells correct, 1 of 5 filled"
        );
    }

    #[test]
    fn a_missing_cord_is_refused_not_defaulted_to_zero() {
        let key = crossing_key();
        let err = validate_submission(&json!({ "state": "C" }), 0, &key)
            .expect_err("a missing cord must be refused");
        assert_eq!(err, "actions[0].cordX must be an integer");
    }

    #[test]
    fn a_non_integer_cord_is_refused() {
        let key = crossing_key();
        let err = validate_submission(&json!({ "cordX": "0", "cordY": 0, "state": "C" }), 0, &key)
            .expect_err("a string cord must be refused");
        assert_eq!(err, "actions[0].cordX must be an integer");
    }

    #[test]
    fn a_missing_state_is_refused() {
        let key = crossing_key();
        let err = validate_submission(&json!({ "cordX": 0, "cordY": 0 }), 0, &key)
            .expect_err("a missing state must be refused");
        assert_eq!(err, "actions[0].state must be a string");
    }

    #[test]
    fn a_multi_letter_state_is_refused() {
        let key = crossing_key();
        let err = validate_submission(&json!({ "cordX": 0, "cordY": 0, "state": "CAT" }), 0, &key)
            .expect_err("a cell holds one letter");
        assert_eq!(err, "actions[0].state must be a single letter");
    }

    #[test]
    fn a_cord_off_the_grid_is_refused() {
        let key = crossing_key();
        let err = validate_submission(&json!({ "cordX": 9, "cordY": 9, "state": "C" }), 0, &key)
            .expect_err("an off-grid cord must be refused");
        assert_eq!(
            err,
            "actions[0] targets a cell that is not part of the grid"
        );
    }

    #[test]
    fn a_valid_submission_carries_the_servers_verdict() {
        let key = crossing_key();
        let s = validate_submission(
            &json!({ "cordX": 0, "cordY": 0, "state": "X", "actionType": "correctGuess" }),
            0,
            &key,
        )
        .expect("a well-formed action is accepted");
        // The client's declared `actionType` is ignored entirely.
        assert_eq!(s.verdict, Verdict::Incorrect);
        assert_eq!(s.verdict.as_action_type(), "incorrectGuess");
    }

    #[test]
    fn the_verdict_round_trips_through_the_stored_enum() {
        for v in [Verdict::Correct, Verdict::Incorrect, Verdict::Placeholder] {
            assert_eq!(Verdict::from_action_type(v.as_action_type()), Some(v));
        }
        // A value this module cannot write must not be scored.
        assert_eq!(Verdict::from_action_type("correct_guess"), None);
        assert_eq!(Verdict::from_action_type(""), None);
    }

    // --- the end-to-end property AC-3 asks for ---------------------------
    //
    // Simulate the attack exactly as a caller would drive it: post fabricated
    // `correctGuess` actions at cord (0,0) with letters that are not the answer,
    // then call `complete`. Nothing here consults the declared actionType.

    /// A key with a single 4-letter ACROSS clue: CELL at (0,0)..(3,0).
    fn one_clue_key() -> AnswerKey {
        answer_key(&[clue("CELL", 0, 0, "ACROSS")])
    }

    /// Run a caller's submitted batch through validation + classification and
    /// return the verdicts that would be persisted.
    fn verdicts_for(batch: &[serde_json::Value], key: &AnswerKey) -> Vec<(i32, i32, Verdict)> {
        batch
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let s = validate_submission(a, i, key).expect("the batch is well formed");
                (s.x, s.y, s.verdict)
            })
            .collect()
    }

    /// Fold a log into the per-cell latest map, the way the router does.
    fn latest_of(log: &[(i32, i32, Verdict)]) -> HashMap<(i32, i32), Verdict> {
        log.iter().map(|(x, y, v)| ((*x, *y), *v)).collect()
    }

    #[test]
    fn a_fabricated_leaderboard_score_does_not_survive_complete() {
        let key = one_clue_key();

        // The attack: 50 declared-correct guesses with letters that are not
        // the answer, all aimed at cord (0,0).
        let batch: Vec<serde_json::Value> = (0..50)
            .map(|_| {
                json!({
                    "actionType": "correctGuess",
                    "cordX": 0,
                    "cordY": 0,
                    "state": "X",
                    "previousState": "",
                })
            })
            .collect();
        let log = verdicts_for(&batch, &key);
        assert!(
            log.iter().all(|(_, _, v)| *v == Verdict::Incorrect),
            "the server must reclassify every fabricated correctGuess"
        );

        // `complete` is called with the grid untouched otherwise: unsolved, so
        // it is refused and no score is minted at all.
        let latest: HashMap<(i32, i32), Verdict> = HashMap::new();
        let members = vec![("attacker".to_string(), log)];
        let err = settle_completion(&key, &latest, &members)
            .expect_err("an unsolved grid must not be completable");
        assert_eq!(err, "grid is not solved: 0/4 cells correct, 0 of 4 filled");
    }

    #[test]
    fn a_fabricated_score_is_zero_even_on_a_solved_grid() {
        // The subtler version: the attacker solves the board for real, but
        // fabricates thousands of correctGuesses on top hoping the row counter
        // multiplies them. Every cell is counted once, so the extra posts are
        // worth nothing.
        let key = one_clue_key();

        let real: Vec<serde_json::Value> = ["C", "E", "L", "L"]
            .iter()
            .enumerate()
            .map(|(i, ch)| json!({ "cordX": i as i64, "cordY": 0, "state": ch }))
            .collect();
        let mut log = verdicts_for(&real, &key);

        // 5,000 fabricated correctGuesses on cell (0,0), submitted after the
        // real one, so they are the latest verdict for that cell — and they
        // are wrong, so that cell is now wrong too.
        let noise: Vec<serde_json::Value> = (0..5000)
            .map(|_| json!({ "actionType": "correctGuess", "cordX": 0, "cordY": 0, "state": "Q" }))
            .collect();
        log.extend(verdicts_for(&noise, &key));

        // The grid is now wrong on (0,0), so complete refuses: the noise did
        // not even earn the attacker a completed game.
        let latest = latest_of(&log);
        let err = settle_completion(&key, &latest, &[("attacker".into(), log.clone())])
            .expect_err("the last verdict for (0,0) is wrong");
        assert_eq!(err, "grid is not solved: 3/4 cells correct, 4 of 4 filled");

        // And on a genuinely solved grid the same flood is worth exactly the
        // four cells that were actually answered.
        let mut clean = verdicts_for(&real, &key);
        clean.extend(
            (0..5000)
                .filter(|i| *i % 2 == 0)
                .map(|_| (0, 0, Verdict::Correct)),
        );
        let clean_latest = latest_of(&clean);
        let settled = settle_completion(&key, &clean_latest, &[("honest".into(), clean.clone())])
            .expect("the four real letters solve the board");
        assert_eq!(settled.len(), 1);
        assert_eq!(settled[0].correct, 4);
        assert_eq!(settled[0].score, 40, "four cells, not 5,003 posts");
    }

    #[test]
    fn an_empty_grid_is_never_completable() {
        // AC-2: `complete` on a grid nobody touched must not return a game id.
        let key = one_clue_key();
        let err = settle_completion(&key, &HashMap::new(), &[("a".into(), vec![])])
            .expect_err("an empty grid is not a solved grid");
        assert_eq!(err, "grid is not solved: 0/4 cells correct, 0 of 4 filled");
    }

    #[test]
    fn a_game_with_no_clues_is_never_completable() {
        // A published game whose questions are all missing has a key of size 0.
        // `is_solved` requires total > 0 precisely so this cannot pass.
        let key = AnswerKey::new();
        let err = settle_completion(&key, &HashMap::new(), &[("a".into(), vec![])])
            .expect_err("a grid with no cells is not solved");
        assert_eq!(err, "grid is not solved: 0/0 cells correct, 0 of 0 filled");
    }

    #[test]
    fn a_solved_grid_settles_one_row_per_member() {
        let key = one_clue_key();
        let real: Vec<serde_json::Value> = ["C", "E", "L", "L"]
            .iter()
            .enumerate()
            .map(|(i, ch)| json!({ "cordX": i as i64, "cordY": 0, "state": ch }))
            .collect();

        // Two members split the work; only the first got all four right.
        let a = verdicts_for(&real, &key);
        let b: Vec<(i32, i32, Verdict)> =
            vec![(0, 0, Verdict::Correct), (1, 0, Verdict::Incorrect)];
        let latest = latest_of(&a);

        let settled =
            settle_completion(&key, &latest, &[("a".to_string(), a), ("b".to_string(), b)])
                .expect("the board is solved");
        assert_eq!(settled.len(), 2);
        assert_eq!((settled[0].correct, settled[0].score), (4, 40));
        assert_eq!((settled[1].correct, settled[1].incorrect), (1, 1));
        assert_eq!(settled[1].score, 8); // 10 - 2
    }

    #[test]
    fn a_member_who_typed_nothing_scores_zero_on_a_solved_grid() {
        let key = one_clue_key();
        let real: Vec<serde_json::Value> = ["C", "E", "L", "L"]
            .iter()
            .enumerate()
            .map(|(i, ch)| json!({ "cordX": i as i64, "cordY": 0, "state": ch }))
            .collect();
        let a = verdicts_for(&real, &key);
        let latest = latest_of(&a);
        let settled = settle_completion(&key, &latest, &[("lurker".into(), vec![])])
            .expect("the board is solved");
        assert_eq!(settled[0].score, 0);
    }

    // --- AC 5: raw database errors must not reach the client ----------------

    #[test]
    fn a_database_failure_reaches_the_client_as_a_stable_message() {
        // The old code was `.map_err(|e| e.to_string())` throughout, which
        // handed callers raw Postgres text: table and column names, constraint
        // names, sometimes the values that collided.
        let err = sqlx::Error::RowNotFound;
        let msg = crate::ctx::sanitised_db_error("save the submitted letters", &err);
        assert_eq!(msg, "save the submitted letters failed");
        // Nothing from the driver survives into the message.
        assert!(!msg.contains("RowNotFound"), "driver detail leaked: {msg}");
        assert!(
            !msg.to_lowercase().contains("sqlx"),
            "driver detail leaked: {msg}"
        );
    }
}
