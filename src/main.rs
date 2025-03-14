

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

    let mut g = game::Game::new_with_board(game::Gamemode::PCvsPC, board);

    g.start();

    let mut eng = engine::Engine::new(2, 12);
    eng.make_move(&mut g);

    //TODO:  Выбор режима 
    //TODO:  Подключение/Отключение модулей интерактивно
}
