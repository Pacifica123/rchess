//! engine.rs Анализ, эвристики, дерево поиска и прочее
//! TODO: Evaluation, Heuristics, SearchDepth, TimeControl, SearchTree, Зеттатаблицы(?)

use crate::{game::{Board, Color, Game, GameResult, GameUtils, Move, Piece, PieceType}, rules::RulesUtils};

use rand::Rng;

/// Структура для оценки позиции
#[derive(Debug)]
pub struct Evaluator {
    // Параметры оценки, такие как веса фигур и другие коэффициенты
    pub piece_values: [i32; 6], // Значения для пешки, коня, слона, ладьи, ферзя и короля
    pub pawn_structure_values: [i32; 8], // Значения для структуры пешек
}

impl Evaluator {
    pub fn new() -> Self {
        // Инициализация оценщика с стандартными весами
        Self {
            piece_values: [100, 300, 300, 500, 900, 10000], // Примерные значения
            pawn_structure_values: [0; 8], // Значения для структуры пешек
        }
    }

    pub fn evaluate(&self, board: &Board) -> i32 {
        // Метод для оценки позиции на доске
        // Реализация будет включать в себя анализ фигур на доске и их позиций
        unimplemented!()
    }
}

// Структура для поиска ходов
#[derive(Debug)]
pub struct Searcher {
    pub evaluator: Evaluator,
    pub max_depth: u8, // Максимальная глубина поиска
}

impl Searcher {
    pub fn new(evaluator: Evaluator, max_depth: u8) -> Self {
        Self { evaluator, max_depth }
    }

    pub fn find_best_move(&self, board: &Board, color: Color, last_move: Option<Move>) -> Option<Move> {
        // Метод для нахождения лучшего хода с помощью алгоритма поиска (например, Minimax или Alpha-Beta)
        // пока что просто рандом
        let possible_moves = GameUtils::get_possible_moves(board, color, last_move);
        if !possible_moves.is_empty() {
            let mut rng = rand::thread_rng();
            let random_index = rng.gen_range(0..possible_moves.len());
            let random_move = possible_moves[random_index];

            Some(random_move)
        } else {
            None // Возвращаем None, если ходов нет
        }

    }
}

// Структура для таблицы трансформаций
#[derive(Debug)]
pub struct TranspositionTable {
    pub cache: std::collections::HashMap<u64, i32>, // Кэш для хранения результатов оценок
}

impl TranspositionTable {
    pub fn new() -> Self {
        Self {
            cache: std::collections::HashMap::new(),
        }
    }

    pub fn store(&mut self, zobrist_hash: u64, score: i32) {
        self.cache.insert(zobrist_hash, score);
    }

    pub fn retrieve(&self, zobrist_hash: u64) -> Option<i32> {
        self.cache.get(&zobrist_hash).cloned()
    }
}

// Структура для управления временем
#[derive(Debug)]
pub struct TimeManager {
    pub time_per_move: u32, // Время на ход в миллисекундах
}

impl TimeManager {
    pub fn new(time_per_move: u32) -> Self {
        Self { time_per_move }
    }

    pub fn manage_time(&self) {
        // Метод для контроля времени, затраченного на поиск ходов
        unimplemented!()
    }
}

// Основная структура движка
#[derive(Debug)]
pub struct Engine {
    pub searcher: Searcher,
    pub transposition_table: TranspositionTable,
    pub time_manager: TimeManager,
}

impl Engine {
    pub fn new(max_depth: u8, time_per_move: u32) -> Self {
        let evaluator = Evaluator::new();
        let searcher = Searcher::new(evaluator, max_depth);
        let transposition_table = TranspositionTable::new();
        let time_manager = TimeManager::new(time_per_move);

        Self {
            searcher,
            transposition_table,
            time_manager,
        }
    }

pub fn make_move(&mut self, game: &mut Game) {
    if game.status.is_gameover.is_some() {
        println!("игра окончена : {:?}", game.status.is_gameover.as_ref());

        return;
    }
    let move_opt = {
        let board = &game.status.board;
        let color = game.status.current_turn;
        let last_move = game.history.moves.last().copied();
        self.searcher.find_best_move(board, color, last_move)
    };

    if let Some(mv) = move_opt {
        print!("Ход найден! - ");
        crate::utils::print_move(&mv);

        // Применяем саму «физику» перемещения фигуры на доске:
        //      move_piece возвращает Option<Piece> – снятую (срубленную) при ходе фигуру, 
        //      но нам это нужно только для внутреннего учёта (mv.captured_piece уже хранит тип сбитой фигуры, 
        //      если она была).
        let _maybe_captured: Option<Piece> = game.status.board.move_piece(mv.old_position, mv.new_position);
        game.history.moves.push(mv);
        // обновить halfmove_clock
        if mv.piece.piece_type == PieceType::Pawn || mv.captured_piece.is_some() {
            game.status.halfmove_clock = 0;
        } else {
            game.status.halfmove_clock += 1;
        }
        if game.status.current_turn == Color::Black {
            game.status.fullmove_number += 1;
        }

        // 50 ходов
        if game.status.halfmove_clock >= 100 {
            game.status.is_gameover = Some(GameResult::Draw);
            return;
        }
        // остались только короли
        if RulesUtils::is_insufficient_material(&game.status.board) {
            game.status.is_gameover = Some(GameResult::Draw);
            return;
        }
        // смена стороны
        game.status.current_turn = match game.status.current_turn {
            Color::White => Color::Black,
            Color::Black => Color::White,
        };
        // Запись FEN 
        let fen = game.status.board.to_fen(
            game.status.current_turn,
            game.status.castling_rights,
            game.status.en_passant_target,
            game.status.halfmove_clock,
            game.status.fullmove_number,
        );
        game.status.history_fens.push(fen.clone());
        // троекратное повторение
        let reps = game.status.history_fens.iter().filter(|&s| s == &fen).count();
        if reps >= 3 {
            game.status.is_gameover = Some(GameResult::Draw);
            return;
        }
        // проверяем, не закончилась ли партия
        let side_to_move = game.status.current_turn;
        let in_check = RulesUtils::is_in_check(&game.status.board, side_to_move);
        let legal_moves = GameUtils::get_possible_moves(
                &game.status.board,
                side_to_move,
                game.history.moves.last().copied(),
        );
        if legal_moves.is_empty() {
            if in_check {
                // 3.1.1. Король под шахом и ходов нет → это шах‐мат.
                let result = match side_to_move {
                    Color::White => GameResult::BlackWin,
                    Color::Black => GameResult::WhiteWin,
                };
                game.status.is_gameover = Some(result);
            } else {
                // 3.1.2. Король не под шахом, но ходов нет → пат (ничья).
                game.status.is_gameover = Some(GameResult::Draw);
            }
        }
        // Если есть легальные ходы — ничего не делаем: партия продолжается.
    }  

        // Если move_opt == None: значит движок вообще не нашёл ни одного хода на старом цвете,
        // мы сюда не заходим и, фактически, не меняем ничего в состоянии. В такой ситуации
        // можно (опционально) выставить is_gameover = Draw или вызвать отдельную логику,
        // но обычно движок «ни одного хода» возвращает только тогда, когда уже был мат/пат 
        // на предыдущем ходу и мы сюда просто не должны были попасть.
}



}
