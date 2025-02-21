

pub mod game;
pub mod engine;
pub mod parser;
pub mod meta;
pub mod rules;

fn main() {
    println!("Hello, world!");

    /* Test Board */
    let mut board = game::Board::new();
    board.init_by_default();
    board.display();

    //TODO:  Выбор режима 
    //TODO:  Подключение/Отключение модулей интерактивно
}
