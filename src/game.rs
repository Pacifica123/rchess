//! game.rs Основная логика игры


use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::parser::fen::{self, FenParseError};
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
    pub id: &'static str,
    pub piece_type: PieceType,
    pub color: Color,
    pub pos: Option<Position>,  // None - фигура отсутствует на доске (срублена, фора, другое)
    pub first_move: bool        // Для пешек и рокировки
}

impl Piece {
    pub fn new(ptype: PieceType, color: Color, pos: Option<Position>) -> Self{
        let id = Self::generate_id(ptype, color, pos);
        Self{
            id,
            piece_type: ptype,
            color,
            pos,
            first_move:true,
            
        }
    }
    fn generate_id(ptype: PieceType, color: Color, pos: Option<Position>) -> &'static str {
        let color_str = match color {
            Color::White => "w",
            Color::Black => "b",
        };

        let piece_str = match ptype {
            PieceType::King => "K",
            PieceType::Queen => "Q",
            PieceType::Rook => "R",
            PieceType::Bishop => "B",
            PieceType::Knight => "N",
            PieceType::Pawn => "P",
        };

        match pos {
            Some(position) => {
                const FORMAT: &str = "{}{}{}{}";
                Box::leak(format!("{} {} {} {} {}", FORMAT, color_str, piece_str, position.file, position.rank).into_boxed_str())
            }
            None => {
                const FORMAT: &str = "{} {}";
                Box::leak(format!("{} {} {}", FORMAT, color_str, piece_str).into_boxed_str())
            }
        }
    }
    // Для стартовых фигур 
    pub fn new_start(ptype: PieceType, color: Color, pos: Position) -> Self {
        let id = match (ptype, color, pos) {
            // Белые фигуры
            (PieceType::Rook, Color::White, Position { file: 'a', rank: 1 }) => "wRa1",
            (PieceType::Rook, Color::White, Position { file: 'h', rank: 1 }) => "wRh1",
            (PieceType::Knight, Color::White, Position { file: 'b', rank: 1 }) => "wNb1",
            (PieceType::Knight, Color::White, Position { file: 'g', rank: 1 }) => "wNg1",
            (PieceType::Bishop, Color::White, Position { file: 'c', rank: 1 }) => "wBc1",
            (PieceType::Bishop, Color::White, Position { file: 'f', rank: 1 }) => "wBf1",
            (PieceType::Queen, Color::White, Position { file: 'd', rank: 1 }) => "wQd1",
            (PieceType::King, Color::White, Position { file: 'e', rank: 1 }) => "wKe1",
            
            // Черные фигуры
            (PieceType::Rook, Color::Black, Position { file: 'a', rank: 8 }) => "bRa8",
            (PieceType::Rook, Color::Black, Position { file: 'h', rank: 8 }) => "bRh8",
            (PieceType::Knight, Color::Black, Position { file: 'b', rank: 8 }) => "bNb8",
            (PieceType::Knight, Color::Black, Position { file: 'g', rank: 8 }) => "bNg8",
            (PieceType::Bishop, Color::Black, Position { file: 'c', rank: 8 }) => "bBc8",
            (PieceType::Bishop, Color::Black, Position { file: 'f', rank: 8 }) => "bBf8",
            (PieceType::Queen, Color::Black, Position { file: 'd', rank: 8 }) => "bQd8",
            (PieceType::King, Color::Black, Position { file: 'e', rank: 8 }) => "bKe8",
            
            // Белые пешки
            (PieceType::Pawn, Color::White, Position { file: 'a', rank: 2 }) => "wPa2",
            (PieceType::Pawn, Color::White, Position { file: 'b', rank: 2 }) => "wPb2",
            (PieceType::Pawn, Color::White, Position { file: 'c', rank: 2 }) => "wPc2",
            (PieceType::Pawn, Color::White, Position { file: 'd', rank: 2 }) => "wPd2",
            (PieceType::Pawn, Color::White, Position { file: 'e', rank: 2 }) => "wPe2",
            (PieceType::Pawn, Color::White, Position { file: 'f', rank: 2 }) => "wPf2",
            (PieceType::Pawn, Color::White, Position { file: 'g', rank: 2 }) => "wPg2",
            (PieceType::Pawn, Color::White, Position { file: 'h', rank: 2 }) => "wPh2",
            
            // Черные пешки
            (PieceType::Pawn, Color::Black, Position { file: 'a', rank: 7 }) => "bPa7",
            (PieceType::Pawn, Color::Black, Position { file: 'b', rank: 7 }) => "bPb7",
            (PieceType::Pawn, Color::Black, Position { file: 'c', rank: 7 }) => "bPc7",
            (PieceType::Pawn, Color::Black, Position { file: 'd', rank: 7 }) => "bPd7",
            (PieceType::Pawn, Color::Black, Position { file: 'e', rank: 7 }) => "bPe7",
            (PieceType::Pawn, Color::Black, Position { file: 'f', rank: 7 }) => "bPf7",
            (PieceType::Pawn, Color::Black, Position { file: 'g', rank: 7 }) => "bPg7",
            (PieceType::Pawn, Color::Black, Position { file: 'h', rank: 7 }) => "bPh7",
            
            _ => panic!("Неизвестная стартовая фигура: {:?} {:?} {:?}", ptype, color, pos),
        };
        
        Self {
            id: id,
            piece_type: ptype,
            color,
            pos: Some(pos),
            first_move: true,
        }
    }
    
    /// Превращает эту пешку в новую фигуру
    pub fn promote(&mut self, new_type: PieceType) {
        if self.piece_type == PieceType::Pawn {
            self.piece_type = new_type;
            self.first_move = false;
            println!("Пешка  превращена в "); // todo : дебаг принта
        } else {
            println!("Только пешки могут превращаться!");
        }
    }
}
    /*          ---  ДОСКА  ---          */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub(crate) pieces: HashMap<Position, Piece>
}

