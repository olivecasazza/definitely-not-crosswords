use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::Path;

const PATTERN: [&str; 9] = [
    "...#.....",
    "...#.....",
    ".........",
    "#.......#",
    ".........",
    ".........",
    "#.......#",
    ".....#...",
    ".....#...",
];
const MIN_WORD_LENGTH: usize = 3;
const WORDNET_DICT_DIR: &str = "data/crossword/wordnet/dict";

const TOPIC_WORDS: [&str; 15] = [
    "ARGON", "RADON", "XENON", "NEON", "IONS", "ATOM", "ORBIT", "LUNAR", "SOLAR",
    "ROVER", "COMET", "STAR", "MARS", "MOON", "NOVA",
];

#[derive(Clone, Copy)]
struct Cell {
    x: usize,
    y: usize,
}

#[derive(Clone, Copy)]
enum Direction {
    Across,
    Down,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Direction::Across => write!(f, "ACROSS"),
            Direction::Down => write!(f, "DOWN"),
        }
    }
}

#[derive(Clone)]
struct Slot {
    id: String,
    direction: Direction,
    x: usize,
    y: usize,
    length: usize,
    cells: Vec<Cell>,
}

#[derive(Clone, Copy)]
struct Edge {
    other_slot: usize,
    self_index: usize,
    other_index: usize,
}

#[derive(Default)]
struct Stats {
    branches: usize,
    backtracks: usize,
}

struct Solved {
    assignments: Vec<Option<String>>,
    stats: Stats,
}

fn main() {
    let slots = extract_slots(&PATTERN, MIN_WORD_LENGTH);
    let intersections = build_intersections(&slots);
    let dictionary = load_wordnet_dictionary(WORDNET_DICT_DIR, MIN_WORD_LENGTH, max_slot_length(&slots))
        .unwrap_or_else(|error| {
            eprintln!("Could not load WordNet dictionary: {error}");
            std::process::exit(1);
        });
    let domains = create_initial_domains(&slots, &dictionary);
    let mut stats = Stats::default();
    let result = solve(
        &slots,
        &intersections,
        domains.clone(),
        vec![None; slots.len()],
        &mut stats,
    )
    .map(|assignments| Solved { assignments, stats });

    println!("Pattern:");
    print_pattern(&PATTERN);
    println!();

    println!("Initial domains:");
    println!("Loaded dictionary words: {}", dictionary.len());
    for (index, slot) in slots.iter().enumerate() {
        println!(
            "{:<3} {:<6} ({},{}) len={} options={}",
            slot.id,
            slot.direction,
            slot.x,
            slot.y,
            slot.length,
            domains[index].len()
        );
    }
    println!();

    match result {
        Some(solved) => {
            validate_solution(&slots, &intersections, &domains, &solved.assignments)
                .unwrap_or_else(|error| {
                    eprintln!("Invalid generated crossword: {error}");
                    std::process::exit(1);
                });

            println!("Solved grid:");
            println!("{}", render_grid(&PATTERN, &slots, &solved.assignments));
            println!();

            println!("Assignments:");
            for (index, slot) in slots.iter().enumerate() {
                let word = solved.assignments[index].as_ref().unwrap();
                let topic_marker = if is_topic_word(word) { " topic" } else { "" };
                println!(
                    "{:<3} {:<6} ({},{}) {}{}",
                    slot.id, slot.direction, slot.x, slot.y, word, topic_marker
                );
            }
            println!();
            println!("Validation: ok");
            println!("Branches: {}", solved.stats.branches);
            println!("Backtracks: {}", solved.stats.backtracks);
        }
        None => {
            println!("No solution.");
            std::process::exit(1);
        }
    }
}

