//! engine.rs Анализ, эвристики, дерево поиска и прочее
//! TODO: Evaluation, Heuristics, SearchDepth, TimeControl, SearchTree, Зеттатаблицы(?)

use crate::game::{Board, Color, Game, Move, GameUtils};

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
        // Метод для совершения хода в игре
        let board = &game.status.board;
        let color = game.status.current_turn;
        let best_move = self.searcher.find_best_move(board, color, game.history.moves.last().copied());

        if let Some(move_) = best_move {
            print!("Ход найден! - ");
            crate::utils::print_move(&move_);
        }
        // if let Some(move_) = best_move {
        //     // Применить ход к доске и обновить состояние игры
        //     unimplemented!()
        // }
    }
}
