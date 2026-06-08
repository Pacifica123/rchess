use crate::chess::{file_of, rank_of, ChessMove, Color, PieceKind, Position};

const INFINITY: i32 = 1_000_000;
const MATE_SCORE: i32 = 900_000;

#[derive(Clone, Debug)]
pub struct Engine {
    max_depth: u8,
    searched_nodes: u64,
}

impl Engine {
    pub fn new(max_depth: u8) -> Self {
        Self {
            max_depth: max_depth.max(1),
            searched_nodes: 0,
        }
    }

    pub fn set_depth(&mut self, max_depth: u8) {
        self.max_depth = max_depth.max(1);
    }

    pub fn searched_nodes(&self) -> u64 {
        self.searched_nodes
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

        let mut best_move = moves[0];
        let mut best_score = -INFINITY;
        let mut alpha = -INFINITY;
        let beta = INFINITY;

        for chess_move in moves {
            let mut next = position.clone();
            next.apply_unchecked(chess_move).ok()?;
            let score = -self.negamax(&next, self.max_depth.saturating_sub(1), -beta, -alpha, 1);
            if score > best_score {
                best_score = score;
                best_move = chess_move;
            }
            alpha = alpha.max(score);
        }
        Some((best_move, best_score))
    }

    fn negamax(&mut self, position: &Position, depth: u8, mut alpha: i32, beta: i32, ply: i32) -> i32 {
        self.searched_nodes += 1;

        let in_check = position.is_in_check(position.side_to_move());
        let mut moves = position.legal_moves();
        if moves.is_empty() {
            return if in_check {
                -MATE_SCORE + ply
            } else {
                0
            };
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
        best
    }

    fn quiescence(&mut self, position: &Position, mut alpha: i32, beta: i32, ply: i32) -> i32 {
        self.searched_nodes += 1;
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

pub fn evaluate_for_side_to_move(position: &Position) -> i32 {
    let white_score = evaluate_white_perspective(position);
    match position.side_to_move() {
        Color::White => white_score,
        Color::Black => -white_score,
    }
}

fn evaluate_white_perspective(position: &Position) -> i32 {
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
}
