//! Основная логика игры


use std::collections::HashMap;
use std::hash::{Hash, Hasher};

//TODO: Game(GameState, MoveHistory, GameMode), Move

    /*          --- ФИГУРА ---         */
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Color {
    White,
    Black
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceType {
    King,
    Queen,
    Rook,
    Bishop,
    Knight,
    Pawn
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub file: char, // a - h
    pub rank: u8
}

impl Hash for Position {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file.hash(state);
        self.rank.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub piece_type: PieceType,
    pub color: Color,
    pub pos: Option<Position>,  // None - фигура отсутствует на доске (срублена, фора, другое)
    pub first_move: bool        // Для пешек и рокировки
}

impl Piece {
    pub fn new(ptype: PieceType, color: Color, pos: Option<Position>) -> Self{
        Self{
            piece_type: ptype,
            color,
            pos,
            first_move:true,
        }
    }
}
    /*          ---  ДОСКА  ---          */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pieces: HashMap<Position, Piece>
}

impl Board {
    pub fn new() -> Self{
        Self{
            pieces: HashMap::new()
        }
    }
    pub fn init_by_default(&mut self){
        // Инициализация белых фигур
        self.pieces.insert(Position { file: 'a', rank: 1 }, Piece::new(PieceType::Rook, Color::White, Some(Position { file: 'a', rank: 1 })));
        self.pieces.insert(Position { file: 'b', rank: 1 }, Piece::new(PieceType::Knight, Color::White, Some(Position { file: 'b', rank: 1 })));
        self.pieces.insert(Position { file: 'c', rank: 1 }, Piece::new(PieceType::Bishop, Color::White, Some(Position { file: 'c', rank: 1 })));
        self.pieces.insert(Position { file: 'd', rank: 1 }, Piece::new(PieceType::Queen, Color::White, Some(Position { file: 'd', rank: 1 })));
        self.pieces.insert(Position { file: 'e', rank: 1 }, Piece::new(PieceType::King, Color::White, Some(Position { file: 'e', rank: 1 })));
        self.pieces.insert(Position { file: 'f', rank: 1 }, Piece::new(PieceType::Bishop, Color::White, Some(Position { file: 'f', rank: 1 })));
        self.pieces.insert(Position { file: 'g', rank: 1 }, Piece::new(PieceType::Knight, Color::White, Some(Position { file: 'g', rank: 1 })));
        self.pieces.insert(Position { file: 'h', rank: 1 }, Piece::new(PieceType::Rook, Color::White, Some(Position { file: 'h', rank: 1 })));

        for file in b'a'..=b'h' {
            self.pieces.insert(Position { file: file as char, rank: 2 }, Piece::new(PieceType::Pawn, Color::White, Some(Position { file: file as char, rank: 2 })));
        }

        // Инициализация черных фигур
        self.pieces.insert(Position { file: 'a', rank: 8 }, Piece::new(PieceType::Rook, Color::Black, Some(Position { file: 'a', rank: 8 })));
        self.pieces.insert(Position { file: 'b', rank: 8 }, Piece::new(PieceType::Knight, Color::Black, Some(Position { file: 'b', rank: 8 })));
        self.pieces.insert(Position { file: 'c', rank: 8 }, Piece::new(PieceType::Bishop, Color::Black, Some(Position { file: 'c', rank: 8 })));
        self.pieces.insert(Position { file: 'd', rank: 8 }, Piece::new(PieceType::Queen, Color::Black, Some(Position { file: 'd', rank: 8 })));
        self.pieces.insert(Position { file: 'e', rank: 8 }, Piece::new(PieceType::King, Color::Black, Some(Position { file: 'e', rank: 8 })));
        self.pieces.insert(Position { file: 'f', rank: 8 }, Piece::new(PieceType::Bishop, Color::Black, Some(Position { file: 'f', rank: 8 })));
        self.pieces.insert(Position { file: 'g', rank: 8 }, Piece::new(PieceType::Knight, Color::Black, Some(Position { file: 'g', rank: 8 })));
        self.pieces.insert(Position { file: 'h', rank: 8 }, Piece::new(PieceType::Rook, Color::Black, Some(Position { file: 'h', rank: 8 })));

        for file in b'a'..=b'h' {
            self.pieces.insert(Position { file: file as char, rank: 7 }, Piece::new(PieceType::Pawn, Color::Black, Some(Position { file: file as char, rank: 7 })));
        }

    }
    /// Вывод доски в консоль
    pub fn display(&self) {
        for rank in (1..=8).rev() {
            print!("{}   ", rank); // Номер строки
            for file in 'a'..='h' {
                let pos = Position { file, rank };
                match self.pieces.get(&pos) {
                    Some(piece) => {
                        // Выводим символ фигуры в зависимости от типа и цвета
                        let symbol = match (piece.piece_type, piece.color) {
                            (PieceType::King, Color::White) => "♔",
                            (PieceType::Queen, Color::White) => "♕",
                            (PieceType::Rook, Color::White) => "♖",
                            (PieceType::Bishop, Color::White) => "♗",
                            (PieceType::Knight, Color::White) => "♘",
                            (PieceType::Pawn, Color::White) => "♙",
                            (PieceType::King, Color::Black) => "♚",
                            (PieceType::Queen, Color::Black) => "♛",
                            (PieceType::Rook, Color::Black) => "♜",
                            (PieceType::Bishop, Color::Black) => "♝",
                            (PieceType::Knight, Color::Black) => "♞",
                            (PieceType::Pawn, Color::Black) => "♟",
                        };
                        print!("{} ", symbol);
                    }
                    None => print!(". "), // Пустая клетка
                }
            }
            println!(); // Переход на новую строку после каждой линии
        }
        println!("    a b c d e f g h"); // Буквы столбцов
    }
    /// получить фигуру по позиции (или ее отсутсвие)
    pub fn get_piece_at(&self, pos: &Position) -> Option<&Piece>{
        self.pieces.get(pos)
    }

