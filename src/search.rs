use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use crate::chess::{file_of, rank_of, ChessMove, Color, PieceKind, Position};

const INFINITY: i32 = 1_000_000;
const MATE_SCORE: i32 = 900_000;
const TT_SCORE_BIAS: i32 = 1_050_000;
const TT_SCORE_BITS: u64 = (1 << 22) - 1;
const TT_EXACT: u8 = 0;
const TT_LOWER: u8 = 1;
const TT_UPPER: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchSettings {
    pub deterministic_multithread: bool,
    pub max_threads: usize,
    pub granularity: usize,
    pub hash_mb: usize,
}

impl Default for SearchSettings {
    fn default() -> Self {
        let max_threads = std::thread::available_parallelism()
            .map(|threads| threads.get().clamp(1, 64))
            .unwrap_or(1);
        Self {
            deterministic_multithread: max_threads > 1,
            max_threads,
            granularity: 1,
            hash_mb: 64,
        }
    }
}

impl SearchSettings {
    pub fn normalized(mut self) -> Self {
        self.max_threads = self.max_threads.clamp(1, 64);
        self.granularity = self.granularity.clamp(1, 64);
        self.hash_mb = self.hash_mb.clamp(1, 4096);
        self
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    max_depth: u8,
    searched_nodes: u64,
    settings: SearchSettings,
    tt: Arc<TranspositionTable>,
}

impl Engine {
    pub fn new(max_depth: u8) -> Self {
        let settings = SearchSettings::default().normalized();
        Self {
            max_depth: max_depth.max(1),
            searched_nodes: 0,
            tt: Arc::new(TranspositionTable::new(settings.hash_mb)),
            settings,
        }
    }

    pub fn set_depth(&mut self, max_depth: u8) {
        self.max_depth = max_depth.max(1);
    }

    pub fn settings(&self) -> SearchSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: SearchSettings) {
        let next = settings.normalized();
        if next.hash_mb != self.settings.hash_mb {
            self.tt = Arc::new(TranspositionTable::new(next.hash_mb));
        }
        self.settings = next;
    }

    pub fn set_deterministic_multithread(&mut self, value: bool) {
        let mut settings = self.settings;
        settings.deterministic_multithread = value;
        self.set_settings(settings);
    }

    pub fn set_max_threads(&mut self, value: usize) {
        let mut settings = self.settings;
        settings.max_threads = value;
        self.set_settings(settings);
    }

    pub fn set_granularity(&mut self, value: usize) {
        let mut settings = self.settings;
        settings.granularity = value;
        self.set_settings(settings);
    }

    pub fn set_hash_mb(&mut self, value: usize) {
        let mut settings = self.settings;
        settings.hash_mb = value;
        self.set_settings(settings);
    }

    pub fn searched_nodes(&self) -> u64 {
        self.searched_nodes
    }

    pub fn transposition_entries(&self) -> usize {
        self.tt.len()
    }

    pub fn best_move(&mut self, position: &Position) -> Option<ChessMove> {
        self.best_move_with_score(position).map(|(chess_move, _score)| chess_move)
    }

    pub fn best_move_with_score(&mut self, position: &Position) -> Option<(ChessMove, i32)> {
        self.searched_nodes = 0;
        let mut moves = position.legal_moves();
        if moves.is_empty() {
            return None;
        }
        order_moves(position, &mut moves);
        let age = self.tt.next_age();

        if self.settings.deterministic_multithread
            && self.settings.max_threads > 1
            && moves.len() >= self.settings.granularity.max(1) * 2
        {
            return self.best_move_root_split(position, moves, age);
        }

        let mut worker = SearchWorker::new(self.tt.clone(), age);
        let mut best_move = moves[0];
        let mut best_score = -INFINITY;
        let mut alpha = -INFINITY;
        let beta = INFINITY;

        for chess_move in moves {
            let mut next = position.clone();
            next.apply_unchecked(chess_move).ok()?;
            let score = -worker.negamax(&next, self.max_depth.saturating_sub(1), -beta, -alpha, 1);
            if score > best_score {
                best_score = score;
                best_move = chess_move;
            }
            alpha = alpha.max(score);
        }
        self.searched_nodes = worker.searched_nodes;
        Some((best_move, best_score))
    }

