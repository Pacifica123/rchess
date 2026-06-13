use std::io::{self, BufRead, Write};

use crate::chess::{ChessMove, Position, STARTPOS_FEN};
use crate::experience::{ExperienceBook, ExperienceConfig};
use crate::search::{evaluate_for_side_to_move, mate_score_to_uci_moves, Engine, RootCandidate, SearchSettings};

pub fn run() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut state = UciPositionState::startpos();
    let mut engine = Engine::new(4);
    let mut experience = ExperienceConfig::default();

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "uci" {
            println!("id name rchess-reborn 0.4.0");
            println!("id author ReD_Chajek project");
            let settings = engine.settings();
            println!("option name Depth type spin default 4 min 1 max 8");
            println!(
                "option name deterministic_multithread type check default {}",
                settings.deterministic_multithread
            );
            println!("option name max_threads type spin default {} min 1 max 64", settings.max_threads);
            println!("option name granularity type spin default 1 min 1 max 64");
            println!("option name Hash type spin default 64 min 1 max 4096");
            println!("option name UseExperienceBook type check default false");
            println!("option name ExperienceBookPath type string default rchess_experience.rxp");
            println!("option name ExperienceMinGames type spin default 1 min 1 max 10000");
            println!("option name ExperienceScoreToleranceCp type spin default 80 min 0 max 1000");
            println!("option name AvoidDraws type check default false");
            println!("option name DrawContemptCp type spin default 35 min 0 max 400");
            println!("option name RiskLevel type spin default 0 min -100 max 100");
            println!("option name HumanityLevel type spin default 0 min -100 max 100");
            println!("uciok");
        } else if line == "isready" {
            println!("readyok");
        } else if line == "ucinewgame" {
            state = UciPositionState::startpos();
        } else if let Some(rest) = line.strip_prefix("setoption ") {
            handle_setoption(rest, &mut engine, &mut experience);
        } else if let Some(rest) = line.strip_prefix("position ") {
            match parse_position_command(rest) {
                Ok(next_state) => state = next_state,
                Err(error) => eprintln!("info string position error: {error}"),
            }
        } else if let Some(rest) = line.strip_prefix("go") {
            let depth = parse_go_depth(rest).unwrap_or_else(|| parse_go_movetime_depth(rest).unwrap_or(4));
            engine.set_depth(depth);
            let settings = engine.settings();
            let best = search_best_move(&mut engine, &state, &experience);
            match best {
                Some((chess_move, score, experience_note)) => {
                    println!(
                        "info depth {depth} {} nodes {} hashfull 0 string deterministic_multithread={} max_threads={} granularity={} hash_mb={} risk_level={} humanity_level={}",
                        format_uci_score(score),
                        engine.searched_nodes(),
                        settings.deterministic_multithread,
                        settings.max_threads,
                        settings.granularity,
                        settings.hash_mb,
                        settings.risk_level,
                        settings.humanity_level
                    );
                    if let Some(note) = experience_note {
                        println!("info string {note}");
                    }
                    println!("bestmove {}", chess_move.to_uci());
                }
                None => {
                    if state.position.is_checkmate() {
                        println!("info depth {depth} score mate -1 nodes {} string terminal checkmate", engine.searched_nodes());
                    } else {
                        println!("info depth {depth} score cp 0 nodes {} string terminal stalemate-or-no-move", engine.searched_nodes());
                    }
                    println!("bestmove 0000");
                }
            }
        } else if let Some(rest) = line.strip_prefix("perft ") {
            let depth = rest.trim().parse::<u32>().unwrap_or(1);
            println!("nodes {}", state.position.perft(depth));
        } else if line == "d" {
            println!("{}", state.position.ascii_board());
            println!("Fen: {}", state.position.to_fen());
        } else if line == "stop" {
            continue;
        } else if line == "quit" {
            break;
        }

        let _ = stdout.flush();
    }
}

