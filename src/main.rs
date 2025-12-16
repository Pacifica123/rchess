use crate::game::Game;
use crate::game::Gamemode;
use crate::game::Move;
use crate::game::MoveType;
use crate::game::Position;
use crate::parser::fen::FenParseError;


pub mod game;
pub mod engine;
pub mod parser;
pub mod meta;
pub mod rules;
pub mod utils;

fn main() {
    println!("Hello, world!");

    /* Test Board */
    let mut board = game::Board::new();
    board.init_by_default();
    board.display();
    println!("- - - - - - - - - -");

    let mut g = game::Game::new_with_board(game::Gamemode::PCvsPC, board);

    // g.start();

    // let mut eng = engine::Engine::new(2, 12);
    // const TOTAL_MOVES: usize = 11000;
    // for take in 1..=TOTAL_MOVES {
    //     if g.status.is_gameover.is_some() {
    //         break;
    //     }
    //     eng.make_move(&mut g);
    //     println!("\n === Ход №{} | Состояние доски : ", take);
    //     g.status.board.display();
    // }

    /* Test apply_move */
    // let p  = g.status.board.pieces.get(&Position { file: 'e', rank: 2 });
    // let mv = Move {
    //     piece: {
    //         match p {
    //             Some(piece) => *piece,
    //             None => panic!("Piece not found at e2"),
    //         }
    //     },
    //     old_position: Position { file: 'e', rank: 2 },
    //     new_position: Position { file: 'e', rank: 4 },
    //     captured_piece_id: None,
    //     move_type: MoveType::Normal
    // };
    // g.apply_move(mv);
    // g.status.board.display();

    /* Test fromFEN */
    let game_result = Game::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", Gamemode::PlayerVsPlayer);

    let game = match game_result {
        Ok(game) => {
            println!("FEN успешно загружен!");
            game
        }
        Err(e) => {
            eprintln!("Ошибка парсинга FEN: {:?}", e);
            // Возвращаем стандартную партию или panic
            panic!("Невалидный FEN: {:?}", e);
            // Или: return Game::new(Gamemode::PlayerVsPlayer);
        }
    };

    //TODO:  Выбор режима 
    //TODO:  Подключение/Отключение модулей интерактивно
}
