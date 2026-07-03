//! Crossword gameplay screen — Rust/Dioxus port of the Nuxt `pages/game/[id]`.
//!
//! Ports the Pinia `activeGame` store + `GameBoard`/`ActiveClueCard`/`QuestionsList`
//! Vue components into a single panel-kit workspace with three panels: Board,
//! Active Clue, and Clues. All board math comes from `crossword_core::game`.
//!
//! State model (per advisor): `questions` + `actions` are the source of truth;
//! `answer_maps`/`board_size`/`board` are derived via `use_memo`. The live
//! subscription only pushes into `actions`, so everything recomputes for free.
//! Selection snapshots a mutable `game_action_data` (the in-progress word).

use crossword_core::game::{
    board_size as compute_board_size, board_state_from_actions, compute_answer_map, is_solved,
    ActionType, Cell, Direction, GameAction, Question, QuestionWithAnswerMap,
};
use dioxus::prelude::*;
use serde_json::{json, Value};
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

use crate::net;
use crate::Route;

/// Identity key for a question (number + direction; numbers are reused across
/// across/down so direction is part of the key).
type QKey = (i32, Direction);

fn qkey(q: &Question) -> QKey {
    (q.number, q.direction)
}

fn dir_str(d: Direction) -> &'static str {
    match d {
        Direction::Across => "ACROSS",
        Direction::Down => "DOWN",
    }
}

fn action_type_str(a: ActionType) -> &'static str {
    match a {
        ActionType::Placeholder => "placeholder",
        ActionType::CorrectGuess => "correctGuess",
        ActionType::IncorrectGuess => "incorrectGuess",
    }
}

/// One in-progress letter slot for the selected word (mirror of the TS
/// `gameActionData` entries). `state` is the currently-typed letter.
#[derive(Clone, PartialEq)]
struct ActionSlot {
    cord_x: i32,
    cord_y: i32,
    previous_state: String,
    state: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
enum PanelId {
    Board,
    Clue,
    Clues,
}

impl panel_kit::PanelKind for PanelId {
    fn title(self) -> &'static str {
        match self {
            PanelId::Board => "Board",
            PanelId::Clue => "Active Clue",
            PanelId::Clues => "Clues",
        }
    }
}

fn default_layout() -> Vec<panel_kit::PanelWin<PanelId>> {
    let mut b = panel_kit::LayoutBuilder::new();
    vec![
        b.at(PanelId::Board, 16.0, 16.0, 640.0, 640.0),
        b.at(PanelId::Clue, 16.0, 672.0, 640.0, 292.0),
        b.at(PanelId::Clues, 672.0, 16.0, 1232.0, 948.0),
    ]
}

