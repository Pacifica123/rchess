//! rules.rs Правила ходов

use crate::game::{Board, Piece, Position, PieceType, Color, Move};

pub struct Rules;
impl Rules{
    pub fn can_move(p: &Piece, to: Position, board: &Board) ->bool{
        let from = p.pos.unwrap();
        if let Some(piece) = board.get_piece_at(&to) {
            if piece.color == p.color {
                return false; // Нельзя рубить свою фигуру
            }
            if piece.piece_type == PieceType::King {

                return false;
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
        match board.find_piece(PieceType::King, mycolor) {
            Some(king_pos) => {
                let opponent_color = if mycolor == Color::White { Color::Black } else { Color::White };
                RulesUtils::is_square_attacked(board, opponent_color, &king_pos)
            }
            None => {
                // Технически уже нет короля — либо это ошибка, либо состояние «мат»
                // Решение: вернуть false, потому что «нет короля» ≈ «игра закончилась»
                // Или: panic! по смыслу (но лучше вернуть false и обработать мат/пат на уровне Game).
                false
            }
        }
    }

    pub fn is_insufficient_material(board: &Board) -> bool{
        let white_pieces = board.find_by_color(Color::White);
        let black_pieces = board.find_by_color(Color::Black);
        // Если у белых или у чёрных более одной фигуры → материал достаточен
        if white_pieces.len() != 1 || black_pieces.len() != 1 {
            return false;
        }
        // У каждого ровно одна фигура — проверяем, что это король
        white_pieces[0].piece_type == PieceType::King && black_pieces[0].piece_type == PieceType::King
    }

    /// Эта рокировка возможна?
    /// - `king_side = true`  → короткая рокировка (‘h’-ладья);
    /// - `king_side = false` → длинная рокировка (‘a’-ладья).
    pub fn can_castle(board: &Board, mycolor: Color, king_side: bool) -> bool {
        // 1) Если король уже под шахом – рокировка невозможна.
        if RulesUtils::is_in_check(board, mycolor) {
            return false;
        }

        // 2) Находим позицию короля и проверяем, что это его первый ход.
        let king_pos_opt = board.find_piece(PieceType::King, mycolor);
        let king_pos = match king_pos_opt {
            Some(p) => p,
            None => return false, // Короля вообще нет – странная ситуация, но рокировка невозможна.
        };
        let king_piece_opt = board.get_piece_at(&king_pos);
        let king_piece = match king_piece_opt {
            Some(p) => p,
            None => return false,
        };
        if !king_piece.first_move {
            return false;
        }

        // 3) Вычисляем позицию «рокировочной» ладьи: 
        //    если king_side=true, берём ‘h’, иначе ‘a’. Ранг зависит от цвета.
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

        // 4) Проверяем, что на этой клетке действительно стоит ладья нашего цвета и что она ещё не ходила.
        let castle_rook_opt = board.get_piece_at(&rook_pos);
        let castle_rook = match castle_rook_opt {
            Some(p) => p,
            None => return false,
        };
        if castle_rook.piece_type != PieceType::Rook || castle_rook.color != mycolor {
            return false;
        }
        if !castle_rook.first_move {
            return false;
        }

        // 5) Определяем цвет противника (нам нужно будет проверять «под шахом» для всех промежуточных клеток).
        let opponent_color = if mycolor == Color::White {
            Color::Black
        } else {
            Color::White
        };

        // 6) Перейдём к индексной арифметике для файлов 'a'..='h'.
        //    В ASCII 'a' = 97, 'b'=98, …, 'h'=104. 
        //    Зададим idx = (file_char as u8 - b'a') as i8, тогда 'a'→0, 'h'→7.
        let king_file_idx = (king_pos.file as u8).wrapping_sub(b'a') as i8;
        let rook_file_idx = (rook_pos.file as u8).wrapping_sub(b'a') as i8;
        let king_rank = king_pos.rank; // уже u8 (1..=8)

        // 7) Выясняем, в какую сторону «смотреть» при проходе короля:
        let file_direction: i8 = if king_side { 1 } else { -1 };

        // 8) Определяем конечный индекс для «последнего проверяемого поля» короля:
        //    если king_side=true, то это rook_file_idx - 1; если false – rook_file_idx + 1.
        let end_idx = if king_side {
            rook_file_idx - 1
        } else {
            rook_file_idx + 1
        };

        // **ВАЖНО**: Если король или ладья стоят так, что end_idx выходит за пределы 0..=7, 
        // значит рокировку в этом направлении делать бессмысленно (ладья не на своём месте).
        if !(0..=7).contains(&end_idx) {
            return false;
        }

        // 9) Теперь проверим промежуточные клетки по пути короля:
        //    current_idx = king_file_idx + file_direction
        let mut current_idx = king_file_idx + file_direction;

        // Пока мы ещё не дошли до “ячейки рядом с ладьёй” (end_idx),
        // проверяем каждую клетку, не атакует ли её противник.
        // Также убеждаемся, что сам индекс лежит в диапазоне 0..=7, иначе всё ломается.
        while current_idx != end_idx {
            if !(0..=7).contains(&current_idx) {
                // Условие за пределами доски – значит, король стоит не там, где должен, 
                // или мы как-то «перескочили» через край. Рокировка невозможна.
                return false;
            }

            // Собираем текущую позицию:
            let pos = Position {
                file: (b'a' + current_idx as u8) as char,
                rank: king_rank,
            };

            // Если это поле атакуется противником – рокировка невозможна:
            if RulesUtils::is_square_attacked(board, opponent_color, &pos) {
                return false;
            }

            current_idx += file_direction;
        }

        // 10) После выхода из цикла current_idx == end_idx. Это последняя позиция короля (перед ладьёй).
        //     Её тоже нужно проверить на «под шахом».
        if !(0..=7).contains(&current_idx) {
            return false;
        }
        let final_file = (b'a' + current_idx as u8) as char;
        let final_king_pos = Position {
            file: final_file,
            rank: king_rank,
        };
        if RulesUtils::is_square_attacked(board, opponent_color, &final_king_pos) {
            return false;
        }

        // 11) Если все проверки пройдены — рокировка возможна.
        true
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

    pub fn can_capture_en_passant(board: &Board, mycolor: Color, target_pos: &Position, last_move: Move) -> bool {
        // Определяем цвет противника
        let opponent_color = match mycolor {
            Color::White => Color::Black,
            Color::Black => Color::White,
        };

        // Получаем позицию пешки противника
        let pawn_pos = Position {
            file: target_pos.file,
            rank: if mycolor == Color::White { target_pos.rank - 1 } else { target_pos.rank + 1 },
        };

        // Проверяем, есть ли пешка противника на нужной позиции
        if let Some(opponent_pawn) = board.get_piece_at(&pawn_pos) {
            if opponent_pawn.color == opponent_color && opponent_pawn.piece_type == PieceType::Pawn {
                // Проверяем, сделала ли пешка противника двойной ход в последнем ходе
                if last_move.piece.piece_type == PieceType::Pawn && (last_move.new_position.rank).abs_diff(last_move.old_position.rank) == 2 {
                    return true;
                }
                // if opponent_pawn.last_move_was_double_step {
                //     return true; // Взятие на проходе возможно
                // }
            }
        }

        false // Взятие на проходе невозможно
    }
}
