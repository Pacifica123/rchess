use crate::game::{Board, Piece, Position, PieceType, Color};

pub struct Rules;
impl Rules{
    pub fn can_move(p: &Piece, to: Position, board: &Board) ->bool{
        let from = p.pos.unwrap();
        if let Some(piece) = board.get_piece_at(&to) {
            if piece.color == p.color {
                return false; // Нельзя рубить свою фигуру
            }
        }
        match p.piece_type {
            PieceType::Rook => Rules::can_move_rook(from, to, board),
            PieceType::Bishop => Rules::can_move_bishop(from, to, board),
            PieceType::Knight => Rules::can_move_knight(from, to),
            PieceType::Pawn => Rules::can_move_pawn(p.color, from, to, board),
            PieceType::King => Rules::can_move_king(p.color, from, to, board),
            PieceType::Queen => Rules::can_move_queen(from, to, board)
        }
    }
    // --------------------
    /// Ход Ладьей
    fn can_move_rook(from: Position, to: Position, board: &Board) ->bool{
        if from.file != to.file && from.rank != to.rank {
            return false; // диагональный ход
        }

        if from.file == to.file {
            // Движение по вертикали
            let (start_rank, end_rank) = (from.rank.min(to.rank), from.rank.max(to.rank));
            for rank in (start_rank + 1)..end_rank {
                let pos = Position { file: from.file, rank };
                if board.get_piece_at(&pos).is_some() {
                    return false;
                }
            }
        } else {
            // Движение по горизонтали
            let (start_file, end_file) = if from.file < to.file { (from.file, to.file) } else { (to.file, from.file) };
            for file_code in ((start_file as u8) + 1)..(end_file as u8) { // Итерируемся по u8
                let file = file_code as char; // Преобразуем обратно в char
                let pos = Position { file, rank: from.rank };
                if board.get_piece_at(&pos).is_some() {
                    return false;
                }
            }
        }

        true
    }
    // --------------------
    /// Ход Слоном
    fn can_move_bishop(from: Position, to: Position, board: &Board) -> bool{
        let file_diff = (from.file as i8 - to.file as i8).abs();
        let rank_diff = (from.rank as i8 - to.rank as i8).abs();

        if file_diff != rank_diff {
            return false;
        }

        let file_step = if to.file > from.file { 1 } else { -1 };
        let rank_step = if to.rank > from.rank { 1 } else { -1 };

        let mut file = from.file as i8;
        let mut rank = from.rank as i8;

        while file != to.file as i8 { // Используем while, чтобы избежать бесконечного цикла
            file += file_step;
            rank += rank_step;

            // Сначала проверяем, достигли ли мы целевой позиции.  Важно, чтобы это было *перед* проверкой на наличие фигуры,
            // но *после* обновления file и rank
            if file == to.file as i8 && rank == to.rank as i8 {
                // Теперь проверяем, занята ли целевая позиция
                if board.get_piece_at(&Position { file: file as u8 as char, rank: rank as u8 }).is_some() {
                    return false; // Целевая позиция занята
                }
                return true; // Только теперь мы действительно дошли и позиция свободна
            }

            // Проверяем, есть ли фигура на пути (исключая начальную и конечную позиции)
            if board.get_piece_at(&Position { file: file as u8 as char, rank: rank as u8 }).is_some() {
                return false; // Фигура блокирует путь
            }
        }

        false // Если цикл завершился без достижения целевой позиции, что-то пошло не так.
    }
    // --------------------
    /// Ход Конем
    fn can_move_knight(from: Position, to: Position) -> bool {
        let file_diff = (from.file as i8 - to.file as i8).abs();
        let rank_diff = (from.rank as i8 - to.rank as i8).abs();

        (file_diff == 2 && rank_diff == 1) || (file_diff == 1 && rank_diff == 2)
    }
    // --------------------
    /// Ход Пешки
    fn can_move_pawn(color: Color, from: Position, to: Position, board: &Board) -> bool {
        let forward = if color == Color::White { 1 } else { -1 };

        if from.file == to.file { // Простой ход вперед
            if to.rank as i8 == from.rank as i8 + forward {
                return board.get_piece_at(&to).is_none();
            }
            // Проверяем ход на две клетки
            if (from.rank == 2 && color == Color::White) || (from.rank == 7 && color == Color::Black) {
                if to.rank as i8 == from.rank as i8 + 2 * forward {
                    let middle = Position { file: from.file, rank: (from.rank as i8 + forward) as u8 };
                    return board.get_piece_at(&to).is_none() && board.get_piece_at(&middle).is_none();
                }
            }
        } else if (from.file as i8 - to.file as i8).abs() == 1 { // Рубка по диагонали
            if to.rank as i8 == from.rank as i8 + forward {
                if let Some(target) = board.get_piece_at(&to) {
                    return target.color != color; // Пешка не может рубить свои фигуры
                }
            }
        }

        false
    }
    // --------------------
    /// Ход Ферзя
    fn can_move_queen(from: Position, to: Position, board: &Board) -> bool {
        // Ферзь ходит как слон или ладья
        Rules::can_move_rook(from, to, board) || Rules::can_move_bishop(from, to, board)
    }
    // --------------------
    /// Ход Короля
    fn can_move_king(mycolor: Color, from: Position, to: Position, board: &Board) -> bool {
        let file_diff = (from.file as i8 - to.file as i8).abs();
        let rank_diff = (from.rank as i8 - to.rank as i8).abs();

        // Проверяем, что король двигается на одну клетку в любом направлении
        if file_diff <= 1 && rank_diff <= 1 && (file_diff != 0 || rank_diff != 0) {
            // TODO:добавить проверку на то что под шах лезть нельзя
            // Проверяем, что целевая клетка не занята фигурой того же цвета
            if let Some(piece) = board.get_piece_at(&to) {
                return piece.color != mycolor;
            }
            return true;

        }
        false
    }
}