#[derive(Clone, Debug)]
struct UciPositionState {
    start_fen: String,
    position: Position,
    moves: Vec<ChessMove>,
}

impl UciPositionState {
    fn startpos() -> Self {
        Self {
            start_fen: STARTPOS_FEN.to_string(),
            position: Position::startpos(),
            moves: Vec::new(),
        }
    }
}

fn search_best_move(
    engine: &mut Engine,
    state: &UciPositionState,
    experience: &ExperienceConfig,
) -> Option<(crate::chess::ChessMove, i32, Option<String>)> {
    let candidates = engine.root_candidates(&state.position);
    let best = candidates.first().copied()?;
    let config = experience.clone().normalized();
    let mut selected_move = best.chess_move;
    let mut selected_score = best.score;
    let mut note = None;

    if config.enabled {
        match ExperienceBook::load_from_path(&config.path) {
            Ok(book) => {
                if let Some(decision) = book.choose_move(&state.position, &candidates, config.min_games, config.score_tolerance_cp) {
                    selected_move = decision.chosen_move;
                    selected_score = decision.chosen_score;
                    note = Some(decision.uci_info());
                } else {
                    note = Some(format!(
                        "experience book active path={} but no eligible record for {}",
                        config.path,
                        best.chess_move.to_uci()
                    ));
                }
            }
            Err(error) => {
                note = Some(format!("experience book disabled for this move: {error}"));
            }
        }
    }

    if let Some((draw_move, draw_score, draw_note)) = choose_avoid_draw_move(engine.settings(), state, &candidates, selected_move, selected_score) {
        selected_move = draw_move;
        selected_score = draw_score;
        append_info_note(&mut note, draw_note);
    }

    Some((selected_move, selected_score, note))
}

fn append_info_note(note: &mut Option<String>, extra: String) {
    match note {
        Some(existing) if !existing.is_empty() => {
            existing.push_str("; ");
            existing.push_str(&extra);
        }
        _ => *note = Some(extra),
    }
}

fn choose_avoid_draw_move(
    settings: SearchSettings,
    state: &UciPositionState,
    candidates: &[RootCandidate],
    selected_move: ChessMove,
    selected_score: i32,
) -> Option<(ChessMove, i32, String)> {
    if !settings.avoid_draws || evaluate_for_side_to_move(&state.position) <= -150 {
        return None;
    }
    if !candidate_has_draw_risk(state, selected_move) {
        return None;
    }
    let tolerance = (settings.draw_contempt_cp.max(35) * 4).clamp(80, 220);
    let score_floor = selected_score.saturating_sub(tolerance);
    for candidate in candidates {
        if candidate.score < score_floor {
            continue;
        }
        if !candidate_has_draw_risk(state, candidate.chess_move) {
            return Some((
                candidate.chess_move,
                candidate.score,
                format!(
                    "AvoidDraws replaced {} with {} inside {} cp because the selected move repeats or claims a draw",
                    selected_move.to_uci(),
                    candidate.chess_move.to_uci(),
                    tolerance
                ),
            ));
        }
    }
    None
}

fn candidate_has_draw_risk(state: &UciPositionState, chess_move: ChessMove) -> bool {
    let mut moves = state.moves.clone();
    moves.push(chess_move);
    if Position::draw_reason_from_history(&state.start_fen, &moves)
        .ok()
        .flatten()
        .is_some()
    {
        return true;
    }
    Position::repetition_count_from_history(&state.start_fen, &moves)
        .map(|count| count >= 2)
        .unwrap_or(false)
}