#[component]
pub fn GamePlay(id: String) -> Element {
    // --- source-of-truth state ---
    let questions = use_signal(Vec::<Question>::new);
    let mut actions = use_signal(Vec::<GameAction>::new);
    let loading = use_signal(|| true);
    let load_error = use_signal(|| Option::<String>::None);

    // selection / input state
    let mut selected = use_signal(|| Option::<QKey>::None);
    let mut selected_direction = use_signal(|| Option::<Direction>::Some(Direction::Across));
    let mut game_action_data = use_signal(Vec::<ActionSlot>::new);
    let mut focused_index = use_signal(|| Option::<usize>::None);

    // per-letter input mount handles, so we can drive focus without web-sys.
    let mut input_refs = use_signal(Vec::<Option<Rc<MountedData>>>::new);

    // keep subscription handles alive for the component lifetime.
    let _subs = use_signal(|| Option::<(net::Subscription, net::Subscription)>::None);

    let id_for_load = id.clone();

    // --- load + subscribe (once) ---
    use_hook(move || {
        let mut questions = questions;
        let mut actions = actions;
        let mut loading = loading;
        let mut load_error = load_error;
        let mut subs = _subs;
        let id = id_for_load.clone();
        spawn_local(async move {
            match net::query("activeGame.get", Some(json!({ "id": id }))).await {
                Ok(Value::Null) => {
                    load_error.set(Some("Game not found".into()));
                    loading.set(false);
                }
                Ok(data) => {
                    let qs: Vec<Question> = data
                        .get("game")
                        .and_then(|g| g.get("questions"))
                        .and_then(|q| serde_json::from_value(q.clone()).ok())
                        .unwrap_or_default();
                    let acts: Vec<GameAction> = data
                        .get("actions")
                        .and_then(|a| serde_json::from_value(a.clone()).ok())
                        .unwrap_or_default();
                    questions.set(qs);
                    actions.set(acts);
                    loading.set(false);
                }
                Err(e) => {
                    load_error.set(Some(e));
                    loading.set(false);
                }
            }
        });

        // Subscriptions: both emitters are global (no input). We filter by
        // activeGameId off the raw JSON before merging / navigating.
        let nav = navigator();
        let id_actions = id_for_load.clone();
        let on_actions = net::subscribe("activeGame.onAddActions", None, move |data: Value| {
            // payload is an array of GameAction; each carries activeGameId.
            let arr = match data.as_array() {
                Some(a) => a,
                None => return,
            };
            let mut incoming: Vec<GameAction> = Vec::new();
            for v in arr {
                let belongs = v
                    .get("activeGameId")
                    .and_then(|x| x.as_str())
                    .map(|s| s == id_actions)
                    .unwrap_or(true);
                if !belongs {
                    continue;
                }
                if let Ok(a) = serde_json::from_value::<GameAction>(v.clone()) {
                    incoming.push(a);
                }
            }
            if !incoming.is_empty() {
                let mut cur = actions.peek().clone();
                cur.extend(incoming);
                actions.set(cur);
            }
        });

        let id_done = id_for_load.clone();
        let on_done = net::subscribe("activeGame.onGameCompleted", None, move |data: Value| {
            let active = data.get("activeGameId").and_then(|x| x.as_str());
            let completed = data.get("completedGameId").and_then(|x| x.as_str());
            if active == Some(id_done.as_str()) {
                if let Some(cid) = completed {
                    nav.push(Route::GameCompleted {
                        id: cid.to_string(),
                    });
                }
            }
        });

        subs.set(Some((on_actions, on_done)));
    });

    // --- derived state (recomputes when questions/actions change) ---
    let answer_maps = use_memo(move || {
        let acts = actions.read();
        let maps: Vec<QuestionWithAnswerMap> = questions
            .read()
            .iter()
            .map(|q| compute_answer_map(q, &acts))
            .collect();
        Rc::new(maps)
    });

    let board = use_memo(move || {
        let maps = answer_maps.read().clone();
        let size = compute_board_size(&maps);
        let acts = actions.read();
        let grid = board_state_from_actions(size, &acts, &maps);
        Rc::new((size, grid))
    });

    // filtered (across/down/all) clue list, by the toggle
    let filtered: Vec<QuestionWithAnswerMap> = {
        let maps = answer_maps.read();
        let dir = *selected_direction.read();
        maps.iter()
            .filter(|m| dir.map(|d| m.question.direction == d).unwrap_or(true))
            .cloned()
            .collect()
    };

    // --- focus driver: focus the input matching focused_index ---
    use_effect(move || {
        let idx = *focused_index.read();
        let refs = input_refs.read();
        if let Some(i) = idx {
            if let Some(Some(node)) = refs.get(i) {
                let node = node.clone();
                spawn_local(async move {
                    let _ = node.set_focus(true).await;
                });
            }
        }
    });

    // --- selection helpers ---------------------------------------------------

    // Snapshot the in-progress word for a question, pre-filling current letters.
    let mut select_question = move |key: QKey| {
        let maps = answer_maps.peek();
        if let Some(m) = maps.iter().find(|m| qkey(&m.question) == key) {
            let slots: Vec<ActionSlot> = m
                .answer_map
                .iter()
                .map(|cell| {
                    let cur = cell
                        .modifications
                        .first()
                        .map(|md| md.state.clone())
                        .unwrap_or_default();
                    ActionSlot {
                        cord_x: cell.cord_x,
                        cord_y: cell.cord_y,
                        previous_state: cur.clone(),
                        state: cur,
                    }
                })
                .collect();
            let n = slots.len();
            selected_direction.set(Some(m.question.direction));
            game_action_data.set(slots);
            input_refs.set(vec![None; n]);
            selected.set(Some(key));
            focused_index.set(Some(0));
        }
    };

    // Submit the current word as a placeholder save (used by unselect).
    let id_for_save = id.clone();
    let mut submit_placeholder = move || {
        let slots = game_action_data.peek().clone();
        if slots.is_empty() {
            return;
        }
        let payload_actions: Vec<Value> = slots
            .iter()
            .map(|s| {
                json!({
                    "activeGameId": id_for_save,
                    "cordX": s.cord_x,
                    "cordY": s.cord_y,
                    "actionType": "placeholder",
                    "previousState": s.previous_state,
                    "state": s.state,
                })
            })
            .collect();
        let input = json!({ "id": id_for_save, "actions": payload_actions });
        // optimistic local merge so the board updates immediately
        let now = js_now_iso();
        let local: Vec<GameAction> = slots
            .iter()
            .map(|s| GameAction {
                action_type: ActionType::Placeholder,
                cord_x: s.cord_x,
                cord_y: s.cord_y,
                state: s.state.clone(),
                submitted_at: now.clone(),
            })
            .collect();
        let mut cur = actions.peek().clone();
        cur.extend(local);
        actions.set(cur);
        spawn_local(async move {
            let _ = net::mutation("activeGame.addActions", Some(input)).await;
        });
    };

    let mut submit_placeholder_for_unselect = submit_placeholder.clone();
    let unselect = move |_| {
        let any_typed = game_action_data.peek().iter().any(|s| !s.state.is_empty());
        if selected.peek().is_some() && any_typed {
            submit_placeholder_for_unselect();
        }
        selected.set(None);
        game_action_data.set(Vec::new());
        focused_index.set(None);
    };

    // Click a board cell → select a covering question (current dir first).
    let select_coordinates = move |x: i32, y: i32| {
        let maps = answer_maps.peek();
        let dir = *selected_direction.peek();
        let covers =
            |m: &QuestionWithAnswerMap| m.answer_map.iter().any(|c| c.cord_x == x && c.cord_y == y);
        let found = maps
            .iter()
            .find(|m| dir.map(|d| m.question.direction == d).unwrap_or(false) && covers(m))
            .or_else(|| maps.iter().find(|m| covers(m)));
        if let Some(m) = found {
            let key = qkey(&m.question);
            select_question(key);
        }
    };

    // Direction toggles (click active → null = show all).
    let toggle_dir = move |d: Direction| {
        let cur = *selected_direction.peek();
        if cur == Some(d) {
            selected_direction.set(None);
        } else {
            // unselect (saving progress) then set the new filter
            let any_typed = game_action_data.peek().iter().any(|s| !s.state.is_empty());
            if selected.peek().is_some() && any_typed {
                submit_placeholder();
            }
            selected.set(None);
            game_action_data.set(Vec::new());
            focused_index.set(None);
            selected_direction.set(Some(d));
        }
    };

    // --- guess submission ----------------------------------------------------
    let id_for_guess = id.clone();
    let submit_guess = move || {
        let slots = game_action_data.peek().clone();
        if slots.is_empty() {
            return;
        }
        // word-level correctness: whole answer must match.
        let key = match *selected.peek() {
            Some(k) => k,
            None => return,
        };
        let maps = answer_maps.peek();
        let m = match maps.iter().find(|m| qkey(&m.question) == key) {
            Some(m) => m.clone(),
            None => return,
        };
        drop(maps);
        let is_correct = m.answer_map.iter().enumerate().all(|(i, cell)| {
            slots
                .get(i)
                .map(|s| s.state.eq_ignore_ascii_case(&cell.correct_state))
                .unwrap_or(false)
        });
        let at = if is_correct {
            ActionType::CorrectGuess
        } else {
            ActionType::IncorrectGuess
        };
        let at_str = action_type_str(at);

        let payload_actions: Vec<Value> = slots
            .iter()
            .map(|s| {
                json!({
                    "activeGameId": id_for_guess,
                    "cordX": s.cord_x,
                    "cordY": s.cord_y,
                    "actionType": at_str,
                    "previousState": s.previous_state,
                    "state": s.state,
                })
            })
            .collect();
        let add_input = json!({ "id": id_for_guess, "actions": payload_actions });

        // Build the new action set locally for an inline solved-check.
        let now = js_now_iso();
        let new_local: Vec<GameAction> = slots
            .iter()
            .map(|s| GameAction {
                action_type: at,
                cord_x: s.cord_x,
                cord_y: s.cord_y,
                state: s.state.clone(),
                submitted_at: now.clone(),
            })
            .collect();
        let mut next_actions = actions.peek().clone();
        next_actions.extend(new_local);

        // recompute board inline (memo would be stale until next tick)
        let maps2: Vec<QuestionWithAnswerMap> = questions
            .peek()
            .iter()
            .map(|q| compute_answer_map(q, &next_actions))
            .collect();
        let size = compute_board_size(&maps2);
        let grid = board_state_from_actions(size, &next_actions, &maps2);
        let solved = is_solved(&grid);

        actions.set(next_actions);

        if is_correct {
            selected.set(None);
            game_action_data.set(Vec::new());
            focused_index.set(None);
        }

        let id_complete = id_for_guess.clone();
        let nav = navigator();
        spawn_local(async move {
            let _ = net::mutation("activeGame.addActions", Some(add_input)).await;
            if is_correct && solved {
                if let Ok(res) =
                    net::mutation("activeGame.complete", Some(json!({ "id": id_complete }))).await
                {
                    if let Some(cid) = res.get("id").and_then(|x| x.as_str()) {
                        nav.push(Route::GameCompleted {
                            id: cid.to_string(),
                        });
                    }
                }
            }
        });
    };

    // --- keyboard handling for the active clue (input auto-advance, etc.) -----
    let handle_letter_input = move |index: usize, raw: String| {
        let val = raw
            .chars()
            .last()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default();
        {
            let mut g = game_action_data.write();
            if let Some(slot) = g.get_mut(index) {
                slot.state = val.clone();
            }
        }
        let len = game_action_data.peek().len();
        if !val.is_empty() && index + 1 < len {
            focused_index.set(Some(index + 1));
        }
    };

    let handle_key = move |index: usize, key: Key| match key {
        Key::Backspace => {
            let cur_empty = game_action_data
                .peek()
                .get(index)
                .map(|s| s.state.is_empty())
                .unwrap_or(true);
            if cur_empty && index > 0 {
                {
                    let mut g = game_action_data.write();
                    if let Some(s) = g.get_mut(index - 1) {
                        s.state = String::new();
                    }
                }
                focused_index.set(Some(index - 1));
            } else {
                let mut g = game_action_data.write();
                if let Some(s) = g.get_mut(index) {
                    s.state = String::new();
                }
            }
        }
        Key::ArrowLeft if index > 0 => focused_index.set(Some(index - 1)),
        Key::ArrowRight => {
            let len = game_action_data.peek().len();
            if index + 1 < len {
                focused_index.set(Some(index + 1));
            }
        }
        _ => {}
    };

    // ------------------------------------------------------------------------
    if *loading.read() {
        return rsx! {
            div { class: "container", p { class: "muted", "Loading game…" } }
        };
    }
    if let Some(e) = load_error.read().clone() {
        return rsx! {
            div { class: "container",
                h1 { "Couldn't load game" }
                p { class: "muted", "{e}" }
                Link { to: Route::Games {}, class: "app-btn", "Back to games" }
            }
        };
    }

    let ws = use_workspace_local();
    crate::store::sync_panel_mode(ws.mode);

    // selected question, looked up fresh for the clue panel render
    let selected_q: Option<QuestionWithAnswerMap> = selected.read().and_then(|k| {
        answer_maps
            .read()
            .iter()
            .find(|m| qkey(&m.question) == k)
            .cloned()
    });

    let board_data = board.read().clone();
    let (size, grid) = (*board_data).clone();

    let body = move |kind: PanelId, _max: bool| -> Element {
        match kind {
            PanelId::Board => render_board(
                &grid,
                size,
                &questions.read(),
                &selected_q,
                &game_action_data.read(),
                *focused_index.read(),
                select_coordinates.clone(),
            ),
            PanelId::Clue => render_clue(
                &selected_q,
                &game_action_data.read(),
                *focused_index.read(),
                input_refs,
                handle_letter_input.clone(),
                handle_key.clone(),
                unselect.clone(),
                submit_guess.clone(),
            ),
            PanelId::Clues => render_clues(
                &filtered,
                *selected.read(),
                *selected_direction.read(),
                &game_action_data.read(),
                select_question.clone(),
                toggle_dir.clone(),
            ),
        }
    };

    rsx! {
        style { {GAME_CSS} }
        div {
            class: ws.root_class(),
            tabindex: "0",
            onmousemove: move |e| ws.handle_mouse_move(&e),
            onmouseup: move |_| ws.handle_mouse_up(),
            {ws.render(body)}
            {ws.dock()}
        }
    }
}