pub struct RulesUtils;
impl RulesUtils {
    /// Кароль под шахом?
    pub fn is_in_check(board: &Board, mycolor: Color) ->bool {
        let king_pos = board.find_piece(PieceType::King, mycolor).unwrap();
        let opponent_color = match mycolor {
            Color::White => Color::Black,
            Color::Black => Color::White
        };
        RulesUtils::is_square_attacked(board, opponent_color, &king_pos)
        // for piece in board.find_by_color(opponent_color) {
        //     if Rules::can_move(&piece, king_pos.unwrap(), board) {
        //         return true;
        //     }
        // }
        // false
    }
    /// Эта рокировка возможна?
    /// - `king_side` = true если хотите по королевскому флангу (короткая)
    pub fn can_castle(board: &Board, mycolor: Color, king_side: bool) ->bool{
        // фундаментальная проверка шаха короля (остальные проверки шаха в вшиты в Move)
        if RulesUtils::is_in_check(board, mycolor) { return false; }
        // фундаментальная проверка что ладьи есть и участники рокировки не делали ходы ранее
        let king_pos = board.find_piece(PieceType::King, mycolor);
        let king = board.get_piece_at(&king_pos.unwrap());
        if !king.unwrap().first_move{
            return false
        }

        // Определяем позицию ладьи в зависимости от стороны и цвета
        let rook_pos = if king_side {
            if mycolor == Color::White {
                Position { file: 'h', rank: 1 }
            } else {
                Position { file: 'h', rank: 8 }
            }
        } else {
            if mycolor == Color::White {
                Position { file: 'a', rank: 1 }
            } else {
                Position { file: 'a', rank: 8 }
            }
        };

        // Проверяем, есть ли ладья и сделала ли она первый ход
        let castle_rook = board.get_piece_at(&rook_pos);
        match castle_rook {
            Some(p) => {
                if !p.first_move { return false }
                // TODO:
                /*
                 * Король не должен проходить через поля, которые находятся под атакой
                 * Король не должен завершать свой ход на поле, которое находится под атакой
                 */
                let opponent_color = match mycolor {
                    Color::White => Color::Black,
                    Color::Black => Color::White
                };


                // Определяем направление движения для рокировки
                let file_direction: i8 = if king_side { 1 } else { -1 };

                // Определяем конечную позицию для проверки (за одну клетку до ладьи)
                let end_file = if king_side {
                    (rook_pos.file as u8 - 1) as char
                } else {
                    (rook_pos.file as u8 + 1) as char
                };

                // тут я беспокоюсь за + file_direction...
                let mut current_file = (king_pos.unwrap().file as u8 + file_direction as u8) as char;

                // Проверяем все поля на пути рокировки
                while current_file != end_file {
                    let pos = Position {
                        file: current_file,
                        rank: king_pos.unwrap().rank,
                    };

                    // Проверяем, находится ли поле под атакой противника
                    if RulesUtils::is_square_attacked(board, opponent_color, &pos) {
                        return false;
                    }

                    // Переходим к следующему полю
                    current_file = (current_file as u8 + file_direction as u8) as char;
                }

                // Дополнительная проверка конечной позиции короля
                let final_king_pos = Position {
                    file: end_file,
                    rank: king_pos.unwrap().rank,
                };
                if RulesUtils::is_square_attacked(board, opponent_color, &final_king_pos) {
                    return false;
                }

                true // Если все проверки пройдены, рокировка возможна
            }
            None => {false}
        }


    }

    /// Вспомогательная функция для проверки, находится ли поле под атакой
    fn is_square_attacked(board: &Board, opponent_color: Color, pos: &Position) -> bool {
        for piece in board.find_by_color(opponent_color) {
            if Rules::can_move(&piece, *pos, board) {
                return true;
            }
        }
        false
    }

    // TODO: взятие на проходе
}