impl Board {
    pub fn new() -> Self{
        Self{
            pieces: HashMap::new()
        }
    }
    pub fn init_by_default(&mut self) {
        // === БЕЛЫЕ ФИГУРЫ ===
        self.pieces.insert(Position { file: 'a', rank: 1 }, 
            Piece::new_start(PieceType::Rook, Color::White, Position { file: 'a', rank: 1 }));
        self.pieces.insert(Position { file: 'b', rank: 1 }, 
            Piece::new_start(PieceType::Knight, Color::White, Position { file: 'b', rank: 1 }));
        self.pieces.insert(Position { file: 'c', rank: 1 }, 
            Piece::new_start(PieceType::Bishop, Color::White, Position { file: 'c', rank: 1 }));
        self.pieces.insert(Position { file: 'd', rank: 1 }, 
            Piece::new_start(PieceType::Queen, Color::White, Position { file: 'd', rank: 1 }));
        self.pieces.insert(Position { file: 'e', rank: 1 }, 
            Piece::new_start(PieceType::King, Color::White, Position { file: 'e', rank: 1 }));
        self.pieces.insert(Position { file: 'f', rank: 1 }, 
            Piece::new_start(PieceType::Bishop, Color::White, Position { file: 'f', rank: 1 }));
        self.pieces.insert(Position { file: 'g', rank: 1 }, 
            Piece::new_start(PieceType::Knight, Color::White, Position { file: 'g', rank: 1 }));
        self.pieces.insert(Position { file: 'h', rank: 1 }, 
            Piece::new_start(PieceType::Rook, Color::White, Position { file: 'h', rank: 1 }));

        // Белые пешки (a2-h2)
        for file in b'a'..=b'h' {
            let file_char = file as char;
            self.pieces.insert(
                Position { file: file_char, rank: 2 },
                Piece::new_start(PieceType::Pawn, Color::White, Position { file: file_char, rank: 2 })
            );
        }

        // === ЧЕРНЫЕ ФИГУРЫ ===
        self.pieces.insert(Position { file: 'a', rank: 8 }, 
            Piece::new_start(PieceType::Rook, Color::Black, Position { file: 'a', rank: 8 }));
        self.pieces.insert(Position { file: 'b', rank: 8 }, 
            Piece::new_start(PieceType::Knight, Color::Black, Position { file: 'b', rank: 8 }));
        self.pieces.insert(Position { file: 'c', rank: 8 }, 
            Piece::new_start(PieceType::Bishop, Color::Black, Position { file: 'c', rank: 8 }));
        self.pieces.insert(Position { file: 'd', rank: 8 }, 
            Piece::new_start(PieceType::Queen, Color::Black, Position { file: 'd', rank: 8 }));
        self.pieces.insert(Position { file: 'e', rank: 8 }, 
            Piece::new_start(PieceType::King, Color::Black, Position { file: 'e', rank: 8 }));
        self.pieces.insert(Position { file: 'f', rank: 8 }, 
            Piece::new_start(PieceType::Bishop, Color::Black, Position { file: 'f', rank: 8 }));
        self.pieces.insert(Position { file: 'g', rank: 8 }, 
            Piece::new_start(PieceType::Knight, Color::Black, Position { file: 'g', rank: 8 }));
        self.pieces.insert(Position { file: 'h', rank: 8 }, 
            Piece::new_start(PieceType::Rook, Color::Black, Position { file: 'h', rank: 8 }));

        // Черные пешки (a7-h7)
        for file in b'a'..=b'h' {
            let file_char = file as char;
            self.pieces.insert(
                Position { file: file_char, rank: 7 },
                Piece::new_start(PieceType::Pawn, Color::Black, Position { file: file_char, rank: 7 })
            );
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
    
    pub fn to_full_fen(&self, current_turn: Color, castling: (bool,bool,bool,bool), en_passant: Option<Position>, halfmove: usize, fullmove: usize) -> String {
        let placement = self.to_fen(current_turn, castling, en_passant, halfmove, fullmove);
        let turn_str = if current_turn == Color::White { " w " } else { " b " };
        let castling_str = format!("{}{}{}{}", 
                                            if castling.0 { "K" } else { "" },
                                            if castling.1 { "Q" } else { "" },
                                            if castling.2 { "k" } else { "" },
                                            if castling.3 { "q" } else { "" });
        let ep_str = en_passant.map_or_else(|| "-".to_string(), |p| format!("{}{}", p.file, p.rank));
        format!("{}{} {} {} {} {}", 
                placement.split_whitespace().next().unwrap(), turn_str, 
                if castling_str.is_empty() { "-" } else { &castling_str }, ep_str, halfmove, fullmove)
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
    pub captured_piece_id: Option<&'static str>,
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

    /// перемещает фигуру, снимает захваченные фигуры, обновляет счетчики (halfmove/fullmove), устанавливает флаг en-passant и сохраняет минимальную информацию в структуру истории
    pub fn apply_move(&mut self, mv: Move){
        // 1. Проверка что ход возможный
        let all_possible_mvs = GameUtils::get_possible_moves(
            &self.status.board, 
            self.status.current_turn,
            self.history.moves.last().copied()

        );
        if !all_possible_mvs.contains(&mv) {
            println!("Недопустимый ход: не найден в возможных ходах."); // todo : для дебага печатать ход
            return; // просто не применяем
        }
        // если все же есть то применяем
        // 2. Применение хода на доску
        let captured_piece = self.status.board.move_piece(mv.old_position, mv.new_position);
        // если взятая фигура в ходе не совпадает со взятой фигурой на доске то что-то явно не так:
        if mv.captured_piece_id.is_some() && captured_piece.is_some() {
            if mv.captured_piece_id != Some(captured_piece.unwrap().id) {
                println!("Произошла какая-то хрень..");
                // Откат: восстанавливаем фигуру обратно
                // if let Some(piece) = self.status.board.get_piece_at(actual_id) {
                //     self.status.board.place_piece(mv.new_position, piece);
                //     self.status.board.place_piece(mv.old_position, None);
                // }
                return;
            } else {//if expected_id != 0 {
                // println!("Ожидался захват фигуры {}, но фигура не взята.", expected_id);
                println!("Ожидался захват фигуры, но фигура не взята.");
                // Откат...
                return;
            }
        }
        // Специальная обработка en passant
        self.status.en_passant_target = None; // сбрасываем по умолчанию
        if mv.move_type == MoveType::EnPassant {
            // Удаляем пешку противника за целью en passant
            let opponent_pawn_pos = if self.status.current_turn == Color::White {
                Position { file: mv.new_position.file, rank: mv.new_position.rank - 1 }
            } else {
                Position { file: mv.new_position.file, rank: mv.new_position.rank + 1 }
            };
            self.status.board.pieces.remove(&opponent_pawn_pos);
        } else if mv.piece.piece_type == PieceType::Pawn && 
                  ((mv.old_position.rank == 2 && mv.new_position.rank == 4 && mv.piece.color == Color::White) ||
                   (mv.old_position.rank == 7 && mv.new_position.rank == 5 && mv.piece.color == Color::Black)) {
            // Двойной шаг пешки -> создаем цель для en passant
            self.status.en_passant_target = Some(Position {
                file: mv.new_position.file,
                rank: if mv.piece.color == Color::White { 3 } else { 6 }
            });
        }
        // Специальная обработка рокировки
        if mv.move_type == MoveType::Castling {
            let (rook_from, rook_to) = if mv.new_position.file == 'g' {
                // Королевская рокировка
                if self.status.current_turn == Color::White {
                    (Position { file: 'h', rank: 1 }, Position { file: 'f', rank: 1 })
                } else {
                    (Position { file: 'h', rank: 8 }, Position { file: 'f', rank: 8 })
                }
            } else {
                // Ферзовая рокировка
                if self.status.current_turn == Color::White {
                    (Position { file: 'a', rank: 1 }, Position { file: 'd', rank: 1 })
                } else {
                    (Position { file: 'a', rank: 8 }, Position { file: 'd', rank: 8 })
                }
            };
            self.status.board.move_piece(rook_from, rook_to);
        }
        // Обновляем права на рокировку
        let (wk, wq, bk, bq) = self.status.castling_rights;
        let new_castling_rights = match mv.piece.piece_type {
            PieceType::King if mv.piece.color == Color::White => (false, false, bk, bq),
            PieceType::Rook if mv.old_position.file == 'h' && mv.piece.color == Color::White => (false, wq, bk, bq),
            PieceType::Rook if mv.old_position.file == 'a' && mv.piece.color == Color::White => (wk, false, bk, bq),
            PieceType::King if mv.piece.color == Color::Black => (wk, wq, false, false),
            PieceType::Rook if mv.old_position.file == 'h' && mv.piece.color == Color::Black => (wk, wq, false, bq),
            PieceType::Rook if mv.old_position.file == 'a' && mv.piece.color == Color::Black => (wk, wq, bk, false),
            _ => (wk, wq, bk, bq),
        };
        self.status.castling_rights = new_castling_rights;
        // Обновляем first_move для перемещенной фигуры
        if let Some(piece) = self.status.board.pieces.get_mut(&mv.new_position) {
            piece.first_move = false;
        }
        
        // Обновить цвет
        self.status.current_turn = match self.status.current_turn {
            Color::White => Color::Black,
            Color::Black => Color::White,
        };
        // 3. Запись в историю ходов
        //      Обновляем счетчики FEN
        if mv.move_type == MoveType::Capture || mv.move_type == MoveType::EnPassant || 
           mv.piece.piece_type == PieceType::Pawn {
            self.status.halfmove_clock = 0;
        } else {
            self.status.halfmove_clock += 1;
        }

        if self.status.current_turn == Color::Black {
            self.status.fullmove_number += 1;
        }
        self.history.moves.push(mv)
    }

    pub fn from_fen(fen: &str, gamemode: Gamemode) -> Result<Self, FenParseError> {
        let status = fen::parse_fen(fen)?;
        Ok(Self {
            status,
            history: MoveHistory::new(),
            gamemode,
            color_engine: None,
        })
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
                            Some(captured)
                        } else {
                            None
                        };

                        let mut move_type = match captured_piece {
                            Some(captured) => MoveType::Capture,  // Есть захваченная фигура
                            None => MoveType::Normal,             // Нет захвата
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
                                            captured_piece_id: captured_piece.map(|captured| captured.id),
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