fn handle_setoption(rest: &str, engine: &mut Engine, experience: &mut ExperienceConfig) {
    let Some((name, value)) = parse_setoption_name_value(rest) else {
        return;
    };
    match normalize_option_name(&name).as_str() {
        "depth" => {
            if let Ok(depth) = value.parse::<u8>() {
                engine.set_depth(depth.clamp(1, 8));
            }
        }
        "deterministic_multithread" => {
            let enabled = matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on");
            engine.set_deterministic_multithread(enabled);
        }
        "max_threads" | "threads" => {
            if let Ok(threads) = value.parse::<usize>() {
                engine.set_max_threads(threads);
            }
        }
        "granularity" => {
            if let Ok(granularity) = value.parse::<usize>() {
                engine.set_granularity(granularity);
            }
        }
        "hash" | "hash_mb" => {
            if let Ok(hash_mb) = value.parse::<usize>() {
                engine.set_hash_mb(hash_mb);
            }
        }
        "useexperiencebook" | "use_experience_book" | "experience_book" => {
            experience.enabled = matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on");
        }
        "experiencebookpath" | "experience_book_path" => {
            if !value.trim().is_empty() {
                experience.path = value.trim().to_string();
            }
        }
        "experiencemingames" | "experience_min_games" => {
            if let Ok(min_games) = value.parse::<u32>() {
                experience.min_games = min_games.clamp(1, 10_000);
            }
        }
        "experiencescoretolerancecp" | "experience_score_tolerance_cp" => {
            if let Ok(tolerance) = value.parse::<i32>() {
                experience.score_tolerance_cp = tolerance.clamp(0, 1_000);
            }
        }
        "avoiddraws" | "avoid_draws" => {
            let enabled = matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on");
            engine.set_avoid_draws(enabled);
        }
        "drawcontemptcp" | "draw_contempt_cp" => {
            if let Ok(value) = value.parse::<i32>() {
                engine.set_draw_contempt_cp(value);
            }
        }
        "risklevel" | "risk_level" | "personality_risk" => {
            if let Ok(value) = value.parse::<i32>() {
                engine.set_risk_level(value);
            }
        }
        "humanitylevel" | "humanity_level" | "personality_humanity" => {
            if let Ok(value) = value.parse::<i32>() {
                engine.set_humanity_level(value);
            }
        }
        _ => {}
    }
}

fn parse_setoption_name_value(rest: &str) -> Option<(String, String)> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.len() < 4 || !tokens[0].eq_ignore_ascii_case("name") {
        return None;
    }
    let value_index = tokens.iter().position(|token| token.eq_ignore_ascii_case("value"))?;
    if value_index <= 1 || value_index + 1 >= tokens.len() {
        return None;
    }
    let name = tokens[1..value_index].join(" ");
    let value = tokens[value_index + 1..].join(" ");
    Some((name, value))
}

fn normalize_option_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

fn parse_go_depth(rest: &str) -> Option<u8> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    for window in tokens.windows(2) {
        if window[0] == "depth" {
            return window[1].parse::<u8>().ok().map(|depth| depth.clamp(1, 8));
        }
    }
    None
}

fn parse_go_movetime_depth(rest: &str) -> Option<u8> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    for window in tokens.windows(2) {
        if window[0] == "movetime" {
            let movetime_ms = window[1].parse::<u64>().ok()?;
            return Some(match movetime_ms {
                0..=25 => 1,
                26..=100 => 2,
                101..=350 => 3,
                351..=1200 => 4,
                1201..=3500 => 5,
                3501..=9000 => 6,
                9001..=20000 => 7,
                _ => 8,
            });
        }
    }
    None
}

fn format_uci_score(score: i32) -> String {
    if let Some(mate) = mate_score_to_uci_moves(score) {
        format!("score mate {mate}")
    } else {
        format!("score cp {score}")
    }
}