    fn best_move_root_split(&mut self, position: &Position, moves: Vec<ChessMove>, age: u8) -> Option<(ChessMove, i32)> {
        let indexed_moves: Vec<(usize, ChessMove)> = moves.iter().copied().enumerate().collect();
        let granularity = self.settings.granularity.max(1);
        let tasks: Vec<Vec<(usize, ChessMove)>> = indexed_moves
            .chunks(granularity)
            .map(|chunk| chunk.to_vec())
            .collect();
        let thread_count = self.settings.max_threads.min(tasks.len()).max(1);
        let mut all_results: Vec<(usize, ChessMove, i32)> = Vec::with_capacity(moves.len());
        let mut total_nodes = 0_u64;
        let tt = self.tt.clone();
        let depth = self.max_depth.saturating_sub(1);

        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(thread_count);
            for thread_id in 0..thread_count {
                let assigned_tasks: Vec<Vec<(usize, ChessMove)>> = tasks
                    .iter()
                    .enumerate()
                    .filter(|(task_index, _)| task_index % thread_count == thread_id)
                    .map(|(_, task)| task.clone())
                    .collect();
                let base_position = position.clone();
                let tt = tt.clone();
                handles.push(scope.spawn(move || {
                    let mut worker = SearchWorker::new(tt, age);
                    let mut results = Vec::new();
                    for task in assigned_tasks {
                        for (index, chess_move) in task {
                            let mut next = base_position.clone();
                            if next.apply_unchecked(chess_move).is_err() {
                                continue;
                            }
                            let score = -worker.negamax(&next, depth, -INFINITY, INFINITY, 1);
                            results.push((index, chess_move, score));
                        }
                    }
                    (results, worker.searched_nodes)
                }));
            }

            for handle in handles {
                if let Ok((results, nodes)) = handle.join() {
                    total_nodes += nodes;
                    for (index, chess_move, score) in results {
                        all_results.push((index, chess_move, score));
                    }
                }
            }
        });

        if all_results.is_empty() {
            return None;
        }
        self.searched_nodes = total_nodes;
        all_results.sort_by_key(|(index, _, _)| *index);

        let mut best = all_results[0];
        for result in all_results.into_iter().skip(1) {
            if result.2 > best.2 || (result.2 == best.2 && result.0 < best.0) {
                best = result;
            }
        }
        Some((best.1, best.2))
    }
}

struct SearchWorker {
    searched_nodes: u64,
    tt: Arc<TranspositionTable>,
    age: u8,
}

impl SearchWorker {
    fn new(tt: Arc<TranspositionTable>, age: u8) -> Self {
        Self { searched_nodes: 0, tt, age }
    }

    fn negamax(&mut self, position: &Position, depth: u8, mut alpha: i32, mut beta: i32, ply: i32) -> i32 {
        self.searched_nodes += 1;
        if position.is_fifty_move_rule_draw() {
            return 0;
        }
        let alpha_start = alpha;
        let key = hash_position(position);

        if depth > 0 {
            if let Some(hit) = self.tt.probe(key, depth) {
                match hit.bound {
                    TT_EXACT => return hit.score,
                    TT_LOWER => alpha = alpha.max(hit.score),
                    TT_UPPER => beta = beta.min(hit.score),
                    _ => {}
                }
                if alpha >= beta {
                    return hit.score;
                }
            }
        }

        let in_check = position.is_in_check(position.side_to_move());
        let mut moves = position.legal_moves();
        if moves.is_empty() {
            return if in_check {
                -MATE_SCORE + ply
            } else {
                0
            };
        }

        if let Some(score) = mate_in_one_score(position, &moves, ply) {
            return score;
        }

        if depth == 0 && !in_check {
            return self.quiescence(position, alpha, beta, ply);
        }

        order_moves(position, &mut moves);
        let mut best = -INFINITY;
        for chess_move in moves {
            let mut next = position.clone();
            if next.apply_unchecked(chess_move).is_err() {
                continue;
            }
            let next_depth = depth.saturating_sub(1);
            let score = -self.negamax(&next, next_depth, -beta, -alpha, ply + 1);
            best = best.max(score);
            alpha = alpha.max(score);
            if alpha >= beta {
                break;
            }
        }

        let bound = if best <= alpha_start {
            TT_UPPER
        } else if best >= beta {
            TT_LOWER
        } else {
            TT_EXACT
        };
        self.tt.store(key, depth, best, bound, self.age);
        best
    }

    fn quiescence(&mut self, position: &Position, mut alpha: i32, beta: i32, ply: i32) -> i32 {
        self.searched_nodes += 1;
        if position.is_fifty_move_rule_draw() {
            return 0;
        }
        let stand_pat = evaluate_for_side_to_move(position);
        if stand_pat >= beta {
            return beta;
        }
        alpha = alpha.max(stand_pat);

        let mut captures = position.legal_captures();
        order_moves(position, &mut captures);
        for chess_move in captures {
            let mut next = position.clone();
            if next.apply_unchecked(chess_move).is_err() {
                continue;
            }
            let score = -self.quiescence(&next, -beta, -alpha, ply + 1);
            if score >= beta {
                return beta;
            }
            alpha = alpha.max(score);
        }
        alpha
    }
}

