//! Pure crossword game logic, ported 1:1 from `lib/game/*.ts`.
//!
//! Renderer-agnostic data + math: cells, the per-clue answer map, the 2D board
//! state, and board sizing. No Dioxus, no I/O — so it tests natively. Structs
//! deserialize directly from the backend's tRPC JSON (camelCase), and serde
//! ignores the extra fields the wire carries (`id`, `type`, `activeGameId`, …).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Direction {
    Across,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionType {
    Placeholder,
    CorrectGuess,
    IncorrectGuess,
}

/// One persisted edit to a cell. Mirrors Prisma `GameAction`; only the fields
/// the client reasons about are named, the rest are dropped on deserialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameAction {
    pub action_type: ActionType,
    pub cord_x: i32,
    pub cord_y: i32,
    #[serde(default)]
    pub state: String,
    /// ISO-8601 timestamp. Lexicographic order == chronological order, which is
    /// what the TS `Date` comparison relied on (see [`sort_modifications`]).
    pub submitted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub number: i32,
    pub answer: String,
    pub question_text: String,
    pub root_x: i32,
    pub root_y: i32,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
}

/// A board cell. `cord == (-1, -1)` is the sentinel "no cell here" (block square),
/// matching the TS sentinel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub modifications: Vec<GameAction>,
    pub correct_state: String,
    pub cord_x: i32,
    pub cord_y: i32,
}

impl Cell {
    pub fn is_block(&self) -> bool {
        self.correct_state.is_empty()
    }
}

/// `Question` + its laid-out answer cells. Equivalent of TS
/// `WithComputedProperties<Question>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionWithAnswerMap {
    pub question: Question,
    pub answer_map: Vec<Cell>,
}

/// Latest edit's letter, or "" — TS `GetCurrentCellState`.
pub fn current_cell_state(cell: &Cell) -> &str {
    cell.modifications
        .first()
        .map(|m| m.state.as_str())
        .unwrap_or("")
}

/// TS `IsCellCorrect`: filled, latest edit matches the answer letter.
pub fn is_cell_correct(cell: &Cell) -> bool {
    if cell.correct_state.is_empty() {
        return false;
    }
    let latest = current_cell_state(cell);
    !latest.is_empty() && latest == cell.correct_state
}

/// Newest-first by `submitted_at`. TS sorted descending by `Date`; ISO strings
/// compare the same way, so `modifications[0]` is the latest edit.
fn sort_modifications(mods: &mut [GameAction]) {
    mods.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
}

/// Coordinate of answer letter `index`, walking from the clue root. TS
/// `GetCordAtAnswerIndex`.
pub fn cord_at_answer_index(q: &Question, index: i32) -> Coord {
    match q.direction {
        Direction::Across => Coord {
            x: q.root_x + index,
            y: q.root_y,
        },
        Direction::Down => Coord {
            x: q.root_x,
            y: q.root_y + index,
        },
    }
}

/// Lay a clue's answer onto the grid and attach the matching actions to each
/// cell, newest first. TS `computeQuestionAnswerMap`.
pub fn compute_answer_map(q: &Question, actions: &[GameAction]) -> QuestionWithAnswerMap {
    let mut answer_map: Vec<Cell> = q
        .answer
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let c = cord_at_answer_index(q, i as i32);
            Cell {
                cord_x: c.x,
                cord_y: c.y,
                correct_state: ch.to_string(),
                modifications: Vec::new(),
            }
        })
        .collect();

    for action in actions {
        if let Some(cell) = answer_map
            .iter_mut()
            .find(|c| c.cord_x == action.cord_x && c.cord_y == action.cord_y)
        {
            cell.modifications.push(action.clone());
        }
    }
    for cell in &mut answer_map {
        sort_modifications(&mut cell.modifications);
    }

    QuestionWithAnswerMap {
        question: q.clone(),
        answer_map,
    }
}

/// Bounding board size: max coord over every answer cell, +1 on each axis.
/// TS `GetBoardSize`.
pub fn board_size(questions: &[QuestionWithAnswerMap]) -> Coord {
    let mut size = Coord { x: 0, y: 0 };
    for q in questions {
        for cell in &q.answer_map {
            if cell.cord_x > size.x {
                size.x = cell.cord_x;
            }
            if cell.cord_y > size.y {
                size.y = cell.cord_y;
            }
        }
    }
    size.x += 1;
    size.y += 1;
    size
}

/// Build the dense `size.y × size.x` grid. Cells with no answer get the block
/// sentinel; answer cells get their actions (newest first). TS
/// `BoardState.BoardStateFromActions`.
pub fn board_state_from_actions(
    size: Coord,
    actions: &[GameAction],
    questions: &[QuestionWithAnswerMap],
) -> Vec<Vec<Cell>> {
    let answer_cells: Vec<&Cell> = questions.iter().flat_map(|q| q.answer_map.iter()).collect();

    (0..size.y)
        .map(|yi| {
            (0..size.x)
                .map(|xi| {
                    match answer_cells
                        .iter()
                        .find(|c| c.cord_x == xi && c.cord_y == yi)
                    {
                        None => Cell {
                            cord_x: -1,
                            cord_y: -1,
                            correct_state: String::new(),
                            modifications: Vec::new(),
                        },
                        Some(answer_cell) => {
                            let mut mods: Vec<GameAction> = actions
                                .iter()
                                .filter(|a| a.cord_x == xi && a.cord_y == yi)
                                .cloned()
                                .collect();
                            sort_modifications(&mut mods);
                            Cell {
                                cord_x: answer_cell.cord_x,
                                cord_y: answer_cell.cord_y,
                                correct_state: answer_cell.correct_state.clone(),
                                modifications: mods,
                            }
                        }
                    }
                })
                .collect()
        })
        .collect()
}