fn parse_position_command(rest: &str) -> Result<UciPositionState, String> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() {
        return Err("empty position command".to_string());
    }

    let mut move_index = None;
    for (index, token) in tokens.iter().enumerate() {
        if *token == "moves" {
            move_index = Some(index);
            break;
        }
    }

    let position_part_end = move_index.unwrap_or(tokens.len());
    let mut position = if tokens[0] == "startpos" {
        Position::startpos()
    } else if tokens[0] == "fen" {
        let fen = tokens[1..position_part_end].join(" ");
        if fen.trim().is_empty() {
            return Err("missing FEN after `position fen`".to_string());
        }
        Position::from_fen(&fen)?
    } else {
        return Err("expected `startpos` or `fen`".to_string());
    };
    let start_fen = position.to_fen();
    let mut moves = Vec::new();

    if let Some(index) = move_index {
        for token in &tokens[index + 1..] {
            let chess_move = position
                .parse_uci_move(token)
                .ok_or_else(|| format!("bad or illegal move: {token}"))?;
            position.make_legal_move(chess_move)?;
            moves.push(chess_move);
        }
    }
    Ok(UciPositionState { start_fen, position, moves })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_startpos_with_moves() {
        let state = parse_position_command("startpos moves e2e4 e7e5").unwrap();
        assert_eq!(state.position.to_fen(), "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2");
    }

    #[test]
    fn parses_fen() {
        let state = parse_position_command(&format!("fen {STARTPOS_FEN}")).unwrap();
        assert_eq!(state.position.to_fen(), STARTPOS_FEN);
        assert_eq!(state.start_fen, STARTPOS_FEN);
    }

    #[test]
    fn parses_resource_setoptions() {
        let mut engine = Engine::new(4);
        let mut experience = ExperienceConfig::default();
        handle_setoption("name deterministic_multithread value true", &mut engine, &mut experience);
        handle_setoption("name max_threads value 4", &mut engine, &mut experience);
        handle_setoption("name granularity value 2", &mut engine, &mut experience);
        handle_setoption("name Hash value 8", &mut engine, &mut experience);
        let settings = engine.settings();
        assert!(settings.deterministic_multithread);
        assert_eq!(settings.max_threads, 4);
        assert_eq!(settings.granularity, 2);
        assert_eq!(settings.hash_mb, 8);
    }

    #[test]
    fn parses_experience_setoptions() {
        let mut engine = Engine::new(4);
        let mut experience = ExperienceConfig::default();
        handle_setoption("name UseExperienceBook value true", &mut engine, &mut experience);
        handle_setoption("name ExperienceBookPath value games/book.rxp", &mut engine, &mut experience);
        handle_setoption("name ExperienceMinGames value 3", &mut engine, &mut experience);
        handle_setoption("name ExperienceScoreToleranceCp value 40", &mut engine, &mut experience);
        assert!(experience.enabled);
        assert_eq!(experience.path, "games/book.rxp");
        assert_eq!(experience.min_games, 3);
        assert_eq!(experience.score_tolerance_cp, 40);
    }

    #[test]
    fn parses_avoid_draw_setoptions() {
        let mut engine = Engine::new(4);
        let mut experience = ExperienceConfig::default();
        handle_setoption("name AvoidDraws value true", &mut engine, &mut experience);
        handle_setoption("name DrawContemptCp value 90", &mut engine, &mut experience);
        let settings = engine.settings();
        assert!(settings.avoid_draws);
        assert_eq!(settings.draw_contempt_cp, 90);
    }

    #[test]
    fn parses_personality_setoptions() {
        let mut engine = Engine::new(4);
        let mut experience = ExperienceConfig::default();
        handle_setoption("name RiskLevel value 75", &mut engine, &mut experience);
        handle_setoption("name HumanityLevel value 40", &mut engine, &mut experience);
        let settings = engine.settings();
        assert_eq!(settings.risk_level, 75);
        assert_eq!(settings.humanity_level, 40);
    }

    #[test]
    fn maps_movetime_to_internal_depth() {
        assert_eq!(parse_go_movetime_depth("movetime 10"), Some(1));
        assert_eq!(parse_go_movetime_depth("movetime 1000"), Some(4));
        assert_eq!(parse_go_movetime_depth("movetime 30000"), Some(8));
    }
}