fn extract_slots(pattern: &[&str], min_word_length: usize) -> Vec<Slot> {
    let height = pattern.len();
    let width = pattern[0].len();
    let rows: Vec<Vec<u8>> = pattern.iter().map(|row| row.as_bytes().to_vec()).collect();
    let mut slots = Vec::new();

    for y in 0..height {
        let mut x = 0;
        while x < width {
            while x < width && rows[y][x] == b'#' {
                x += 1;
            }
            let start = x;
            while x < width && rows[y][x] != b'#' {
                x += 1;
            }
            if x - start >= min_word_length {
                let cells = (start..x).map(|cell_x| Cell { x: cell_x, y }).collect();
                slots.push(Slot {
                    id: format!("A{}", slots.len() + 1),
                    direction: Direction::Across,
                    x: start,
                    y,
                    length: x - start,
                    cells,
                });
            }
        }
    }

    for x in 0..width {
        let mut y = 0;
        while y < height {
            while y < height && rows[y][x] == b'#' {
                y += 1;
            }
            let start = y;
            while y < height && rows[y][x] != b'#' {
                y += 1;
            }
            if y - start >= min_word_length {
                let cells = (start..y).map(|cell_y| Cell { x, y: cell_y }).collect();
                slots.push(Slot {
                    id: format!("D{}", slots.len() + 1),
                    direction: Direction::Down,
                    x,
                    y: start,
                    length: y - start,
                    cells,
                });
            }
        }
    }

    slots
}

fn build_intersections(slots: &[Slot]) -> Vec<Vec<Edge>> {
    let mut intersections = vec![Vec::new(); slots.len()];

    for a_index in 0..slots.len() {
        for b_index in (a_index + 1)..slots.len() {
            for (a_cell_index, a_cell) in slots[a_index].cells.iter().enumerate() {
                for (b_cell_index, b_cell) in slots[b_index].cells.iter().enumerate() {
                    if a_cell.x == b_cell.x && a_cell.y == b_cell.y {
                        intersections[a_index].push(Edge {
                            other_slot: b_index,
                            self_index: a_cell_index,
                            other_index: b_cell_index,
                        });
                        intersections[b_index].push(Edge {
                            other_slot: a_index,
                            self_index: b_cell_index,
                            other_index: a_cell_index,
                        });
                    }
                }
            }
        }
    }

    intersections
}

fn max_slot_length(slots: &[Slot]) -> usize {
    slots.iter().map(|slot| slot.length).max().unwrap_or(MIN_WORD_LENGTH)
}

fn load_wordnet_dictionary(
    dict_dir: &str,
    min_word_length: usize,
    max_word_length: usize,
) -> Result<Vec<String>, String> {
    let index_files = ["index.noun", "index.verb", "index.adj", "index.adv"];
    let mut words = HashSet::new();

    for file_name in index_files {
        let path = Path::new(dict_dir).join(file_name);
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;

        for line in contents.lines() {
            if line.is_empty() || line.starts_with(' ') {
                continue;
            }

            let Some(lemma) = line.split_whitespace().next() else {
                continue;
            };

            if lemma.contains('_') || lemma.contains('-') || lemma.contains('\'') {
                continue;
            }

            let word = lemma.to_ascii_uppercase();
            if word.len() < min_word_length || word.len() > max_word_length {
                continue;
            }

            if word.bytes().all(|byte| byte.is_ascii_uppercase()) {
                words.insert(word);
            }
        }
    }

    let mut words = words.into_iter().collect::<Vec<_>>();
    words.sort();
    Ok(words)
}

fn create_initial_domains(slots: &[Slot], words: &[String]) -> Vec<Vec<String>> {
    slots
        .iter()
        .map(|slot| {
            words
                .iter()
                .filter(|word| word.len() == slot.length)
                .cloned()
                .collect()
        })
        .collect()
}

fn solve(
    slots: &[Slot],
    intersections: &[Vec<Edge>],
    mut domains: Vec<Vec<String>>,
    assignments: Vec<Option<String>>,
    stats: &mut Stats,
) -> Option<Vec<Option<String>>> {
    if !propagate(&mut domains, intersections, &assignments) {
        stats.backtracks += 1;
        return None;
    }

    if assignments.iter().all(Option::is_some) {
        return Some(assignments);
    }

    let slot_index = choose_lowest_entropy_slot(slots, &domains, &assignments)?;
    for word in weighted_words(&domains[slot_index]) {
        stats.branches += 1;
        let mut next_assignments = assignments.clone();
        next_assignments[slot_index] = Some(word);

        if let Some(result) = solve(slots, intersections, domains.clone(), next_assignments, stats) {
            return Some(result);
        }
    }

    stats.backtracks += 1;
    None
}

