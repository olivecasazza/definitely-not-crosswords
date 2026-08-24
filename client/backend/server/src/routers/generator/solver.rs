//! Pure crossword solver — a faithful port of the grid-generation half of
//! server/services/crossword/generateCrossword.ts (everything except the ONNX
//! embedding, which lives in `embed.rs`). No async, no DB: takes a scored
//! `Dictionary` and emits a valid, numbered crossword. Determinism is per-seed
//! (LCG RNG) but NOT byte-equivalent to the TS output — different embeddings
//! rank words differently, which is expected (see Phase D notes).

use super::dict::Dictionary;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Across,
    Down,
}
impl Direction {
    fn opposite(self) -> Direction {
        match self {
            Direction::Across => Direction::Down,
            Direction::Down => Direction::Across,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Across => "ACROSS",
            Direction::Down => "DOWN",
        }
    }
}

#[derive(Clone)]
pub struct Placed {
    pub word: String,
    pub dir: Direction,
    pub x: i32,
    pub y: i32,
}

pub struct Params {
    pub width: i32,
    pub height: i32,
    pub min_len: i32,
    pub max_len: i32,
    pub target: i32,
    pub runs: i32,
    pub max_attempts: i32,
}

pub struct Best {
    pub grid: Grid,
    pub placed: Vec<Placed>,
    pub score: f64,
    pub seed: u32,
}

pub type Grid = Vec<Vec<Option<u8>>>;

/// LCG matching the TS `createRng`: state = state*1664525 + 1013904223 (mod 2^32).
struct Rng {
    state: u32,
}
impl Rng {
    fn new(seed: u32) -> Self {
        Rng { state: seed }
    }
    fn next(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        self.state as f64 / 4_294_967_296.0
    }
}

fn empty_grid(p: &Params) -> Grid {
    vec![vec![None; p.width as usize]; p.height as usize]
}

fn cell_at(w: &Placed, i: i32) -> (i32, i32) {
    match w.dir {
        Direction::Across => (w.x + i, w.y),
        Direction::Down => (w.x, w.y + i),
    }
}

fn in_bounds(x: i32, y: i32, p: &Params) -> bool {
    x >= 0 && x < p.width && y >= 0 && y < p.height
}

fn letter_at(grid: &Grid, x: i32, y: i32) -> Option<u8> {
    if y < 0 || x < 0 {
        return None;
    }
    let (yu, xu) = (y as usize, x as usize);
    if yu >= grid.len() || xu >= grid[0].len() {
        return None;
    }
    grid[yu][xu]
}

/// (has_across, has_down) directions of placed words covering (x,y).
fn occupied_dirs(placed: &[Placed], x: i32, y: i32) -> (bool, bool) {
    let (mut a, mut d) = (false, false);
    for pw in placed {
        for i in 0..pw.word.len() as i32 {
            let (cx, cy) = cell_at(pw, i);
            if cx == x && cy == y {
                match pw.dir {
                    Direction::Across => a = true,
                    Direction::Down => d = true,
                }
            }
        }
    }
    (a, d)
}

fn can_place(grid: &Grid, placed: &[Placed], c: &Placed, p: &Params) -> bool {
    let wb = c.word.as_bytes();
    let len = wb.len() as i32;
    let (dx, dy) = match c.dir {
        Direction::Across => (1, 0),
        Direction::Down => (0, 1),
    };
    if letter_at(grid, c.x - dx, c.y - dy).is_some()
        || letter_at(grid, c.x + dx * len, c.y + dy * len).is_some()
    {
        return false;
    }

    let mut crossings = 0;
    for i in 0..len {
        let x = c.x + dx * i;
        let y = c.y + dy * i;
        if !in_bounds(x, y, p) {
            return false;
        }
        let ch = wb[i as usize];
        match grid[y as usize][x as usize] {
            Some(e) if e != ch => return false,
            Some(_) => {
                let (a, d) = occupied_dirs(placed, x, y);
                let same = match c.dir {
                    Direction::Across => a,
                    Direction::Down => d,
                };
                if same {
                    return false;
                }
                crossings += 1;
            }
            None => {
                let blocked = if c.dir == Direction::Across {
                    letter_at(grid, x, y - 1).is_some() || letter_at(grid, x, y + 1).is_some()
                } else {
                    letter_at(grid, x - 1, y).is_some() || letter_at(grid, x + 1, y).is_some()
                };
                if blocked {
                    return false;
                }
            }
        }
    }

    placed.is_empty() || crossings > 0
}