#[derive(Debug)]
struct TranspositionTable {
    age: AtomicU64,
    entries: Vec<AtomicTtEntry>,
}

impl TranspositionTable {
    fn new(hash_mb: usize) -> Self {
        let bytes = hash_mb.clamp(1, 4096).saturating_mul(1024 * 1024);
        let entry_size = std::mem::size_of::<AtomicTtEntry>().max(1);
        let entry_count = (bytes / entry_size).max(1024);
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            entries.push(AtomicTtEntry::new());
        }
        Self { age: AtomicU64::new(0), entries }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn next_age(&self) -> u8 {
        self.age.fetch_add(1, Ordering::Relaxed).wrapping_add(1) as u8
    }

    fn probe(&self, key: u64, depth: u8) -> Option<TtHit> {
        let slot = self.slot(key)?;
        let key_a = slot.key.load(Ordering::Acquire);
        if key_a != key {
            return None;
        }
        let data = slot.data.load(Ordering::Acquire);
        let key_b = slot.key.load(Ordering::Acquire);
        if key_b != key || data == 0 {
            return None;
        }
        let hit = decode_tt_data(data)?;
        if hit.depth < depth {
            return None;
        }
        Some(hit)
    }

    fn store(&self, key: u64, depth: u8, score: i32, bound: u8, age: u8) {
        if score.abs() >= MATE_SCORE - 1024 {
            return;
        }
        let Some(slot) = self.slot(key) else {
            return;
        };
        let old_data = slot.data.load(Ordering::Acquire);
        let old_hit = decode_tt_data(old_data);
        let replace = match old_hit {
            Some(old) => old.depth <= depth || old.age != age,
            None => true,
        };
        if !replace {
            return;
        }
        if let Some(data) = encode_tt_data(depth, score, bound, age) {
            slot.data.store(data, Ordering::Release);
            slot.key.store(key, Ordering::Release);
        }
    }

    fn slot(&self, key: u64) -> Option<&AtomicTtEntry> {
        if self.entries.is_empty() {
            return None;
        }
        let index = (key as usize) % self.entries.len();
        self.entries.get(index)
    }
}

#[derive(Debug)]
struct AtomicTtEntry {
    key: AtomicU64,
    data: AtomicU64,
}

impl AtomicTtEntry {
    fn new() -> Self {
        Self {
            key: AtomicU64::new(0),
            data: AtomicU64::new(0),
        }
    }
}

#[derive(Clone, Copy)]
struct TtHit {
    depth: u8,
    score: i32,
    bound: u8,
    age: u8,
}

fn encode_tt_data(depth: u8, score: i32, bound: u8, age: u8) -> Option<u64> {
    let encoded_score = score.checked_add(TT_SCORE_BIAS)? as u64;
    if encoded_score > TT_SCORE_BITS || bound > TT_UPPER {
        return None;
    }
    Some((bound as u64) | ((depth as u64) << 2) | ((age as u64) << 10) | (encoded_score << 18))
}

fn decode_tt_data(data: u64) -> Option<TtHit> {
    if data == 0 {
        return None;
    }
    let bound = (data & 0b11) as u8;
    let depth = ((data >> 2) & 0xff) as u8;
    let age = ((data >> 10) & 0xff) as u8;
    let encoded_score = ((data >> 18) & TT_SCORE_BITS) as i32;
    let score = encoded_score - TT_SCORE_BIAS;
    Some(TtHit { depth, score, bound, age })
}

fn hash_position(position: &Position) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in position.to_fen().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == 0 { 1 } else { hash }
}

pub fn evaluate_for_side_to_move(position: &Position) -> i32 {
    let white_score = evaluate_white_perspective(position);
    match position.side_to_move() {
        Color::White => white_score,
        Color::Black => -white_score,
    }
}

pub fn evaluate_white_perspective(position: &Position) -> i32 {
    let mut score = 0;
    let mut white_bishops = 0;
    let mut black_bishops = 0;

    for square in 0_u8..64 {
        let Some(piece) = position.piece_at(square) else {
            continue;
        };
        let value = piece.kind.material_value() + positional_bonus(piece.kind, piece.color, square);
        match piece.color {
            Color::White => {
                score += value;
                if piece.kind == PieceKind::Bishop {
                    white_bishops += 1;
                }
            }
            Color::Black => {
                score -= value;
                if piece.kind == PieceKind::Bishop {
                    black_bishops += 1;
                }
            }
        }
    }

    if white_bishops >= 2 {
        score += 35;
    }
    if black_bishops >= 2 {
        score -= 35;
    }
    score
}

