use std::io::{self, BufRead, Write};

use crate::chess::{Position, STARTPOS_FEN};
use crate::search::Engine;

pub fn run() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut position = Position::startpos();
    let mut engine = Engine::new(4);

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "uci" {
            println!("id name rchess-reborn 0.2.0");
            println!("id author ReD_Chajek project");
            println!("option name Depth type spin default 4 min 1 max 8");
            println!("uciok");
        } else if line == "isready" {
            println!("readyok");
        } else if line == "ucinewgame" {
            position = Position::startpos();
        } else if let Some(rest) = line.strip_prefix("setoption ") {
            handle_setoption(rest, &mut engine);
        } else if let Some(rest) = line.strip_prefix("position ") {
            match parse_position_command(rest) {
                Ok(next_position) => position = next_position,
                Err(error) => eprintln!("info string position error: {error}"),
            }
        } else if let Some(rest) = line.strip_prefix("go") {
            let depth = parse_go_depth(rest).unwrap_or(4);
            engine.set_depth(depth);
            let best_move = engine.best_move(&position);
            println!("info depth {depth} nodes {}", engine.searched_nodes());
            match best_move {
                Some(chess_move) => println!("bestmove {}", chess_move.to_uci()),
                None => println!("bestmove 0000"),
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

fn handle_setoption(rest: &str, engine: &mut Engine) {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.len() >= 4
        && tokens[0].eq_ignore_ascii_case("name")
        && tokens[1].eq_ignore_ascii_case("Depth")
        && tokens[2].eq_ignore_ascii_case("value")
    {
        if let Ok(depth) = tokens[3].parse::<u8>() {
            engine.set_depth(depth.clamp(1, 8));
        }
    }
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
}