fn crossing_count(grid: &Grid, c: &Placed, p: &Params) -> i32 {
    let wb = c.word.as_bytes();
    let (dx, dy) = match c.dir {
        Direction::Across => (1, 0),
        Direction::Down => (0, 1),
    };
    let mut count = 0;
    for i in 0..wb.len() as i32 {
        let x = c.x + dx * i;
        let y = c.y + dy * i;
        if in_bounds(x, y, p) && letter_at(grid, x, y) == Some(wb[i as usize]) {
            count += 1;
        }
    }
    count
}

fn place_word(grid: &mut Grid, placed: &mut Vec<Placed>, c: Placed) {
    let wb = c.word.as_bytes().to_vec();
    for (i, &b) in wb.iter().enumerate() {
        let (x, y) = cell_at(&c, i as i32);
        grid[y as usize][x as usize] = Some(b);
    }
    placed.push(c);
}

fn sample<'a, T>(values: &'a [T], rng: &mut Rng) -> &'a T {
    let i = ((rng.next() * values.len() as f64).floor() as usize).min(values.len() - 1);
    &values[i]
}

fn shuffle<T: Clone>(values: &[T], rng: &mut Rng) -> Vec<T> {
    let mut copy = values.to_vec();
    let len = copy.len();
    let mut i = len;
    while i > 1 {
        i -= 1;
        let j = ((rng.next() * (i as f64 + 1.0)).floor() as usize).min(len - 1);
        copy.swap(i, j);
    }
    copy
}

