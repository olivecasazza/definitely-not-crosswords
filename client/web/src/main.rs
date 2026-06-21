//! Crossword web client — first vertical slice.
//!
//! Renders the three play surfaces (board, active clue, clue list) as panel-kit
//! panels, driven entirely by the ported `crossword_core` logic. The game here
//! is a hardcoded fixture; wiring it to the live tRPC backend is the next slice
//! (the wire format is already built + tested in `crossword_core::rpc`; the
//! renderer just needs to add the fetch via gloo-net).

use crossword_core::game::{
    board_size, board_state_from_actions, compute_answer_map, current_cell_state, Cell, Direction,
    Question, QuestionWithAnswerMap,
};
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin, CSS};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

const STORAGE_KEY: &str = "crossword_layout";

/// Crossword-specific chrome, layered after `panel_kit::CSS`. Reuses panel-kit's
/// theme variables (`--bg`, `--fg`, `--line2`, `--dim`) so it retheming for free.
const GAME_CSS: &str = "
.board { display: grid; gap: 2px; width: 100%; aspect-ratio: var(--ar, 1);
  background: var(--line2); border: 2px solid var(--line2); }
.cell { position: relative; background: var(--bg); display: flex; align-items: center;
  justify-content: center; font-family: var(--mono); font-weight: 600; font-size: 1.1rem;
  text-transform: uppercase; user-select: none; }