/// `use_workspace` with a stable storage key for this screen.
fn use_workspace_local() -> panel_kit::Workspace<PanelId> {
    panel_kit::use_workspace("crossword_game_play", default_layout)
}

// ---------------------------------------------------------------------------
// rendering helpers
// ---------------------------------------------------------------------------

fn cell_number(questions: &[Question], cell: &Cell) -> Option<i32> {
    questions
        .iter()
        .find(|q| q.root_x == cell.cord_x && q.root_y == cell.cord_y)
        .map(|q| q.number)
}

fn render_board(
    grid: &[Vec<Cell>],
    size: crossword_core::game::Coord,
    questions: &[Question],
    selected_q: &Option<QuestionWithAnswerMap>,
    slots: &[ActionSlot],
    focused_index: Option<usize>,
    select_coordinates: impl FnMut(i32, i32) + Clone + 'static,
) -> Element {
    let cols = size.x.max(1);
    let rows = size.y.max(1);
    let style = format!(
        "grid-template-columns: repeat({cols}, 1fr); grid-template-rows: repeat({rows}, 1fr); aspect-ratio: {} / {};",
        cols, rows
    );

    // focused coord (the cell currently being typed in)
    let focused_coord: Option<(i32, i32)> = match (selected_q, focused_index) {
        (Some(m), Some(i)) => m.answer_map.get(i).map(|c| (c.cord_x, c.cord_y)),
        _ => None,
    };
    let is_in_selected = |x: i32, y: i32| -> bool {
        selected_q
            .as_ref()
            .map(|m| m.answer_map.iter().any(|c| c.cord_x == x && c.cord_y == y))
            .unwrap_or(false)
    };
    let typed_at = |x: i32, y: i32| -> String {
        slots
            .iter()
            .find(|s| s.cord_x == x && s.cord_y == y)
            .map(|s| s.state.clone())
            .unwrap_or_default()
    };

    let flat: Vec<Cell> = grid.iter().flatten().cloned().collect();

    rsx! {
        div { class: "cw-board-wrap",
            div { class: "cw-board", style: "{style}",
                for cell in flat {
                    {
                        let is_letter = !cell.is_block();
                        let x = cell.cord_x;
                        let y = cell.cord_y;
                        if !is_letter {
                            rsx! { div { class: "cw-cell cw-block" } }
                        } else {
                            let selected = is_in_selected(x, y);
                            let focused = focused_coord == Some((x, y));
                            let num = cell_number(questions, &cell);
                            let action_type = cell.modifications.first().map(|m| m.action_type);
                            let mut classes = String::from("cw-cell cw-letter");
                            if focused {
                                classes.push_str(" cw-focused");
                            } else if selected {
                                classes.push_str(" cw-selected");
                            } else {
                                match action_type {
                                    Some(ActionType::Placeholder) => classes.push_str(" cw-placeholder"),
                                    Some(ActionType::IncorrectGuess) => classes.push_str(" cw-incorrect"),
                                    Some(ActionType::CorrectGuess) => classes.push_str(" cw-correct"),
                                    None => {}
                                }
                            }
                            let display = if selected {
                                typed_at(x, y)
                            } else {
                                cell.modifications.first().map(|m| m.state.clone()).unwrap_or_default()
                            };
                            let mut sc = select_coordinates.clone();
                            rsx! {
                                div {
                                    class: "{classes}",
                                    onclick: move |_| sc(x, y),
                                    if let Some(n) = num {
                                        span { class: "cw-num", "{n}" }
                                    }
                                    span { class: "cw-char", "{display}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_clue(
    selected_q: &Option<QuestionWithAnswerMap>,
    slots: &[ActionSlot],
    focused_index: Option<usize>,
    mut input_refs: Signal<Vec<Option<Rc<MountedData>>>>,
    handle_letter_input: impl FnMut(usize, String) + Clone + 'static,
    handle_key: impl FnMut(usize, Key) + Clone + 'static,
    mut unselect: impl FnMut(Event<MouseData>) + Clone + 'static,
    submit_guess: impl FnMut() + Clone + 'static,
) -> Element {
    let q = match selected_q {
        Some(q) => q,
        None => {
            return rsx! {
                div { class: "cw-clue-empty",
                    h3 { "Ready to solve?" }
                    p { class: "muted",
                        "Tap a square on the board or pick a clue from the list to start typing."
                    }
                }
            };
        }
    };
    let dir = dir_str(q.question.direction);
    let len = q.answer_map.len();
    let mut unselect2 = unselect.clone();
    let mut submit2 = submit_guess.clone();

    rsx! {
        div { class: "cw-clue",
            div { class: "cw-clue-head",
                span { class: "cw-dir-badge cw-dir-{dir.to_lowercase()}", "{dir}" }
                span { class: "muted", "CLUE {q.question.number} · {len} LETTERS" }
                button { class: "cw-link-btn", onclick: move |e| unselect(e), "ESC to clear" }
            }
            div { class: "cw-clue-text", "{q.question.question_text}" }
            div { class: "cw-letters",
                for (index , slot) in slots.iter().cloned().enumerate() {
                    {
                        let focused = focused_index == Some(index);
                        let mut cls = String::from("cw-letter-input");
                        if focused {
                            cls.push_str(" cw-input-focused");
                        }
                        let mut hi = handle_letter_input.clone();
                        let mut hk = handle_key.clone();
                        rsx! {
                            input {
                                key: "{slot.cord_x}-{slot.cord_y}",
                                class: "{cls}",
                                r#type: "text",
                                maxlength: "1",
                                autocomplete: "off",
                                spellcheck: "false",
                                value: "{slot.state}",
                                onmounted: move |e: Event<MountedData>| {
                                    let mut refs = input_refs.write();
                                    if index < refs.len() {
                                        refs[index] = Some(e.data());
                                    }
                                },
                                oninput: move |e| hi(index, e.value()),
                                onkeydown: move |e| hk(index, e.key()),
                            }
                        }
                    }
                }
            }
            div { class: "cw-clue-actions",
                button { class: "cw-btn-cancel", onclick: move |e| unselect2(e), "Cancel" }
                button { class: "cw-btn-guess", onclick: move |_| submit2(), "Guess" }
            }
        }
    }
}

fn render_clues(
    filtered: &[QuestionWithAnswerMap],
    selected: Option<QKey>,
    selected_direction: Option<Direction>,
    slots: &[ActionSlot],
    select_question: impl FnMut(QKey) + Clone + 'static,
    mut toggle_dir: impl FnMut(Direction) + Clone + 'static,
) -> Element {
    let mut toggle_a = toggle_dir.clone();
    let across_active = selected_direction == Some(Direction::Across);
    let down_active = selected_direction == Some(Direction::Down);

    // live letter state for a clue's bubble: prefer in-progress slots when selected
    let bubble_state = |key: QKey, cell: &Cell| -> (String, &'static str) {
        if selected == Some(key) {
            if let Some(s) = slots
                .iter()
                .find(|s| s.cord_x == cell.cord_x && s.cord_y == cell.cord_y)
            {
                let at = cell
                    .modifications
                    .first()
                    .map(|m| action_type_str(m.action_type))
                    .unwrap_or("placeholder");
                return (s.state.clone(), if s.state.is_empty() { "" } else { at });
            }
        }
        match cell.modifications.first() {
            Some(m) => (m.state.clone(), action_type_str(m.action_type)),
            None => (String::new(), ""),
        }
    };

    rsx! {
        div { class: "cw-clues",
            div { class: "cw-clues-head",
                h2 { "Clues" }
                div { class: "cw-tabs",
                    button {
                        class: if across_active { "cw-tab cw-tab-active-across" } else { "cw-tab" },
                        onclick: move |_| toggle_a(Direction::Across),
                        "Across"
                    }
                    button {
                        class: if down_active { "cw-tab cw-tab-active-down" } else { "cw-tab" },
                        onclick: move |_| toggle_dir(Direction::Down),
                        "Down"
                    }
                }
            }
            div { class: "cw-clue-list",
                for m in filtered.iter().cloned() {
                    {
                        let key = qkey(&m.question);
                        let is_sel = selected == Some(key);
                        let mut sq = select_question.clone();
                        let row_cls = if is_sel { "cw-clue-row cw-clue-row-sel" } else { "cw-clue-row" };
                        rsx! {
                            div {
                                class: "{row_cls}",
                                onclick: move |_| sq(key),
                                div { class: "cw-clue-badge", "{m.question.number}" }
                                div { class: "cw-clue-body",
                                    div { class: "cw-clue-row-text", "{m.question.question_text}" }
                                    div { class: "cw-bubbles",
                                        for cell in m.answer_map.iter() {
                                            {
                                                let (letter, at) = bubble_state(key, cell);
                                                let mut bcls = String::from("cw-bubble");
                                                if letter.is_empty() {
                                                    bcls.push_str(" cw-bubble-empty");
                                                } else {
                                                    match at {
                                                        "incorrectGuess" => bcls.push_str(" cw-incorrect"),
                                                        "correctGuess" => bcls.push_str(" cw-correct"),
                                                        _ => bcls.push_str(" cw-placeholder"),
                                                    }
                                                }
                                                rsx! {
                                                    div { class: "{bcls}", "{letter}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Timestamp for an optimistic local action. `sort_modifications` orders
/// newest-first by lexicographic `submitted_at`. We can't pull `js-sys`/`Date`
/// (Cargo.toml is locked), so we synthesize a key that (a) sorts ABOVE any real
/// server timestamp — every real ts starts with a year digit < '9' — and (b)
/// strictly increases per call via an atomic counter, so a later optimistic
/// action always beats an earlier one. Without the counter, equal-key ties fall
/// back to stable-insertion order, which surfaces the OLDER edit and makes a
/// corrected wrong guess uncompletable. wasm is single-threaded; Relaxed is fine.
fn js_now_iso() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("9999-{:020}", SEQ.fetch_add(1, Ordering::Relaxed))
}

// ---------------------------------------------------------------------------

const GAME_CSS: &str = r#"
.cw-board-wrap { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; padding: 8px; box-sizing: border-box; }
.cw-board { display: grid; gap: 3px; width: 100%; max-width: min(100%, 480px); }
.cw-cell { position: relative; aspect-ratio: 1 / 1; border-radius: 0; display: flex; align-items: center; justify-content: center; font-weight: 700; text-transform: uppercase; user-select: none; font-size: clamp(10px, 2.4vw, 20px); }
.cw-block { background: var(--bg-cell-empty); border: 1px solid rgba(39,39,42,0.25); opacity: 0.4; }
.cw-letter { background: var(--bg-cell-letter); color: var(--text-primary); border: 1px solid var(--border-app); cursor: pointer; transition: all .12s ease; }
.cw-letter:hover { border-color: var(--border-hover); }
.cw-focused { background: var(--pastel-yellow); color: #18181b; border: 1px solid var(--pastel-yellow); transform: scale(1.05); z-index: 2; }
.cw-selected { background: rgba(254,234,153,0.18); color: var(--text-primary); border: 1px solid var(--pastel-yellow); }
.cw-placeholder { border: 2px solid var(--pastel-yellow); }
.cw-incorrect { background: rgba(255,140,140,0.15); color: var(--pastel-red); border: 1px solid var(--pastel-red); }
.cw-correct { background: rgba(168,230,207,0.15); color: var(--pastel-green); border: 1px solid var(--pastel-green); }
.cw-num { position: absolute; top: 2px; left: 3px; font-size: clamp(6px, 1.2vw, 9px); line-height: 1; color: var(--text-secondary); opacity: 0.85; font-weight: 700; pointer-events: none; }
.cw-char { pointer-events: none; }

.cw-clue { display: flex; flex-direction: column; gap: 12px; height: 100%; }
.cw-clue-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; height: 100%; gap: 6px; }
.cw-clue-head { display: flex; align-items: center; gap: 8px; border-bottom: 1px solid var(--border-app); padding-bottom: 8px; flex-wrap: wrap; }
.cw-dir-badge { font-family: var(--font-sans); font-size: var(--fs-2xs); font-weight: 600; letter-spacing: 0.1em; padding: 2px 6px; border-radius: 0; border: 1px solid; }
.cw-dir-across { background: rgba(254,234,153,0.1); color: var(--pastel-yellow); border-color: rgba(254,234,153,0.2); }
.cw-dir-down { background: rgba(168,230,207,0.1); color: var(--pastel-green); border-color: rgba(168,230,207,0.2); }
.cw-link-btn { margin-left: auto; background: none; border: none; color: var(--text-secondary); font-size: 11px; cursor: pointer; }
.cw-link-btn:hover { color: var(--text-primary); }
.cw-clue-text { font-size: 15px; font-weight: 500; line-height: 1.5; color: var(--text-primary); }
.cw-letters { display: flex; flex-wrap: wrap; gap: 8px; justify-content: center; padding: 4px 0; }
.cw-letter-input { width: 40px; height: 40px; text-align: center; font-size: 18px; font-weight: 700; text-transform: uppercase; border-radius: 0; border: 1px solid var(--border-app); background: var(--bg-card); color: var(--text-primary); }
.cw-letter-input:hover { border-color: var(--border-hover); }
.cw-input-focused { border-color: var(--pastel-yellow); box-shadow: 0 0 12px rgba(254,234,153,0.25); }
.cw-clue-actions { display: flex; justify-content: flex-end; gap: 12px; margin-top: auto; }
.cw-btn-cancel { font-family: var(--font-sans); padding: 8px 16px; font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; border-radius: 0; border: 1px solid var(--border-app); background: var(--bg-card); color: var(--text-secondary); cursor: pointer; }
.cw-btn-cancel:hover { color: var(--text-primary); border-color: var(--border-hover); }
.cw-btn-guess { font-family: var(--font-sans); padding: 8px 20px; font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; border-radius: 0; border: 1px solid var(--pastel-yellow); background: var(--pastel-yellow); color: var(--contrast-ink); cursor: pointer; }

.cw-clues { display: flex; flex-direction: column; gap: 10px; height: 100%; }
.cw-clues-head { display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border-app); padding-bottom: 8px; }
.cw-clues-head h2 { font-size: 14px; color: var(--text-secondary); margin: 0; }
.cw-tabs { display: flex; gap: 4px; }
.cw-tab { font-family: var(--font-sans); padding: 4px 12px; font-size: var(--fs-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; border-radius: 0; border: 1px solid var(--border-app); background: transparent; color: var(--text-secondary); cursor: pointer; }
.cw-tab:hover { border-color: var(--border-hover); }
.cw-tab-active-across { background: var(--pastel-yellow); color: #18181b; border-color: var(--pastel-yellow); font-weight: 700; }
.cw-tab-active-down { background: var(--pastel-green); color: #18181b; border-color: var(--pastel-green); font-weight: 700; }
.cw-clue-list { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; padding-right: 4px; }
.cw-clue-row { display: flex; gap: 10px; padding: 10px; border-radius: 0; border: 1px solid var(--border-app); cursor: pointer; }
.cw-clue-row:hover { border-color: var(--border-hover); }
.cw-clue-row-sel { background: rgba(254,234,153,0.04); border-color: rgba(254,234,153,0.4); }
.cw-clue-badge { width: 28px; height: 28px; flex-shrink: 0; border-radius: 0; display: flex; align-items: center; justify-content: center; font-weight: 700; font-size: var(--fs-md); background: var(--bg-cell-empty); color: var(--text-secondary); border: 1px solid var(--border-app); }
.cw-clue-body { display: flex; flex-direction: column; gap: 8px; width: 100%; }
.cw-clue-row-text { font-size: 13px; color: var(--text-secondary); line-height: 1.4; }
.cw-bubbles { display: flex; flex-wrap: wrap; gap: 4px; }
.cw-bubble { width: 20px; height: 20px; border-radius: 0; display: flex; align-items: center; justify-content: center; font-size: 10px; font-weight: 700; text-transform: uppercase; }
.cw-bubble-empty { background: var(--bg-cell-empty); border: 1px solid var(--border-app); opacity: 0.3; }
.cw-bubble.cw-placeholder { background: var(--bg-cell-letter); color: var(--text-primary); border: 1px solid var(--pastel-yellow); }
.cw-bubble.cw-incorrect { background: rgba(255,140,140,0.18); color: var(--pastel-red); border: 1px solid var(--pastel-red); }
.cw-bubble.cw-correct { background: rgba(168,230,207,0.18); color: var(--pastel-green); border: 1px solid var(--pastel-green); }
"#;
