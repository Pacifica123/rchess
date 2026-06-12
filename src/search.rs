use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use crate::chess::{file_of, index, rank_of, ChessMove, Color, Piece, PieceKind, Position};

const INFINITY: i32 = 1_000_000;
const MATE_SCORE: i32 = 900_000;
const TT_SCORE_BIAS: i32 = 1_050_000;
const TT_SCORE_BITS: u64 = (1 << 22) - 1;
const TT_EXACT: u8 = 0;
const TT_LOWER: u8 = 1;
const TT_UPPER: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootCandidate {
    pub root_index: usize,
    pub chess_move: ChessMove,
    pub score: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchSettings {
    pub deterministic_multithread: bool,
    pub max_threads: usize,
    pub granularity: usize,
    pub hash_mb: usize,
    pub avoid_draws: bool,
    pub draw_contempt_cp: i32,
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
            avoid_draws: false,
            draw_contempt_cp: 35,
        }
    }
}

impl SearchSettings {
    pub fn normalized(mut self) -> Self {
        self.max_threads = self.max_threads.clamp(1, 64);
        self.granularity = self.granularity.clamp(1, 64);
        self.hash_mb = self.hash_mb.clamp(1, 4096);
        self.draw_contempt_cp = self.draw_contempt_cp.clamp(0, 400);
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
        if next.hash_mb != self.settings.hash_mb
            || next.avoid_draws != self.settings.avoid_draws
            || next.draw_contempt_cp != self.settings.draw_contempt_cp
        {
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

    pub fn set_avoid_draws(&mut self, value: bool) {
        let mut settings = self.settings;
        settings.avoid_draws = value;
        self.set_settings(settings);
    }

    pub fn set_draw_contempt_cp(&mut self, value: i32) {
        let mut settings = self.settings;
        settings.draw_contempt_cp = value;
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
        self.root_candidates(position)
            .into_iter()
            .next()
            .map(|candidate| (candidate.chess_move, candidate.score))
    }

    pub fn root_candidates(&mut self, position: &Position) -> Vec<RootCandidate> {
        self.searched_nodes = 0;
        let mut moves = position.legal_moves();
        if moves.is_empty() {
            return Vec::new();
        }
        order_moves(position, &mut moves);
        let age = self.tt.next_age();

        let mut candidates = if self.settings.deterministic_multithread
            && self.settings.max_threads > 1
            && moves.len() >= self.settings.granularity.max(1) * 2
        {
            self.root_candidates_split(position, moves, age)
        } else {
            self.root_candidates_single_thread(position, moves, age)
        };
        candidates.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.root_index.cmp(&right.root_index))
        });
        candidates
    }

    fn root_candidates_single_thread(&mut self, position: &Position, moves: Vec<ChessMove>, age: u8) -> Vec<RootCandidate> {
        let mut worker = SearchWorker::new(self.tt.clone(), age, self.settings);
        let mut candidates = Vec::with_capacity(moves.len());
        let depth = self.max_depth.saturating_sub(1);
        for (root_index, chess_move) in moves.into_iter().enumerate() {
            let mut next = position.clone();
            if next.apply_unchecked(chess_move).is_err() {
                continue;
            }
            let raw_score = -worker.negamax(&next, depth, -INFINITY, INFINITY, 1);
            let score = adjusted_root_score(position, chess_move, raw_score);
            candidates.push(RootCandidate { root_index, chess_move, score });
        }
        self.searched_nodes = worker.searched_nodes;
        candidates
    }

    fn root_candidates_split(&mut self, position: &Position, moves: Vec<ChessMove>, age: u8) -> Vec<RootCandidate> {
        let indexed_moves: Vec<(usize, ChessMove)> = moves.iter().copied().enumerate().collect();
        let granularity = self.settings.granularity.max(1);
        let tasks: Vec<Vec<(usize, ChessMove)>> = indexed_moves
            .chunks(granularity)
            .map(|chunk| chunk.to_vec())
            .collect();
        let thread_count = self.settings.max_threads.min(tasks.len()).max(1);
        let mut all_results: Vec<RootCandidate> = Vec::with_capacity(moves.len());
        let mut total_nodes = 0_u64;
        let tt = self.tt.clone();
        let depth = self.max_depth.saturating_sub(1);
        let settings = self.settings;

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
                    let mut worker = SearchWorker::new(tt, age, settings);
                    let mut results = Vec::new();
                    for task in assigned_tasks {
                        for (root_index, chess_move) in task {
                            let mut next = base_position.clone();
                            if next.apply_unchecked(chess_move).is_err() {
                                continue;
                            }
                            let raw_score = -worker.negamax(&next, depth, -INFINITY, INFINITY, 1);
                            let score = adjusted_root_score(&base_position, chess_move, raw_score);
                            results.push(RootCandidate { root_index, chess_move, score });
                        }
                    }
                    (results, worker.searched_nodes)
                }));
            }

            for handle in handles {
                if let Ok((results, nodes)) = handle.join() {
                    total_nodes += nodes;
                    all_results.extend(results);
                }
            }
        });

        self.searched_nodes = total_nodes;
        all_results
    }
}