.cell.block { background: var(--line2); }
.cell.sel { background: color-mix(in srgb, var(--accent, #e6b800) 30%, var(--bg)); }
.cell.cur { background: var(--accent, #e6b800); color: #111; }
.cell .num { position: absolute; top: 1px; left: 2px; font-size: .5rem; font-weight: 400;
  color: var(--dim); }
.clue { padding: .3rem .4rem; border-radius: 3px; cursor: pointer; color: var(--fg); }
.clue:hover { background: var(--line); }
.clue.active { background: color-mix(in srgb, var(--accent, #e6b800) 25%, var(--bg)); }
.clue .n { color: var(--dim); margin-right: .4rem; font-family: var(--mono); }
.clue-head { font-family: var(--mono); font-size: .7rem; color: var(--dim);
  text-transform: uppercase; letter-spacing: .05em; margin: .2rem 0; }
.letters { display: flex; gap: 3px; margin-top: .5rem; }
.letters .box { width: 2rem; height: 2rem; border: 1px solid var(--line2); border-radius: 3px;
  display: flex; align-items: center; justify-content: center; font-family: var(--mono);
  text-transform: uppercase; }
";

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Board,
    Clue,
    Clues,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Board => "Board",
            Panel::Clue => "Active Clue",
            Panel::Clues => "Clues",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Board, 16.0, 16.0, 440.0, 480.0),
        b.at(Panel::Clue, 472.0, 16.0, 360.0, 220.0),
        b.at(Panel::Clues, 472.0, 252.0, 360.0, 244.0),
    ]
}

/// Hardcoded fixture puzzle (a clean 4×4 with consistent crossings) standing in
/// for `activeGame.get` until the RPC client lands.
fn fixture() -> Vec<Question> {
    let q = |number, answer: &str, root_x, root_y, direction| Question {
        number,
        answer: answer.to_string(),
        question_text: String::new(),
        root_x,
        root_y,
        direction,
    };
    vec![
        Question {
            question_text: "Memory-safe systems language".into(),
            ..q(1, "RUST", 0, 0, Direction::Across)
        },
        Question {
            question_text: "Competition of speed".into(),
            ..q(1, "RACE", 0, 0, Direction::Down)
        },
        Question {
            question_text: "Ocean's rise and fall".into(),
            ..q(2, "TIDE", 3, 0, Direction::Down)
        },
        Question {
            question_text: "What programmers write".into(),
            ..q(3, "CODE", 0, 2, Direction::Across)
        },
    ]
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Compute the puzzle once. No actions yet (no edits persisted). Read-only
    // and shared across closures, so it lives behind an `Rc`.
    let questions: Rc<Vec<QuestionWithAnswerMap>> = use_hook(|| {
        Rc::new(
            fixture()
                .iter()
                .map(|q| compute_answer_map(q, &[]))
                .collect(),
        )
    });
    let size = use_hook(|| board_size(&questions));
    let board: Rc<Vec<Vec<Cell>>> =
        use_hook(|| Rc::new(board_state_from_actions(size, &[], &questions)));

    let selected = use_signal(|| Option::<usize>::None);
    let ws = use_workspace(STORAGE_KEY, default_layout);

    // Coords covered by the selected clue, for board highlighting.
    let sel_coords = use_memo({
        let questions = questions.clone();
        move || -> Vec<(i32, i32)> {
            selected()
                .map(|i| {
                    questions[i]
                        .answer_map
                        .iter()
                        .map(|c| (c.cord_x, c.cord_y))
                        .collect()
                })
                .unwrap_or_default()
        }
    });

    let body = move |kind: Panel, _maximized: bool| -> Element {
        match kind {
            Panel::Board => render_board(&board, size, &questions, sel_coords),
            Panel::Clue => render_active_clue(&questions, selected),
            Panel::Clues => render_clue_list(&questions, selected),
        }
    };

    rsx! {
        style { {CSS} }
        style { {GAME_CSS} }
        div {
            class: ws.root_class(),
            tabindex: "0",
            onmousemove: move |e| ws.handle_mouse_move(&e),
            onmouseup: move |_| ws.handle_mouse_up(),
            header { class: "topbar",
                h1 { "definitely-not-crosswords" }
                span { class: "hint", "drag panels · click a clue to select" }
            }
            {ws.render(body)}
            {ws.dock()}
        }
    }
}

fn render_board(
    board: &[Vec<Cell>],
    size: crossword_core::game::Coord,
    questions: &[QuestionWithAnswerMap],
    sel_coords: Memo<Vec<(i32, i32)>>,
) -> Element {
    let cols = size.x;
    let ar = size.x as f64 / size.y as f64;
    rsx! {
        div {
            class: "board",
            style: "grid-template-columns: repeat({cols}, 1fr); --ar: {ar};",
            for (y, row) in board.iter().enumerate() {
                for (x, cell) in row.iter().enumerate() {
                    if cell.is_block() {
                        div { class: "cell block" }
                    } else {
                        {
                            let (x, y) = (x as i32, y as i32);
                            let num = clue_number_at(questions, x, y);
                            let selected = sel_coords.read().contains(&(x, y));
                            let class = if selected { "cell sel" } else { "cell" };
                            let letter = current_cell_state(cell).to_string();
                            rsx! {
                                div { class: "{class}",
                                    if let Some(n) = num {
                                        span { class: "num", "{n}" }
                                    }
                                    "{letter}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Clue number to render in a cell: the number of a clue whose root is here.
fn clue_number_at(questions: &[QuestionWithAnswerMap], x: i32, y: i32) -> Option<i32> {
    questions
        .iter()
        .find(|q| q.question.root_x == x && q.question.root_y == y)
        .map(|q| q.question.number)
}

fn render_active_clue(
    questions: &[QuestionWithAnswerMap],
    selected: Signal<Option<usize>>,
) -> Element {
    match selected() {
        None => rsx! { p { class: "clue-head", "Select a clue" } },
        Some(i) => {
            let q = &questions[i].question;
            let dir = match q.direction {
                Direction::Across => "Across",
                Direction::Down => "Down",
            };
            rsx! {
                p { class: "clue-head", "Clue {q.number} · {dir} · {q.answer.len()} letters" }
                p { "{q.question_text}" }
                div { class: "letters",
                    for _ in 0..q.answer.len() {
                        div { class: "box" }
                    }
                }
            }
        }
    }
}

fn render_clue_list(
    questions: &[QuestionWithAnswerMap],
    selected: Signal<Option<usize>>,
) -> Element {
    let section = move |dir: Direction, label: &'static str| {
        let mut selected = selected;
        rsx! {
            p { class: "clue-head", "{label}" }
            for (i, q) in questions.iter().enumerate() {
                if q.question.direction == dir {
                    {
                        let active = selected() == Some(i);
                        let class = if active { "clue active" } else { "clue" };
                        let (n, text) = (q.question.number, q.question.question_text.clone());
                        rsx! {
                            div { class: "{class}", onclick: move |_| selected.set(Some(i)),
                                span { class: "n", "{n}" }
                                "{text}"
                            }
                        }
                    }
                }
            }
        }
    };
    rsx! {
        {section(Direction::Across, "Across")}
        {section(Direction::Down, "Down")}
    }
}
