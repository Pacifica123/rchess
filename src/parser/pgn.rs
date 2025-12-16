use crate::game::{CurrentGameStatus, MoveHistory};

// pgn.rs
pub fn parse_pgn(pgn: &str) -> Result<(CurrentGameStatus, MoveHistory), String> {
    // Базовый парсер PGN: разбор ходов вида "e4", "Nf3", "O-O"
    // TODO: полный SAN парсинг с проверкой неоднозначностей
    // let mut status = CurrentGameStatus::default(); // из Game::new
    let mut history = MoveHistory::new();
    
    // Разбор [FEN "..."] если есть, иначе стартовая позиция
    // Разбор ходов через regex или ручной парсер
    // Применение через Game::apply_move
    
    todo!("Полный PGN парсер с SAN -> Move конвертацией");
}