struct SearchWorker {
    searched_nodes: u64,
    tt: Arc<TranspositionTable>,
    age: u8,
    settings: SearchSettings,
}

impl SearchWorker {
    fn new(tt: Arc<TranspositionTable>, age: u8, settings: SearchSettings) -> Self {
        Self { searched_nodes: 0, tt, age, settings: settings.normalized() }
    }

    fn draw_score(&self, position: &Position) -> i32 {
        draw_score_for_side_to_move(position, self.settings)
    }

    fn negamax(&mut self, position: &Position, depth: u8, mut alpha: i32, mut beta: i32, ply: i32) -> i32 {
        self.searched_nodes += 1;
        if position.is_fifty_move_rule_draw() {
            return self.draw_score(position);
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
                self.draw_score(position)
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
            return self.draw_score(position);
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
    score + strategic_opening_balance(position) + king_safety_balance(position)
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


fn strategic_opening_balance(position: &Position) -> i32 {
    side_opening_score(position, Color::White) - side_opening_score(position, Color::Black)
}

fn side_opening_score(position: &Position, color: Color) -> i32 {
    if position.fullmove_number() > 24 {
        return 0;
    }

    let undeveloped = undeveloped_minor_count(position, color);
    let mut score = 0;
    score -= undeveloped * 18;
    score -= overextended_minor_penalty(position, color, undeveloped);
    score -= early_queen_penalty(position, color, undeveloped);
    score -= unsafe_king_penalty(position, color, undeveloped);
    if king_is_castled(position, color) && position.fullmove_number() <= 18 {
        score += 30;
    }
    score
}

fn undeveloped_minor_count(position: &Position, color: Color) -> i32 {
    home_minor_squares(color)
        .iter()
        .filter(|square| {
            matches!(
                position.piece_at(**square),
                Some(piece) if piece.color == color && (piece.kind == PieceKind::Knight || piece.kind == PieceKind::Bishop)
            )
        })
        .count() as i32
}

fn overextended_minor_penalty(position: &Position, color: Color, undeveloped: i32) -> i32 {
    let mut penalty = 0;
    let castled = king_is_castled(position, color);
    for square in 0_u8..64 {
        let Some(piece) = position.piece_at(square) else {
            continue;
        };
        if piece.color != color || (piece.kind != PieceKind::Knight && piece.kind != PieceKind::Bishop) {
            continue;
        }
        let own_rank = own_rank(square, color);
        if own_rank >= 4 && position.fullmove_number() <= 16 {
            penalty += 20 + undeveloped * 12;
            if !castled {
                penalty += 18;
            }
            if piece.kind == PieceKind::Knight && own_rank >= 5 {
                penalty += 20;
            }
        }
    }
    penalty
}

fn early_queen_penalty(position: &Position, color: Color, undeveloped: i32) -> i32 {
    if position.fullmove_number() > 18 {
        return 0;
    }
    let Some(square) = queen_square(position, color) else {
        return 0;
    };
    if square == home_queen_square(color) {
        return 0;
    }

    let mut penalty = 20;
    let own_rank = own_rank(square, color);
    if own_rank >= 4 {
        penalty += 35;
    }
    if own_rank >= 6 {
        penalty += 70;
    }
    if undeveloped >= 2 {
        penalty += undeveloped * 25;
    }
    if !king_is_castled(position, color) {
        penalty += 45;
    }
    penalty
}

fn unsafe_king_penalty(position: &Position, color: Color, undeveloped: i32) -> i32 {
    let Some(king) = position.king_square(color) else {
        return 300;
    };
    let fullmove = position.fullmove_number();
    let mut penalty = 0;
    let castled = king_is_castled(position, color);
    let file = file_of(king);

    if !castled && fullmove >= 7 {
        penalty += 25 + undeveloped * 10;
        if (2..=5).contains(&file) {
            penalty += 35;
        }
        if fullmove >= 12 {
            penalty += 25;
        }
    }

    if central_pawns_disrupted(position, color) && !castled {
        penalty += 35;
        if (2..=5).contains(&file) {
            penalty += 35;
        }
    }

    penalty
}

fn central_pawns_disrupted(position: &Position, color: Color) -> bool {
    let (d_square, e_square) = match color {
        Color::White => (11, 12),
        Color::Black => (51, 52),
    };
    let d_home = position.piece_at(d_square) == Some(crate::chess::Piece { color, kind: PieceKind::Pawn });
    let e_home = position.piece_at(e_square) == Some(crate::chess::Piece { color, kind: PieceKind::Pawn });
    !(d_home && e_home)
}

fn queen_square(position: &Position, color: Color) -> Option<u8> {
    (0_u8..64).find(|square| {
        position.piece_at(*square) == Some(crate::chess::Piece { color, kind: PieceKind::Queen })
    })
}

fn home_minor_squares(color: Color) -> [u8; 4] {
    match color {
        Color::White => [1, 2, 5, 6],
        Color::Black => [57, 58, 61, 62],
    }
}

fn home_queen_square(color: Color) -> u8 {
    match color {
        Color::White => 3,
        Color::Black => 59,
    }
}

fn king_is_castled(position: &Position, color: Color) -> bool {
    match color {
        Color::White => matches!(position.king_square(Color::White), Some(2 | 6)),
        Color::Black => matches!(position.king_square(Color::Black), Some(58 | 62)),
    }
}

fn own_rank(square: u8, color: Color) -> i32 {
    match color {
        Color::White => rank_of(square),
        Color::Black => 7 - rank_of(square),
    }
}

fn draw_score_for_side_to_move(position: &Position, settings: SearchSettings) -> i32 {
    if !settings.avoid_draws || settings.draw_contempt_cp == 0 {
        return 0;
    }
    let static_score = evaluate_for_side_to_move(position);
    if static_score <= -150 {
        (settings.draw_contempt_cp / 2).max(1)
    } else {
        -settings.draw_contempt_cp.max(1)
    }
}

fn adjusted_root_score(position: &Position, chess_move: ChessMove, raw_score: i32) -> i32 {
    if score_is_mate(raw_score) {
        return raw_score;
    }
    raw_score.saturating_add(root_tactical_adjustment(position, chess_move))
}

fn root_tactical_adjustment(position: &Position, chess_move: ChessMove) -> i32 {
    let Some(mover) = position.piece_at(chess_move.from) else {
        return 0;
    };
    let captured_value = captured_piece_value(position, chess_move).unwrap_or(0);
    let was_capture = captured_value > 0;
    let mut next = position.clone();
    if next.apply_unchecked(chess_move).is_err() {
        return 0;
    }
    if next.is_checkmate() {
        return 0;
    }

    let mut penalty = 0;
    let opponent_moves = next.legal_moves();
    if mate_in_one_score(&next, &opponent_moves, 1).is_some() {
        penalty += 220_000;
    } else {
        let own_king_danger = side_king_danger(&next, mover.color);
        let greedy_queen_capture = mover.kind == PieceKind::Queen && was_capture && captured_value >= PieceKind::Rook.material_value();
        if (own_king_danger >= 55 || greedy_queen_capture) && side_has_forced_mate_in_two(&next, &opponent_moves) {
            penalty += 90_000;
        }
    }

    let checking_replies = checking_reply_count(&next, &opponent_moves);
    if checking_replies > 1 {
        penalty += (checking_replies as i32 - 1) * 18;
    }

    if was_capture {
        let see = static_exchange_eval(position, chess_move);
        if see < -40 {
            penalty += (-see).min(500);
        }
        if mover.kind == PieceKind::Queen && captured_value >= PieceKind::Rook.material_value() {
            penalty += 45;
            if checking_replies > 0 {
                penalty += 80 + checking_replies as i32 * 25;
            }
            if side_king_danger(&next, mover.color) >= 70 {
                penalty += 90;
            }
        }
    }

    -penalty
}

pub fn static_exchange_eval(position: &Position, chess_move: ChessMove) -> i32 {
    let Some(attacker) = position.piece_at(chess_move.from) else {
        return 0;
    };
    let Some(victim_value) = captured_piece_value(position, chess_move) else {
        return 0;
    };
    let mut next = position.clone();
    if next.apply_unchecked(chess_move).is_err() {
        return 0;
    }
    let moved_value = next
        .piece_at(chess_move.to)
        .map(|piece| piece.kind.material_value())
        .unwrap_or_else(|| attacker.kind.material_value());
    let opponent = attacker.color.opposite();
    let mut score = victim_value;
    if let Some(recapture_value) = least_attacker_value(&next, chess_move.to, opponent) {
        score -= moved_value;
        if least_attacker_value(&next, chess_move.to, attacker.color).is_some() {
            score += recapture_value.min(moved_value) / 2;
        }
    }
    score
}

fn captured_piece_value(position: &Position, chess_move: ChessMove) -> Option<i32> {
    if let Some(piece) = position.piece_at(chess_move.to) {
        return Some(piece.kind.material_value());
    }
    let mover = position.piece_at(chess_move.from)?;
    if mover.kind == PieceKind::Pawn
        && position.is_capture(chess_move)
        && file_of(chess_move.from) != file_of(chess_move.to)
    {
        return Some(PieceKind::Pawn.material_value());
    }
    None
}

fn checking_reply_count(position: &Position, moves: &[ChessMove]) -> usize {
    let mut count = 0;
    for chess_move in moves.iter().copied() {
        let mut next = position.clone();
        if next.apply_unchecked(chess_move).is_ok() && next.is_in_check(next.side_to_move()) {
            count += 1;
            if count >= 6 {
                break;
            }
        }
    }
    count
}

fn side_has_forced_mate_in_two(position: &Position, first_moves: &[ChessMove]) -> bool {
    if first_moves.len() > 48 {
        return false;
    }
    for first in first_moves.iter().copied() {
        let mut after_first = position.clone();
        if after_first.apply_unchecked(first).is_err() {
            continue;
        }
        if after_first.is_checkmate() {
            return true;
        }
        let replies = after_first.legal_moves();
        if replies.is_empty() || replies.len() > 64 {
            continue;
        }
        let mut all_replies_allow_mate = true;
        for reply in replies {
            let mut after_reply = after_first.clone();
            if after_reply.apply_unchecked(reply).is_err() {
                all_replies_allow_mate = false;
                break;
            }
            let mating_moves = after_reply.legal_moves();
            if mate_in_one_score(&after_reply, &mating_moves, 0).is_none() {
                all_replies_allow_mate = false;
                break;
            }
        }
        if all_replies_allow_mate {
            return true;
        }
    }
    false
}

fn king_safety_balance(position: &Position) -> i32 {
    -side_king_danger(position, Color::White) + side_king_danger(position, Color::Black)
}

fn side_king_danger(position: &Position, color: Color) -> i32 {
    let Some(king) = position.king_square(color) else {
        return 400;
    };
    if position.fullmove_number() > 45 && non_pawn_material_total(position) <= 1_600 {
        return 0;
    }
    let opponent = color.opposite();
    let ring = king_ring_squares(king);
    let mut danger = 0;
    for square in 0_u8..64 {
        let Some(piece) = position.piece_at(square) else {
            continue;
        };
        if piece.color != opponent {
            continue;
        }
        let unit = attack_unit(piece.kind);
        let mut touches_ring = false;
        for target in &ring {
            if attacks_square(position, square, piece, *target) {
                touches_ring = true;
                danger += unit;
            }
        }
        if touches_ring && piece.kind == PieceKind::Queen {
            danger += 4;
        }
        if attacks_square(position, square, piece, king) {
            danger += unit * 2;
        }
    }

    danger += missing_pawn_shield_penalty(position, color, king);
    if !king_is_castled(position, color) && (2..=5).contains(&file_of(king)) && position.fullmove_number() <= 28 {
        danger += 18;
    }
    danger.min(350)
}

fn non_pawn_material_total(position: &Position) -> i32 {
    let mut total = 0;
    for square in 0_u8..64 {
        let Some(piece) = position.piece_at(square) else {
            continue;
        };
        if piece.kind != PieceKind::Pawn && piece.kind != PieceKind::King {
            total += piece.kind.material_value();
        }
    }
    total
}

fn king_ring_squares(king: u8) -> Vec<u8> {
    let mut squares = Vec::with_capacity(9);
    for df in -1..=1 {
        for dr in -1..=1 {
            if let Some(square) = index(file_of(king) + df, rank_of(king) + dr) {
                squares.push(square);
            }
        }
    }
    squares
}

fn missing_pawn_shield_penalty(position: &Position, color: Color, king: u8) -> i32 {
    let direction = match color {
        Color::White => 1,
        Color::Black => -1,
    };
    let mut penalty = 0;
    for df in -1..=1 {
        if let Some(square) = index(file_of(king) + df, rank_of(king) + direction) {
            match position.piece_at(square) {
                Some(Piece { color: pawn_color, kind: PieceKind::Pawn }) if pawn_color == color => {}
                _ => penalty += 8,
            }
        }
    }
    penalty
}

fn attack_unit(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 3,
        PieceKind::Knight => 5,
        PieceKind::Bishop => 4,
        PieceKind::Rook => 5,
        PieceKind::Queen => 8,
        PieceKind::King => 1,
    }
}

fn least_attacker_value(position: &Position, target: u8, color: Color) -> Option<i32> {
    let mut best: Option<i32> = None;
    for square in 0_u8..64 {
        let Some(piece) = position.piece_at(square) else {
            continue;
        };
        if piece.color == color && attacks_square(position, square, piece, target) {
            let value = piece.kind.material_value();
            best = Some(best.map(|current| current.min(value)).unwrap_or(value));
        }
    }
    best
}

fn attacks_square(position: &Position, from: u8, piece: Piece, target: u8) -> bool {
    if from == target {
        return false;
    }
    let df = file_of(target) - file_of(from);
    let dr = rank_of(target) - rank_of(from);
    match piece.kind {
        PieceKind::Pawn => match piece.color {
            Color::White => dr == 1 && df.abs() == 1,
            Color::Black => dr == -1 && df.abs() == 1,
        },
        PieceKind::Knight => (df.abs() == 1 && dr.abs() == 2) || (df.abs() == 2 && dr.abs() == 1),
        PieceKind::Bishop => df.abs() == dr.abs() && line_clear(position, from, target, df.signum(), dr.signum()),
        PieceKind::Rook => (df == 0 || dr == 0) && line_clear(position, from, target, df.signum(), dr.signum()),
        PieceKind::Queen => {
            (df.abs() == dr.abs() || df == 0 || dr == 0)
                && line_clear(position, from, target, df.signum(), dr.signum())
        }
        PieceKind::King => df.abs() <= 1 && dr.abs() <= 1,
    }
}

fn line_clear(position: &Position, from: u8, target: u8, step_file: i32, step_rank: i32) -> bool {
    if step_file == 0 && step_rank == 0 {
        return false;
    }
    let mut file = file_of(from) + step_file;
    let mut rank = rank_of(from) + step_rank;
    while let Some(square) = index(file, rank) {
        if square == target {
            return true;
        }
        if position.piece_at(square).is_some() {
            return false;
        }
        file += step_file;
        rank += step_rank;
    }
    false
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
        score += (static_exchange_eval(position, chess_move) * 2).clamp(-2_000, 2_000);
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
            avoid_draws: false,
            draw_contempt_cp: 35,
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

    #[test]
    fn avoid_draws_turns_draw_score_into_contempt_when_not_worse() {
        let position = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w Q - 100 42").unwrap();
        let mut engine = Engine::new(1);
        engine.set_avoid_draws(true);
        assert!(draw_score_for_side_to_move(&position, engine.settings()) < 0);
    }

    #[test]
    fn see_marks_hanging_queen_capture_as_bad() {
        let position = Position::from_fen("4k3/8/8/8/8/1b6/r7/Q3K3 w - - 0 1").unwrap();
        let chess_move = position.parse_uci_move("a1a2").unwrap();
        assert!(static_exchange_eval(&position, chess_move) < 0);
    }
}
