//! game.rs Основная логика игры


use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::rules;

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
    pub fn move_piece(&mut self, from: Position, to: Position) -> Option<Piece> {
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

    pub fn to_fen(&self, 
        current_turn: Color, 
        castling_rights: (bool,bool,bool,bool), 
        en_passant_target: Option<Position>, 
        halfmove_clock: usize, 
        fullmove_number: usize) -> String {
        let mut rows: Vec<String> = Vec::new();
        for rank in (1..=8).rev() {
            let mut empty_count = 0;
            let mut row_str = String::new();
            for file in 'a'..='h' {
                let pos = Position { file, rank };
                if let Some(piece) = self.get_piece_at(&pos) {
                    if empty_count > 0 {
                        row_str.push_str(&empty_count.to_string());
                        empty_count = 0;
                    }
                    let c = match (piece.piece_type, piece.color) {
                        (PieceType::King, Color::White)   => 'K',
                        (PieceType::Queen, Color::White)  => 'Q',
                        (PieceType::Rook, Color::White)   => 'R',
                        (PieceType::Bishop, Color::White) => 'B',
                        (PieceType::Knight, Color::White) => 'N',
                        (PieceType::Pawn, Color::White)   => 'P',
                        (PieceType::King, Color::Black)   => 'k',
                        (PieceType::Queen, Color::Black)  => 'q',
                        (PieceType::Rook, Color::Black)   => 'r',
                        (PieceType::Bishop, Color::Black) => 'b',
                        (PieceType::Knight, Color::Black) => 'n',
                        (PieceType::Pawn, Color::Black)   => 'p',
                    };
                    row_str.push(c);
                } else {
                    empty_count += 1;
                }
            }
            if empty_count > 0 {
                row_str.push_str(&empty_count.to_string());
            }
            rows.push(row_str);
        }
        let placement = rows.join("/");              // пример: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR"
        let turn_str = if current_turn == Color::White { " w" } else { " b" };
        format!("{}{}", placement, turn_str)
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

impl MoveHistory {
    pub fn new() -> Self{
        Self{moves: Vec::new()}
    }
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
    pub halfmove_clock: usize,
    /// Сколько полных ходов сыграно (нужно, если вы хотите уметь экспортировать FEN полностью)
    pub fullmove_number: usize,
    /// Права на рокировку: (WhiteKingSide, WhiteQueenSide, BlackKingSide, BlackQueenSide)
    pub castling_rights: (bool, bool, bool, bool),
    /// Если на предыдущем ходе пешка сделала двойной шаг, здесь хранится поле, 
    /// которое может быть взято «на проходе»; иначе – None
    pub en_passant_target: Option<Position>,
    /// История позиционных ключей (хешей или FEN‐строк) после каждого полухода
    /// (для обнаружения троекратного повторения)
    pub history_fens: Vec<String>,
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

impl Game {
    /// Инициализация новой партии
    pub fn new(gamemode: Gamemode) -> Self {
        let board = Board::new(); // Предполагается, что Board имеет метод new для инициализации стартовой позиции
        let current_turn = Color::White; // Стартовый ход белых
        let status = CurrentGameStatus {
            board,
            current_turn,
            is_gameover: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            castling_rights: (true, true, true, true),
            en_passant_target: None,
            history_fens: Vec::new(),
        };
        let history = MoveHistory::new(); // Предполагается, что MoveHistory имеет метод new для инициализации пустой истории ходов

        let color_engine = match gamemode {
            Gamemode::PCvsPlayer => Some(Color::White), // По умолчанию движок играет за белых
            _ => None,
        };

        Self {
            status,
            history,
            gamemode,
            color_engine,
        }
    }
    /// Инициализация новой партии с заданной доской
    pub fn new_with_board(gamemode: Gamemode, board: Board) -> Self {
        let current_turn = Color::White; // Стартовый ход белых
        let status = CurrentGameStatus {
            board,
            current_turn,
            is_gameover: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            castling_rights: (true, true, true, true),
            en_passant_target: None,
            history_fens: Vec::new(),
        };
        let history = MoveHistory::new();

        let color_engine = match gamemode {
            Gamemode::PCvsPlayer => Some(Color::White),
            _ => None,
        };

        Self {
            status,
            history,
            gamemode,
            color_engine,
        }
    }

    /// Старт партии (может быть использован для дополнительных действий перед началом игры)
    pub fn start(&mut self) {
        // Здесь можно добавить дополнительные действия перед началом игры, если необходимо
        println!("Игра началась!");
    }


}


pub struct GameUtils;
impl GameUtils {
    pub fn get_possible_moves(board: &Board, color: Color, last_move: Option<Move>) -> Vec<Move> {
        let mut possible_moves = Vec::new();

        // Получить все фигуры заданного цвета
        let pieces: Vec<Piece> = board
        .pieces
        .values()
        .filter(|piece| piece.color == color)
        .cloned()
        .collect();

        for piece in pieces {
            let from = piece.pos.unwrap();

            // Проверить все 64 клетки доски
            for file in 'a'..='h' {
                for rank in 1..=8 {
                    let to = Position { file, rank };

                    if rules::Rules::can_move(&piece, to, board) {
                        let captured_piece = if let Some(captured) = board.get_piece_at(&to) {
                            Some(captured.piece_type)
                        } else {
                            None
                        };

                        let mut move_type = match captured_piece {
                            Some(_) => MoveType::Capture,
                            None => MoveType::Normal,
                        };

                        // Дополнительно проверить на рокировку
                        if piece.piece_type == PieceType::King {
                            if rules::RulesUtils::can_castle(board, color, true) && (to.file == 'g' && from.file == 'e') {
                                move_type = MoveType::Castling;
                            } else if rules::RulesUtils::can_castle(board, color, false) && (to.file == 'c' && from.file == 'e') {
                                move_type = MoveType::Castling;
                            }
                        }

                        // Проверка на взятие на проходе
                        if piece.piece_type == PieceType::Pawn && last_move.is_some() {
                            if rules::RulesUtils::can_capture_en_passant(board, color, &to, last_move.unwrap()) {
                                move_type = MoveType::EnPassant;
                            }
                        }

                        // Проверка на превращение пешки
                        if piece.piece_type == PieceType::Pawn && to.rank == (if color == Color::White { 8 } else { 1 }) {
                            move_type = MoveType::Promotion;
                        }

                        possible_moves.push(Move {
                            piece: piece.clone(),
                                            old_position: from,
                                            new_position: to,
                                            captured_piece,
                                            move_type,
                        });
                    }
                }
            }
        }

        possible_moves
    }

}

// TODO
// начать игру можно только по свершению первого хода (надо различать init и start)
