use crate::chess::{ChessMove, Color, Position};
use crate::pgn::move_to_san;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisStage {
    BeforeMove,
    AfterMove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisJob {
    pub item_index: usize,
    pub stage: AnalysisStage,
    pub fen: String,
}

#[derive(Clone, Debug)]
pub struct MoveAnalysis {
    pub ply: usize,
    pub side: Color,
    pub san: String,
    pub uci: String,
    pub before_fen: String,
    pub after_fen: String,
    pub before_score_cp: Option<i32>,
    pub after_score_cp: Option<i32>,
    pub loss_cp: Option<i32>,
    pub accuracy: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct AnalysisSummary {
    pub white_accuracy: Option<f32>,
    pub black_accuracy: Option<f32>,
    pub verdict: String,
}

#[derive(Clone, Debug)]
pub struct GameAnalysis {
    pub start_fen: String,
    pub items: Vec<MoveAnalysis>,
}

impl GameAnalysis {
    pub fn from_history(start_fen: &str, moves: &[ChessMove]) -> Result<Self, String> {
        let mut position = Position::from_fen(start_fen.trim())?;
        let mut items = Vec::with_capacity(moves.len());

        for (index, chess_move) in moves.iter().copied().enumerate() {
            let side = position.side_to_move();
            let before_fen = position.to_fen();
            let san = move_to_san(&position, chess_move).unwrap_or_else(|_| chess_move.to_uci());
            position.make_legal_move(chess_move)?;
            let after_fen = position.to_fen();
            items.push(MoveAnalysis {
                ply: index + 1,
                side,
                san,
                uci: chess_move.to_uci(),
                before_fen,
                after_fen,
                before_score_cp: None,
                after_score_cp: None,
                loss_cp: None,
                accuracy: None,
            });
        }

        Ok(Self {
            start_fen: start_fen.trim().to_string(),
            items,
        })
    }

    pub fn jobs(&self) -> Vec<AnalysisJob> {
        let mut jobs = Vec::with_capacity(self.items.len() * 2);
        for (item_index, item) in self.items.iter().enumerate() {
            jobs.push(AnalysisJob {
                item_index,
                stage: AnalysisStage::BeforeMove,
                fen: item.before_fen.clone(),
            });
            jobs.push(AnalysisJob {
                item_index,
                stage: AnalysisStage::AfterMove,
                fen: item.after_fen.clone(),
            });
        }
        jobs
    }

    pub fn total_jobs(&self) -> usize {
        self.items.len() * 2
    }

    pub fn completed_jobs(&self) -> usize {
        self.items
            .iter()
            .map(|item| item.before_score_cp.iter().count() + item.after_score_cp.iter().count())
            .sum()
    }

    pub fn set_score(&mut self, job: &AnalysisJob, score_cp: i32) {
        if let Some(item) = self.items.get_mut(job.item_index) {
            match job.stage {
                AnalysisStage::BeforeMove => item.before_score_cp = Some(score_cp),
                AnalysisStage::AfterMove => item.after_score_cp = Some(score_cp),
            }
        }
        self.recompute_item(job.item_index);
    }

    pub fn summary(&self) -> AnalysisSummary {
        let white_accuracy = average_accuracy_for(&self.items, Color::White);
        let black_accuracy = average_accuracy_for(&self.items, Color::Black);
        let verdict = match (white_accuracy, black_accuracy) {
            (Some(white), Some(black)) => {
                let diff = white - black;
                if diff.abs() < 3.0 {
                    format!("accuracy is balanced: White {white:.1}, Black {black:.1}")
                } else if diff > 0.0 {
                    format!("White was more accurate: {white:.1} vs {black:.1}")
                } else {
                    format!("Black was more accurate: {black:.1} vs {white:.1}")
                }
            }
            (Some(white), None) => format!("only White has analysed moves: {white:.1}"),
            (None, Some(black)) => format!("only Black has analysed moves: {black:.1}"),
            (None, None) => "not enough analysed moves".to_string(),
        };
        AnalysisSummary {
            white_accuracy,
            black_accuracy,
            verdict,
        }
    }

    pub fn report(&self) -> String {
        let summary = self.summary();
        let mut output = String::new();
        output.push_str("rchess game analysis\n");
        output.push_str(&format!("Start FEN: {}\n", self.start_fen));
        output.push_str(&format!("Verdict: {}\n", summary.verdict));
        output.push_str(&format!(
            "White accuracy: {}\n",
            format_accuracy(summary.white_accuracy)
        ));
        output.push_str(&format!(
            "Black accuracy: {}\n\n",
            format_accuracy(summary.black_accuracy)
        ));
        output.push_str("Ply  Side   Move      Before  After   Loss   Accuracy\n");
        for item in &self.items {
            output.push_str(&format!(
                "{:<4} {:<6} {:<9} {:<7} {:<7} {:<6} {}\n",
                item.ply,
                color_name(item.side),
                item.san,
                format_cp(item.before_score_cp),
                format_cp(item.after_score_cp.map(|value| -value)),
                format_loss(item.loss_cp),
                format_accuracy(item.accuracy),
            ));
        }
        output
    }

    fn recompute_item(&mut self, index: usize) {
        let Some(item) = self.items.get_mut(index) else {
            return;
        };
        let (Some(before), Some(after_raw)) = (item.before_score_cp, item.after_score_cp) else {
            item.loss_cp = None;
            item.accuracy = None;
            return;
        };

        // UCI scores are reported from the side-to-move perspective.
        // Before the move the side to move is the player being judged.
        // After the move the opponent is to move, so invert the sign.
        let after_from_mover_view = -after_raw;
        let loss = (before - after_from_mover_view).max(0);
        item.loss_cp = Some(loss);
        item.accuracy = Some(move_accuracy_from_loss_cp(loss));
    }
}

pub fn move_accuracy_from_loss_cp(loss_cp: i32) -> f32 {
    (100.0 - (loss_cp.max(0).min(1000) as f32 / 10.0)).clamp(0.0, 100.0)
}

pub fn format_cp(score: Option<i32>) -> String {
    match score {
        Some(value) => format_cp_value(value),
        None => "-".to_string(),
    }
}

pub fn format_cp_value(score: i32) -> String {
    let pawns = score as f32 / 100.0;
    if score >= 0 {
        format!("+{pawns:.2}")
    } else {
        format!("{pawns:.2}")
    }
}

pub fn format_accuracy(value: Option<f32>) -> String {
    value
        .map(|accuracy| format!("{accuracy:.1}"))
        .unwrap_or_else(|| "-".to_string())
}

fn format_loss(loss: Option<i32>) -> String {
    loss.map(|value| value.to_string()).unwrap_or_else(|| "-".to_string())
}

fn average_accuracy_for(items: &[MoveAnalysis], color: Color) -> Option<f32> {
    let values: Vec<f32> = items
        .iter()
        .filter(|item| item.side == color)
        .filter_map(|item| item.accuracy)
        .collect();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f32>() / values.len() as f32)
    }
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chess::STARTPOS_FEN;

    #[test]
    fn builds_analysis_jobs_for_history() {
        let mut position = Position::startpos();
        let chess_move = position.parse_uci_move("e2e4").unwrap();
        let analysis = GameAnalysis::from_history(STARTPOS_FEN, &[chess_move]).unwrap();
        assert_eq!(analysis.items.len(), 1);
        assert_eq!(analysis.jobs().len(), 2);
        assert_eq!(analysis.items[0].san, "e4");
    }

    #[test]
    fn computes_accuracy_from_centipawn_loss() {
        let mut position = Position::startpos();
        let chess_move = position.parse_uci_move("e2e4").unwrap();
        let mut analysis = GameAnalysis::from_history(STARTPOS_FEN, &[chess_move]).unwrap();
        let jobs = analysis.jobs();
        analysis.set_score(&jobs[0], 30);
        analysis.set_score(&jobs[1], -10);
        assert_eq!(analysis.items[0].loss_cp, Some(20));
        assert_eq!(analysis.items[0].accuracy, Some(98.0));
    }
}