    /// возвращает Some чтобы обработать рубку если на to стояла фигура
    pub fn move_piece(mut self, from: Position, to: Position) -> Option<Piece> {
        if let Some(mut piece) = self.pieces.remove(&from){
            piece.pos = Some(to); // <- Обновляем позицию фигуры

            //если на to была фигура, она сьедается автоматически
            return self.pieces.insert(to, piece);
        }
        None
    }

    /// Получить фигуру по типу и цвету
    pub fn find_piece(&self, piece_type: PieceType, color: Color) -> Option<Position> {
        for (pos, piece) in &self.pieces {
            if piece.piece_type == piece_type && piece.color == color {
                return Some(*pos);
            }
        }
        None
    }

    pub fn find_by_color(&self, color: Color) ->Vec<Piece> {
        let mut res = Vec::new();
        for (_, p) in &self.pieces {
            if p.color == color{
                res.push(*p);
            }
        }
        res
    }
}

    /*          ---  ХОДЫ  ---          */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveType {
    Normal,
    Capture,    // захват
    EnPassant,  // взятие на проходе
    Castling,   // рокировка
    Promotion,  // превращение пешки
    Check,       // шах
    Checkmate,   // мат
    Stalemate   // пат
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub piece: Piece,
    pub old_position: Position,
    pub new_position: Position,
    pub captured_piece: Option<PieceType>,
    pub move_type: MoveType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveHistory {
    pub moves: Vec<Move>,
}

    /*          ---  ПАРТИЯ  ---          */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gamemode {
    PCvsPC,
    PCvsPlayer,
    PlayerVsPlayer
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameResult {
    WhiteWin,
    BlackWin,
    Draw
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentGameStatus {
    pub board: Board,
    pub current_turn: Color, // Цвет игрока, чей ход
    pub is_gameover: Option<GameResult>,

}


/**TODO на будущее:
 * Таймер для игры на время из времени партии, времени прибавки за ход, времени на ход для движка
 * Какие-то эвристики и прочие шкалы
 */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub status: CurrentGameStatus,
    pub history: MoveHistory,
    gamemode: Gamemode,
    color_engine: Option<Color> // для режима PCvsPlayer

}


pub struct GameUtils;
// TODO вернуть все возможные ходы для того или иного цвета
// начать игру можно только по свершению первого хода (надо различать init и start)