fn seed_candidates(d: &Dictionary, p: &Params) -> Vec<String> {
    let min = p.min_len.max(5);
    let max = p.max_len.min(12);
    let mut words: Vec<String> = vec![];
    for len in min..=max {
        if let Some(v) = d.by_length.get(&(len as usize)) {
            words.extend(v.iter().filter(|w| d.topic_words.contains(*w)).cloned());
        }
    }
    for len in min..=max {
        if let Some(v) = d.by_length.get(&(len as usize)) {
            words.extend(v.iter().cloned());
        }
    }
    words.sort_by(|a, b| {
        let sb = d.topic_scores.get(b).copied().unwrap_or(0.0);
        let sa = d.topic_scores.get(a).copied().unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    words
}

fn find_best_placements(
    grid: &Grid,
    placed: &[Placed],
    d: &Dictionary,
    p: &Params,
    rng: &mut Rng,
) -> Vec<(Placed, f64)> {
    let used: HashSet<&str> = placed.iter().map(|w| w.word.as_str()).collect();
    let mut placements: Vec<(Placed, f64)> = vec![];

    for anchor in shuffle(placed, rng) {
        let idxs: Vec<i32> = (0..anchor.word.len() as i32).collect();
        for ai in shuffle(&idxs, rng) {
            let (acx, acy) = cell_at(&anchor, ai);
            let letter = anchor.word.as_bytes()[ai as usize];
            let dir = anchor.dir.opposite();

            let Some(letter_words) = d.by_letter.get(&letter) else {
                continue;
            };
            for word in letter_words.iter().take(650) {
                if used.contains(word.as_str()) {
                    continue;
                }
                let matching: Vec<i32> = word
                    .as_bytes()
                    .iter()
                    .enumerate()
                    .filter(|(_, &b)| b == letter)
                    .map(|(i, _)| i as i32)
                    .collect();
                for wi in shuffle(&matching, rng) {
                    let candidate = Placed {
                        word: word.clone(),
                        dir,
                        x: if dir == Direction::Across {
                            acx - wi
                        } else {
                            acx
                        },
                        y: if dir == Direction::Down {
                            acy - wi
                        } else {
                            acy
                        },
                    };
                    if !can_place(grid, placed, &candidate, p) {
                        continue;
                    }
                    let topic = d.topic_scores.get(word).copied().unwrap_or(0.0);
                    let quality = d.quality_scores.get(word).copied().unwrap_or(0) as f64;
                    let freq = d.frequency_scores.get(word).copied().unwrap_or(0.0);
                    let len_score = if word.len() >= 4 && word.len() <= 7 {
                        8.0
                    } else {
                        0.0
                    };
                    let score = crossing_count(grid, &candidate, p) as f64 * 50.0
                        + topic * 100.0
                        + quality * 5.0
                        + (freq + 1.0).ln() * 8.0
                        + len_score;
                    placements.push((candidate, score));
                }
            }
        }
        if placements.len() > 1000 {
            break;
        }
    }

    placements.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    placements
}

fn generate(
    d: &Dictionary,
    p: &Params,
    seed: u32,
    run_number: i32,
    emit: &mut dyn FnMut(Value),
) -> (Grid, Vec<Placed>) {
    let mut rng = Rng::new(seed);
    let mut grid = empty_grid(p);
    let mut placed: Vec<Placed> = vec![];

    for sw in seed_candidates(d, p) {
        let c = Placed {
            word: sw.clone(),
            dir: Direction::Across,
            x: (p.width - sw.len() as i32) / 2,
            y: p.height / 2,
        };
        if can_place(&grid, &placed, &c, p) {
            place_word(&mut grid, &mut placed, c);
            break;
        }
    }

    let mut attempt = 0;
    while attempt < p.max_attempts && (placed.len() as i32) < p.target {
        let candidates = find_best_placements(&grid, &placed, d, p, &mut rng);
        if candidates.is_empty() {
            break;
        }
        let topn = candidates.len().min(12);
        let chosen = sample(&candidates[..topn], &mut rng).0.clone();
        place_word(&mut grid, &mut placed, chosen);

        if attempt % 20 == 0 {
            emit(json!({
                "type": "progress", "stage": "solving-attempts",
                "current": attempt, "total": p.max_attempts,
                "message": format!("Run {}/{}: placed {}/{}", run_number, p.runs, placed.len(), p.target),
            }));
        }
        attempt += 1;
    }

    (grid, placed)
}

pub fn generate_best(
    d: &Dictionary,
    p: &Params,
    emit: &mut dyn FnMut(Value),
) -> Result<Best, String> {
    let mut best: Option<Best> = None;

    for run in 0..p.runs {
        let seed = (run + 1) as u32;
        let (grid, placed) = generate(d, p, seed, run + 1, emit);

        if validate_grid(&grid, &placed, &d.dictionary_set, p).is_ok() {
            let score = score_board(&grid, &placed, d);
            if best.as_ref().is_none_or(|b| score > b.score) {
                best = Some(Best {
                    grid,
                    placed,
                    score,
                    seed,
                });
            }
        }

        emit(json!({
            "type": "progress", "stage": "solving",
            "current": run + 1, "total": p.runs,
            "message": format!("Best so far: {} words", best.as_ref().map(|b| b.placed.len()).unwrap_or(0)),
        }));
    }

    best.ok_or_else(|| "No valid crossword was generated.".to_string())
}

pub fn validate_grid(
    grid: &Grid,
    placed: &[Placed],
    dict_set: &HashSet<String>,
    p: &Params,
) -> Result<(), String> {
    let mut answers: HashSet<&str> = HashSet::new();
    for pw in placed {
        if !answers.insert(pw.word.as_str()) {
            return Err(format!("Duplicate answer: {}", pw.word));
        }
        if !dict_set.contains(&pw.word) {
            return Err(format!("Answer not in dictionary: {}", pw.word));
        }
        assert_maximal_slot(grid, pw)?;
    }

    let placed_keys: HashSet<String> = placed.iter().map(slot_key).collect();
    for slot in extract_slots(grid, p) {
        if slot.word.len() as i32 >= p.min_len && !placed_keys.contains(&slot_key(&slot)) {
            return Err(format!("Unclued accidental slot found: {}", slot.word));
        }
    }
    Ok(())
}

fn assert_maximal_slot(grid: &Grid, pw: &Placed) -> Result<(), String> {
    let (dx, dy) = match pw.dir {
        Direction::Across => (1, 0),
        Direction::Down => (0, 1),
    };
    let len = pw.word.len() as i32;
    if letter_at(grid, pw.x - dx, pw.y - dy).is_some()
        || letter_at(grid, pw.x + dx * len, pw.y + dy * len).is_some()
    {
        return Err(format!("Placed word is not a maximal slot: {}", pw.word));
    }
    Ok(())
}

fn extract_slots(grid: &Grid, p: &Params) -> Vec<Placed> {
    let mut slots = vec![];
    for y in 0..p.height {
        let mut x = 0;
        while x < p.width {
            while x < p.width && grid[y as usize][x as usize].is_none() {
                x += 1;
            }
            let start = x;
            let mut word = String::new();
            while x < p.width {
                let Some(b) = grid[y as usize][x as usize] else {
                    break;
                };
                word.push(b as char);
                x += 1;
            }
            if word.len() as i32 >= p.min_len {
                slots.push(Placed {
                    dir: Direction::Across,
                    x: start,
                    y,
                    word,
                });
            }
        }
    }
    for x in 0..p.width {
        let mut y = 0;
        while y < p.height {
            while y < p.height && grid[y as usize][x as usize].is_none() {
                y += 1;
            }
            let start = y;
            let mut word = String::new();
            while y < p.height {
                let Some(b) = grid[y as usize][x as usize] else {
                    break;
                };
                word.push(b as char);
                y += 1;
            }
            if word.len() as i32 >= p.min_len {
                slots.push(Placed {
                    dir: Direction::Down,
                    x,
                    y: start,
                    word,
                });
            }
        }
    }
    slots
}

fn slot_key(s: &Placed) -> String {
    format!("{}:{}:{}:{}", s.dir.as_str(), s.x, s.y, s.word)
}

/// Assign clue numbers in reading order; returns (word, number) sorted by number
/// then direction (ACROSS before DOWN), matching the TS `numberWords`.
pub fn number_words(placed: &[Placed]) -> Vec<(Placed, i32)> {
    let mut starts: HashMap<(i32, i32), Option<i32>> = HashMap::new();
    for w in placed {
        starts.insert((w.x, w.y), None);
    }
    let mut number = 1;
    let max_y = placed.iter().map(|w| w.y).max().unwrap_or(0);
    let max_x = placed.iter().map(|w| w.x).max().unwrap_or(0);
    for y in 0..=max_y {
        for x in 0..=max_x {
            if let Some(slot) = starts.get_mut(&(x, y)) {
                *slot = Some(number);
                number += 1;
            }
        }
    }
    let mut out: Vec<(Placed, i32)> = placed
        .iter()
        .map(|w| {
            let n = starts.get(&(w.x, w.y)).and_then(|o| *o).unwrap_or(0);
            (w.clone(), n)
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.dir.as_str().cmp(b.0.dir.as_str())));
    out
}

fn score_board(grid: &Grid, placed: &[Placed], d: &Dictionary) -> f64 {
    let filled = grid.iter().flatten().filter(|c| c.is_some()).count() as f64;
    let topic_score: f64 = placed
        .iter()
        .map(|w| d.topic_scores.get(&w.word).copied().unwrap_or(0.0))
        .sum();
    let topic_word_count = placed
        .iter()
        .filter(|w| d.topic_words.contains(&w.word))
        .count() as f64;
    placed.len() as f64 * 1000.0 + filled * 10.0 + topic_word_count * 25.0 + topic_score
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tiny hand-built dictionary that crosses CAT/COT/CAR at shared letters
    // proves can_place, crossing detection, numbering, and validation end to end.
    fn mini_dict() -> Dictionary {
        // seed_candidates needs a length>=5 word to start a grid; the 3-letter
        // words cross it.
        let words = ["CRANE", "CAR", "RAN", "EAR", "ACE", "ARC"];
        let mut by_letter: HashMap<u8, Vec<String>> = HashMap::new();
        let mut by_length: HashMap<usize, Vec<String>> = HashMap::new();
        for w in words {
            by_length.entry(w.len()).or_default().push(w.to_string());
            for b in w.bytes().collect::<HashSet<_>>() {
                by_letter.entry(b).or_default().push(w.to_string());
            }
        }
        Dictionary {
            dictionary_set: words.iter().map(|s| s.to_string()).collect(),
            topic_words: HashSet::new(),
            topic_scores: HashMap::new(),
            quality_scores: words.iter().map(|w| (w.to_string(), 5)).collect(),
            frequency_scores: words.iter().map(|w| (w.to_string(), 10.0)).collect(),
            clue_by_word: words
                .iter()
                .map(|w| (w.to_string(), w.to_lowercase()))
                .collect(),
            by_letter,
            by_length,
        }
    }

    #[test]
    fn places_and_crosses_words() {
        let d = mini_dict();
        let p = Params {
            width: 9,
            height: 9,
            min_len: 3,
            max_len: 6,
            target: 4,
            runs: 8,
            max_attempts: 80,
        };
        let mut noop = |_: Value| {};
        let best = generate_best(&d, &p, &mut noop).expect("a valid grid");
        // at least two crossing words and the winning grid validates
        assert!(
            best.placed.len() >= 2,
            "expected crossings, got {}",
            best.placed.len()
        );
        validate_grid(&best.grid, &best.placed, &d.dictionary_set, &p).unwrap();
        // numbering is 1-based and contiguous start cells
        let numbered = number_words(&best.placed);
        assert!(numbered.iter().all(|(_, n)| *n >= 1));
    }

    #[test]
    fn rng_matches_lcg() {
        // first draw of seed=1: (1*1664525+1013904223) mod 2^32 / 2^32
        let mut rng = Rng::new(1);
        let expected =
            (1u32.wrapping_mul(1664525).wrapping_add(1013904223)) as f64 / 4_294_967_296.0;
        assert!((rng.next() - expected).abs() < 1e-12);
    }
}