/// The whole puzzle is solved when every answer cell is correct. (TS store's
/// `isSolved` getter.)
pub fn is_solved(board: &[Vec<Cell>]) -> bool {
    board
        .iter()
        .flatten()
        .filter(|c| !c.is_block())
        .all(is_cell_correct)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(x: i32, y: i32, letter: &str, at: &str, kind: ActionType) -> GameAction {
        GameAction {
            action_type: kind,
            cord_x: x,
            cord_y: y,
            state: letter.to_string(),
            submitted_at: at.to_string(),
        }
    }

    fn q(answer: &str, x: i32, y: i32, dir: Direction) -> Question {
        Question {
            number: 1,
            answer: answer.to_string(),
            question_text: "clue".into(),
            root_x: x,
            root_y: y,
            direction: dir,
        }
    }

    #[test]
    fn answer_map_lays_letters_across_and_down() {
        let across = compute_answer_map(&q("CAT", 2, 3, Direction::Across), &[]);
        let coords: Vec<_> = across
            .answer_map
            .iter()
            .map(|c| (c.cord_x, c.cord_y))
            .collect();
        assert_eq!(coords, vec![(2, 3), (3, 3), (4, 3)]);

        let down = compute_answer_map(&q("CAT", 2, 3, Direction::Down), &[]);
        let coords: Vec<_> = down
            .answer_map
            .iter()
            .map(|c| (c.cord_x, c.cord_y))
            .collect();
        assert_eq!(coords, vec![(2, 3), (2, 4), (2, 5)]);
    }

    #[test]
    fn modifications_are_newest_first() {
        // two edits to the same cell; the later timestamp must end up first.
        let actions = vec![
            act(2, 3, "X", "2026-01-01T00:00:00Z", ActionType::Placeholder),
            act(2, 3, "C", "2026-01-02T00:00:00Z", ActionType::CorrectGuess),
        ];
        let m = compute_answer_map(&q("CAT", 2, 3, Direction::Across), &actions);
        assert_eq!(current_cell_state(&m.answer_map[0]), "C");
    }

    #[test]
    fn cell_correctness() {
        let mut m = compute_answer_map(
            &q("CAT", 0, 0, Direction::Across),
            &[act(
                0,
                0,
                "C",
                "2026-01-01T00:00:00Z",
                ActionType::CorrectGuess,
            )],
        );
        assert!(is_cell_correct(&m.answer_map[0])); // "C" == "C"
        assert!(!is_cell_correct(&m.answer_map[1])); // unfilled
                                                     // a block (empty correct_state) is never "correct"
        m.answer_map[0].correct_state = String::new();
        assert!(!is_cell_correct(&m.answer_map[0]));
    }

    #[test]
    fn board_size_and_sentinels() {
        // CAT across at (0,0) and CAR down at (0,0) share the C.
        let qs = vec![
            compute_answer_map(&q("CAT", 0, 0, Direction::Across), &[]),
            compute_answer_map(&q("CAR", 0, 0, Direction::Down), &[]),
        ];
        let size = board_size(&qs);
        assert_eq!((size.x, size.y), (3, 3)); // max coord 2 +1

        let board = board_state_from_actions(size, &[], &qs);
        assert_eq!(board.len(), 3);
        assert_eq!(board[0].len(), 3);
        // (0,0) is a real cell; (2,2) is a block (no answer covers it).
        assert!(!board[0][0].is_block());
        assert!(board[2][2].is_block());
    }

    #[test]
    fn solved_detection() {
        let qs = vec![compute_answer_map(
            &q("HI", 0, 0, Direction::Across),
            &[
                act(0, 0, "H", "2026-01-01T00:00:00Z", ActionType::CorrectGuess),
                act(1, 0, "I", "2026-01-01T00:00:00Z", ActionType::CorrectGuess),
            ],
        )];
        let size = board_size(&qs);
        let actions: Vec<GameAction> = qs
            .iter()
            .flat_map(|q| q.answer_map.iter())
            .flat_map(|c| c.modifications.clone())
            .collect();
        let board = board_state_from_actions(size, &actions, &qs);
        assert!(is_solved(&board));
    }

    #[test]
    fn deserializes_backend_json() {
        // shape as it arrives from tRPC: camelCase, extra fields ignored.
        let action: GameAction = serde_json::from_str(
            r#"{"id":"a1","type":"GameAction","activeGameId":"g1","userId":"u1",
                "actionType":"correctGuess","cordX":2,"cordY":3,"state":"C",
                "previousState":"","submittedAt":"2026-01-02T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(action.action_type, ActionType::CorrectGuess);
        assert_eq!((action.cord_x, action.cord_y), (2, 3));

        let question: Question = serde_json::from_str(
            r#"{"id":"q1","number":1,"answer":"CAT","questionText":"feline",
                "rootX":2,"rootY":3,"direction":"ACROSS","gameId":"g1"}"#,
        )
        .unwrap();
        assert_eq!(question.direction, Direction::Across);
        assert_eq!(question.answer, "CAT");
    }
}
