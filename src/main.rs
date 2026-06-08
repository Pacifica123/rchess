mod chess;
mod search;
mod uci;

use std::env;
use std::process;

use chess::Position;
use search::Engine;

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
        _ => uci::run(),
    }
}
