use std::env;
use std::fs;
use std::io::{self, Read};
use std::process;

use rchess::chess::Position;
use rchess::pgn::{export_pgn_with_tags, parse_pgn, position_after_moves};
use rchess::search::Engine;

fn main() {
    let mut args = env::args().skip(1);

    match args.next().as_deref() {
        Some("perft") => {
            let depth = args
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(3);
            let position = Position::startpos();
            println!("{}", position.perft(depth));
        }
        Some("bestmove") => {
            let depth = args
                .next()
                .and_then(|value| value.parse::<u8>().ok())
                .unwrap_or(4);
            let fen = args.collect::<Vec<_>>().join(" ");
            let position = if fen.trim().is_empty() {
                Position::startpos()
            } else {
                match Position::from_fen(&fen) {
                    Ok(position) => position,
                    Err(error) => {
                        eprintln!("FEN error: {error}");
                        process::exit(2);
                    }
                }
            };
            let mut engine = Engine::new(depth);
            match engine.best_move(&position) {
                Some(best_move) => println!("{}", best_move.to_uci()),
                None => println!("0000"),
            }
        }
        Some("fen") => {
            let fen = args.collect::<Vec<_>>().join(" ");
            match Position::from_fen(&fen) {
                Ok(position) => {
                    println!("{}", position.to_fen());
                    println!("{}", position.ascii_board());
                }
                Err(error) => {
                    eprintln!("FEN error: {error}");
                    process::exit(2);
                }
            }
        }
        Some("pgn") => {
            let input = match read_pgn_input(args.next()) {
                Ok(input) => input,
                Err(error) => {
                    eprintln!("PGN read error: {error}");
                    process::exit(2);
                }
            };
            match parse_pgn(&input) {
                Ok(game) => match position_after_moves(&game.start_fen, &game.moves) {
                    Ok(position) => {
                        println!("Result: {}", game.result);
                        println!("Final FEN: {}", position.to_fen());
                        println!(
                            "UCI moves: {}",
                            game.moves
                                .iter()
                                .map(|chess_move| chess_move.to_uci())
                                .collect::<Vec<_>>()
                                .join(" ")
                        );
                        println!();
                        match export_pgn_with_tags(&game.start_fen, &game.moves, &game.result, &game.tags) {
                            Ok(normalized) => print!("{normalized}"),
                            Err(error) => eprintln!("PGN export error: {error}"),
                        }
                    }
                    Err(error) => {
                        eprintln!("PGN replay error: {error}");
                        process::exit(2);
                    }
                },
                Err(error) => {
                    eprintln!("PGN parse error: {error}");
                    process::exit(2);
                }
            }
        }
        _ => rchess::uci::run(),
    }
}

fn read_pgn_input(path: Option<String>) -> Result<String, String> {
    if let Some(path) = path {
        fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))
    } else {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| error.to_string())?;
        Ok(input)
    }
}
