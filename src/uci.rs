use std::io::{self, BufRead, Write};

use crate::chess::{Position, STARTPOS_FEN};
use crate::experience::{ExperienceBook, ExperienceConfig};
use crate::search::{mate_score_to_uci_moves, Engine};

pub fn run() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut position = Position::startpos();
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
            println!("uciok");
        } else if line == "isready" {
            println!("readyok");
        } else if line == "ucinewgame" {
            position = Position::startpos();
        } else if let Some(rest) = line.strip_prefix("setoption ") {
            handle_setoption(rest, &mut engine, &mut experience);
        } else if let Some(rest) = line.strip_prefix("position ") {
            match parse_position_command(rest) {
                Ok(next_position) => position = next_position,
                Err(error) => eprintln!("info string position error: {error}"),
            }
        } else if let Some(rest) = line.strip_prefix("go") {
            let depth = parse_go_depth(rest).unwrap_or_else(|| parse_go_movetime_depth(rest).unwrap_or(4));
            engine.set_depth(depth);
            let settings = engine.settings();
            let best = search_best_move(&mut engine, &position, &experience);
            match best {
                Some((chess_move, score, experience_note)) => {
                    println!(
                        "info depth {depth} {} nodes {} hashfull 0 string deterministic_multithread={} max_threads={} granularity={} hash_mb={}",
                        format_uci_score(score),
                        engine.searched_nodes(),
                        settings.deterministic_multithread,
                        settings.max_threads,
                        settings.granularity,
                        settings.hash_mb
                    );
                    if let Some(note) = experience_note {
                        println!("info string {note}");
                    }
                    println!("bestmove {}", chess_move.to_uci());
                }
                None => {
                    if position.is_checkmate() {
                        println!("info depth {depth} score mate -1 nodes {} string terminal checkmate", engine.searched_nodes());
                    } else {
                        println!("info depth {depth} score cp 0 nodes {} string terminal stalemate-or-no-move", engine.searched_nodes());
                    }
                    println!("bestmove 0000");
                }
            }
        } else if let Some(rest) = line.strip_prefix("perft ") {
            let depth = rest.trim().parse::<u32>().unwrap_or(1);
            println!("nodes {}", position.perft(depth));
        } else if line == "d" {
            println!("{}", position.ascii_board());
            println!("Fen: {}", position.to_fen());
        } else if line == "stop" {
            continue;
        } else if line == "quit" {
            break;
        }

        let _ = stdout.flush();
    }
}

fn search_best_move(
    engine: &mut Engine,
    position: &Position,
    experience: &ExperienceConfig,
) -> Option<(crate::chess::ChessMove, i32, Option<String>)> {
    let candidates = engine.root_candidates(position);
    let best = candidates.first().copied()?;
    let config = experience.clone().normalized();
    if !config.enabled {
        return Some((best.chess_move, best.score, None));
    }

    match ExperienceBook::load_from_path(&config.path) {
        Ok(book) => {
            if let Some(decision) = book.choose_move(position, &candidates, config.min_games, config.score_tolerance_cp) {
                Some((decision.chosen_move, decision.chosen_score, Some(decision.uci_info())))
            } else {
                Some((
                    best.chess_move,
                    best.score,
                    Some(format!(
                        "experience book active path={} but no eligible record for {}",
                        config.path,
                        best.chess_move.to_uci()
                    )),
                ))
            }
        }
        Err(error) => Some((
            best.chess_move,
            best.score,
            Some(format!("experience book disabled for this move: {error}")),
        )),
    }
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

fn parse_position_command(rest: &str) -> Result<Position, String> {
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

    if let Some(index) = move_index {
        for token in &tokens[index + 1..] {
            let chess_move = position
                .parse_uci_move(token)
                .ok_or_else(|| format!("bad or illegal move: {token}"))?;
            position.make_legal_move(chess_move)?;
        }
    }
    Ok(position)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_startpos_with_moves() {
        let position = parse_position_command("startpos moves e2e4 e7e5").unwrap();
        assert_eq!(position.to_fen(), "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2");
    }

    #[test]
    fn parses_fen() {
        let position = parse_position_command(&format!("fen {STARTPOS_FEN}")).unwrap();
        assert_eq!(position.to_fen(), STARTPOS_FEN);
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
    fn maps_movetime_to_internal_depth() {
        assert_eq!(parse_go_movetime_depth("movetime 10"), Some(1));
        assert_eq!(parse_go_movetime_depth("movetime 1000"), Some(4));
        assert_eq!(parse_go_movetime_depth("movetime 30000"), Some(8));
    }
}