fn propagate(
    domains: &mut [Vec<String>],
    intersections: &[Vec<Edge>],
    assignments: &[Option<String>],
) -> bool {
    let mut changed = true;

    while changed {
        changed = false;
        let used_words: HashSet<String> = assignments.iter().filter_map(Clone::clone).collect();

        for slot_index in 0..domains.len() {
            if assignments[slot_index].is_some() {
                continue;
            }

            let mut next_domain = domains[slot_index].clone();

            for edge in &intersections[slot_index] {
                let allowed_letters: HashSet<u8> = if let Some(word) = &assignments[edge.other_slot] {
                    HashSet::from([word.as_bytes()[edge.other_index]])
                } else {
                    domains[edge.other_slot]
                        .iter()
                        .map(|word| word.as_bytes()[edge.other_index])
                        .collect()
                };

                next_domain.retain(|word| allowed_letters.contains(&word.as_bytes()[edge.self_index]));
            }

            next_domain.retain(|word| !used_words.contains(word));

            if next_domain.len() != domains[slot_index].len() {
                domains[slot_index] = next_domain;
                changed = true;
            }

            if domains[slot_index].is_empty() {
                return false;
            }
        }
    }

    true
}

fn choose_lowest_entropy_slot(
    slots: &[Slot],
    domains: &[Vec<String>],
    assignments: &[Option<String>],
) -> Option<usize> {
    (0..slots.len())
        .filter(|index| assignments[*index].is_none())
        .min_by_key(|index| (domains[*index].len(), usize::MAX - slots[*index].length))
}

fn weighted_words(domain: &[String]) -> Vec<String> {
    let mut words = domain.to_vec();
    words.sort_by(|a, b| {
        let a_topic = is_topic_word(a);
        let b_topic = is_topic_word(b);
        b_topic.cmp(&a_topic).then_with(|| a.cmp(b))
    });
    words
}

fn validate_solution(
    slots: &[Slot],
    intersections: &[Vec<Edge>],
    initial_domains: &[Vec<String>],
    assignments: &[Option<String>],
) -> Result<(), String> {
    if assignments.len() != slots.len() {
        return Err("assignment count does not match slot count".to_string());
    }

    let mut used_words = HashSet::new();

    for (slot_index, slot) in slots.iter().enumerate() {
        let word = assignments[slot_index]
            .as_ref()
            .ok_or_else(|| format!("{} is unassigned", slot.id))?;

        if word.len() != slot.length {
            return Err(format!(
                "{} has length {}, expected {}",
                slot.id,
                word.len(),
                slot.length
            ));
        }

        if !initial_domains[slot_index].contains(word) {
            return Err(format!("{} uses word {word}, which is not in its initial domain", slot.id));
        }

        if !used_words.insert(word.clone()) {
            return Err(format!("duplicate answer found: {word}"));
        }
    }

    for (slot_index, edges) in intersections.iter().enumerate() {
        let word = assignments[slot_index].as_ref().unwrap();
        for edge in edges {
            let other_word = assignments[edge.other_slot].as_ref().unwrap();
            let self_letter = word.as_bytes()[edge.self_index];
            let other_letter = other_word.as_bytes()[edge.other_index];
            if self_letter != other_letter {
                return Err(format!(
                    "{} and {} disagree at crossing: {} != {}",
                    slots[slot_index].id,
                    slots[edge.other_slot].id,
                    self_letter as char,
                    other_letter as char
                ));
            }
        }
    }

    Ok(())
}

fn is_topic_word(word: &str) -> bool {
    TOPIC_WORDS.contains(&word)
}

fn render_grid(pattern: &[&str], slots: &[Slot], assignments: &[Option<String>]) -> String {
    let mut grid: Vec<Vec<u8>> = pattern.iter().map(|row| row.as_bytes().to_vec()).collect();

    for (slot_index, slot) in slots.iter().enumerate() {
        if let Some(word) = &assignments[slot_index] {
            for (letter_index, cell) in slot.cells.iter().enumerate() {
                grid[cell.y][cell.x] = word.as_bytes()[letter_index];
            }
        }
    }

    grid.iter()
        .map(|row| {
            row.iter()
                .map(|byte| (*byte as char).to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_pattern(pattern: &[&str]) {
    for row in pattern {
        println!("{}", row.chars().map(|ch| ch.to_string()).collect::<Vec<_>>().join(" "));
    }
}