fn positional_bonus(kind: PieceKind, color: Color, square: u8) -> i32 {
    let file = file_of(square);
    let rank = rank_of(square);
    let own_rank = match color {
        Color::White => rank,
        Color::Black => 7 - rank,
    };
    let center_file_distance = (file * 2 - 7).abs();
    let center_rank_distance = (rank * 2 - 7).abs();
    let center_bonus = 14 - center_file_distance - center_rank_distance;

    match kind {
        PieceKind::Pawn => own_rank * 8 + (6 - center_file_distance).max(0),
        PieceKind::Knight => center_bonus * 4,
        PieceKind::Bishop => center_bonus * 3 + own_rank,
        PieceKind::Rook => own_rank * 2,
        PieceKind::Queen => center_bonus,
        PieceKind::King => {
            if own_rank <= 1 {
                10 - center_file_distance
            } else {
                -center_bonus
            }
        }
    }
}


pub fn evaluate_tactical_for_side_to_move(position: &Position) -> i32 {
    if position.is_checkmate() {
        return -MATE_SCORE;
    }
    if position.is_stalemate() || position.is_fifty_move_rule_draw() {
        return 0;
    }
    let moves = position.legal_moves();
    if let Some(score) = mate_in_one_score(position, &moves, 0) {
        return score;
    }
    evaluate_for_side_to_move(position)
}

fn mate_in_one_score(position: &Position, moves: &[ChessMove], ply: i32) -> Option<i32> {
    for chess_move in moves.iter().copied() {
        let mut next = position.clone();
        if next.apply_unchecked(chess_move).is_err() {
            continue;
        }
        if next.is_checkmate() {
            return Some(MATE_SCORE - ply - 1);
        }
    }
    None
}

pub fn score_is_mate(score: i32) -> bool {
    score.abs() >= MATE_SCORE - 1024
}

pub fn mate_score_to_uci_moves(score: i32) -> Option<i32> {
    if !score_is_mate(score) {
        return None;
    }
    let plies = (MATE_SCORE - score.abs()).max(0);
    let moves = (plies + 1) / 2;
    Some(if score > 0 { moves.max(1) } else { -moves.max(1) })
}

fn order_moves(position: &Position, moves: &mut [ChessMove]) {
    moves.sort_by(|left, right| move_order_score(position, *right).cmp(&move_order_score(position, *left)));
}

fn move_order_score(position: &Position, chess_move: ChessMove) -> i32 {
    let Some(attacker) = position.piece_at(chess_move.from) else {
        return 0;
    };
    let mut score = 0;
    if let Some(promotion) = chess_move.promotion {
        score += 8_000 + promotion.material_value();
    }
    if position.is_capture(chess_move) {
        let victim_value = position
            .piece_at(chess_move.to)
            .map(|piece| piece.kind.material_value())
            .unwrap_or(PieceKind::Pawn.material_value());
        score += 10 * victim_value - attacker.kind.material_value();
    }
    if attacker.kind == PieceKind::King && (file_of(chess_move.from) - file_of(chess_move.to)).abs() == 2 {
        score += 50;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chess::Position;

    #[test]
    fn engine_finds_a_legal_move_from_startpos() {
        let position = Position::startpos();
        let mut engine = Engine::new(2);
        let best_move = engine.best_move(&position).unwrap();
        assert!(position.legal_moves().contains(&best_move));
    }

    #[test]
    fn deterministic_root_split_matches_single_thread_best_move() {
        let position = Position::startpos();
        let mut single = Engine::new(2);
        single.set_deterministic_multithread(false);
        single.set_hash_mb(4);
        let single_best = single.best_move_with_score(&position).unwrap();

        let mut parallel = Engine::new(2);
        parallel.set_settings(SearchSettings {
            deterministic_multithread: true,
            max_threads: 4,
            granularity: 2,
            hash_mb: 4,
        });
        let parallel_best = parallel.best_move_with_score(&position).unwrap();

        assert_eq!(single_best.0, parallel_best.0);
        assert_eq!(single_best.1, parallel_best.1);
    }

    #[test]
    fn hash_size_setting_resizes_transposition_table() {
        let mut engine = Engine::new(1);
        engine.set_hash_mb(1);
        let small = engine.transposition_entries();
        engine.set_hash_mb(2);
        assert!(engine.transposition_entries() > small);
    }

    #[test]
    fn quiescence_frontier_sees_mate_in_one() {
        let position = Position::from_fen("6k1/8/5QK1/8/8/8/8/8 w - - 0 1").unwrap();
        let score = evaluate_tactical_for_side_to_move(&position);
        assert!(score_is_mate(score));

        let mut engine = Engine::new(1);
        let (_best, search_score) = engine.best_move_with_score(&position).unwrap();
        assert!(score_is_mate(search_score));
    }

    #[test]
    fn tactical_eval_scores_fifty_move_rule_as_draw() {
        let position = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w Q - 100 42").unwrap();
        assert_eq!(evaluate_tactical_for_side_to_move(&position), 0);
    }
}
