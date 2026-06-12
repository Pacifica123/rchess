use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::analysis::{move_accuracy_from_loss_cp, GameAnalysis, MoveAnalysis};
use crate::chess::{ChessMove, Color, Position};
use crate::pgn::move_to_san;
use crate::search::{evaluate_tactical_for_side_to_move, RootCandidate};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperienceConfig {
    pub enabled: bool,
    pub path: String,
    pub min_games: u32,
    pub score_tolerance_cp: i32,
}

impl Default for ExperienceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: "rchess_experience.rxp".to_string(),
            min_games: 2,
            score_tolerance_cp: 25,
        }
    }
}

impl ExperienceConfig {
    pub fn normalized(mut self) -> Self {
        self.min_games = self.min_games.clamp(1, 10_000);
        self.score_tolerance_cp = self.score_tolerance_cp.clamp(0, 1_000);
        if self.path.trim().is_empty() {
            self.path = ExperienceConfig::default().path;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExperienceRecord {
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub unknown_results: u32,
    pub total_loss_cp: i64,
    pub loss_samples: u32,
    pub total_eval_error_cp: i64,
    pub eval_error_samples: u32,
}

impl ExperienceRecord {
    pub fn average_loss_cp(self) -> Option<i32> {
        if self.loss_samples == 0 {
            None
        } else {
            Some((self.total_loss_cp / self.loss_samples as i64) as i32)
        }
    }

    pub fn average_eval_error_cp(self) -> Option<i32> {
        if self.eval_error_samples == 0 {
            None
        } else {
            Some((self.total_eval_error_cp / self.eval_error_samples as i64) as i32)
        }
    }

    pub fn experience_score(self) -> i64 {
        let result_part = self.wins as i64 * 1_000 + self.draws as i64 * 120 - self.losses as i64 * 1_000;
        let loss_penalty = self.average_loss_cp().unwrap_or(0).max(0).min(3_000) as i64;
        let eval_error_penalty = self.average_eval_error_cp().unwrap_or(0).abs().min(3_000) as i64 / 4;
        result_part - loss_penalty - eval_error_penalty
    }

    pub fn compact_summary(self) -> String {
        format!(
            "games={} W/D/L/U={}/{}/{}/{} avg_loss={} exp_score={}",
            self.games,
            self.wins,
            self.draws,
            self.losses,
            self.unknown_results,
            self.average_loss_cp()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.experience_score()
        )
    }

    fn add_sample(&mut self, result: MoveResult, loss_cp: Option<i32>, eval_error_cp: Option<i32>) {
        self.games = self.games.saturating_add(1);
        match result {
            MoveResult::Win => self.wins = self.wins.saturating_add(1),
            MoveResult::Draw => self.draws = self.draws.saturating_add(1),
            MoveResult::Loss => self.losses = self.losses.saturating_add(1),
            MoveResult::Unknown => self.unknown_results = self.unknown_results.saturating_add(1),
        }
        if let Some(loss) = loss_cp {
            self.total_loss_cp += loss.max(0) as i64;
            self.loss_samples = self.loss_samples.saturating_add(1);
        }
        if let Some(error) = eval_error_cp {
            self.total_eval_error_cp += error.abs() as i64;
            self.eval_error_samples = self.eval_error_samples.saturating_add(1);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExperienceBook {
    records: BTreeMap<(String, String), ExperienceRecord>,
}

impl ExperienceBook {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }
        if path.is_dir() {
            let mut paths = Vec::new();
            for entry in fs::read_dir(path).map_err(|error| format!("experience directory read error: {error}"))? {
                let entry = entry.map_err(|error| format!("experience directory entry error: {error}"))?;
                let entry_path = entry.path();
                if entry_path.is_file() {
                    paths.push(entry_path);
                }
            }
            paths.sort();
            let mut book = Self::default();
            for entry_path in paths {
                let text = fs::read_to_string(&entry_path)
                    .map_err(|error| format!("experience book read error {}: {error}", entry_path.display()))?;
                book.merge(Self::parse(&text));
            }
            return Ok(book);
        }
        let text = fs::read_to_string(path).map_err(|error| format!("experience book read error: {error}"))?;
        Ok(Self::parse(&text))
    }

    pub fn parse(text: &str) -> Self {
        let mut book = Self::default();
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') || !line.starts_with("move\t") {
                continue;
            }
            if let Some(sample) = ExperienceSample::parse(line) {
                book.add_sample(sample);
            }
        }
        book
    }

    pub fn record_for(&self, key: &str, chess_move: ChessMove) -> Option<ExperienceRecord> {
        self.records.get(&(key.to_string(), chess_move.to_uci())).copied()
    }

    pub fn choose_move(
        &self,
        position: &Position,
        candidates: &[RootCandidate],
        min_games: u32,
        score_tolerance_cp: i32,
    ) -> Option<ExperienceDecision> {
        let best = candidates.first()?;
        let key = position.repetition_key();
        let threshold = best.score.saturating_sub(score_tolerance_cp.max(0));
        let mut best_experience: Option<ExperienceDecision> = None;

        for candidate in candidates.iter().copied() {
            if candidate.score < threshold {
                continue;
            }
            let Some(record) = self.record_for(&key, candidate.chess_move) else {
                continue;
            };
            if record.games < min_games.max(1) {
                continue;
            }
            let decision = ExperienceDecision {
                key: key.clone(),
                base_move: best.chess_move,
                base_score: best.score,
                chosen_move: candidate.chess_move,
                chosen_score: candidate.score,
                chosen_root_index: candidate.root_index,
                considered_candidates: candidates
                    .iter()
                    .filter(|item| item.score >= threshold)
                    .count(),
                record,
            };
            if experience_decision_is_better(&decision, best_experience.as_ref()) {
                best_experience = Some(decision);
            }
        }
        best_experience.filter(|decision| decision.chosen_move != best.chess_move || decision.record.games > 0)
    }

    fn add_sample(&mut self, sample: ExperienceSample) {
        let key = (sample.key, sample.chess_move);
        let record = self.records.entry(key).or_default();
        record.add_sample(sample.result, sample.loss_cp, sample.eval_error_cp);
    }

    fn merge(&mut self, other: ExperienceBook) {
        for (key, record) in other.records {
            let target = self.records.entry(key).or_default();
            target.games = target.games.saturating_add(record.games);
            target.wins = target.wins.saturating_add(record.wins);
            target.draws = target.draws.saturating_add(record.draws);
            target.losses = target.losses.saturating_add(record.losses);
            target.unknown_results = target.unknown_results.saturating_add(record.unknown_results);
            target.total_loss_cp = target.total_loss_cp.saturating_add(record.total_loss_cp);
            target.loss_samples = target.loss_samples.saturating_add(record.loss_samples);
            target.total_eval_error_cp = target.total_eval_error_cp.saturating_add(record.total_eval_error_cp);
            target.eval_error_samples = target.eval_error_samples.saturating_add(record.eval_error_samples);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperienceDecision {
    pub key: String,
    pub base_move: ChessMove,
    pub base_score: i32,
    pub chosen_move: ChessMove,
    pub chosen_score: i32,
    pub chosen_root_index: usize,
    pub considered_candidates: usize,
    pub record: ExperienceRecord,
}

impl ExperienceDecision {
    pub fn uci_info(&self) -> String {
        format!(
            "experience chose {} over {} within root scores {}/{}; {}; considered={}",
            self.chosen_move.to_uci(),
            self.base_move.to_uci(),
            self.chosen_score,
            self.base_score,
            self.record.compact_summary(),
            self.considered_candidates
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveResult {
    Win,
    Draw,
    Loss,
    Unknown,
}

impl MoveResult {
    fn from_str(value: &str) -> Self {
        match value {
            "win" => Self::Win,
            "draw" => Self::Draw,
            "loss" => Self::Loss,
            _ => Self::Unknown,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Win => "win",
            Self::Draw => "draw",
            Self::Loss => "loss",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExperienceSample {
    key: String,
    chess_move: String,
    result: MoveResult,
    loss_cp: Option<i32>,
    eval_error_cp: Option<i32>,
}

impl ExperienceSample {
    fn parse(line: &str) -> Option<Self> {
        let fields = parse_fields(line);
        let key = fields.get("key")?.to_string();
        let chess_move = fields.get("move")?.to_string();
        let result = fields
            .get("result")
            .map(|value| MoveResult::from_str(value))
            .unwrap_or(MoveResult::Unknown);
        let loss_cp = fields.get("loss_cp").and_then(|value| parse_optional_i32(value));
        let eval_error_cp = fields
            .get("eval_error_cp")
            .and_then(|value| parse_optional_i32(value));
        Some(Self {
            key,
            chess_move,
            result,
            loss_cp,
            eval_error_cp,
        })
    }
}

pub fn append_game_to_experience_book(
    path: impl AsRef<Path>,
    start_fen: &str,
    moves: &[ChessMove],
    result: &str,
    white_name: &str,
    black_name: &str,
    analysis: Option<&GameAnalysis>,
) -> Result<usize, String> {
    let requested_path = path.as_ref();
    let mut path_buf = requested_path.to_path_buf();
    if path_buf.exists() && path_buf.is_dir() {
        path_buf.push("rchess_experience.rxp");
    }
    let path = path_buf.as_path();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| format!("experience book dir error: {error}"))?;
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("experience book append error: {error}"))?;

    let mut position = Position::from_fen(start_fen.trim())?;
    writeln!(
        file,
        "game\tprotocol=rchess-experience-v1\tresult={}\twhite={}\tblack={}\tstart_fen={}",
        field(result),
        field(white_name),
        field(black_name),
        field(start_fen.trim())
    )
    .map_err(|error| format!("experience book write error: {error}"))?;

    let mut written = 0_usize;
    for (index, chess_move) in moves.iter().copied().enumerate() {
        let before = position.clone();
        let key = before.repetition_key();
        let side = before.side_to_move();
        let san = move_to_san(&before, chess_move).unwrap_or_else(|_| chess_move.to_uci());
        position.make_legal_move(chess_move)?;
        let analyzed_item = analysis
            .and_then(|analysis| analysis.items.get(index))
            .filter(|item| item.uci == chess_move.to_uci() && item.before_fen == before.to_fen());
        let detail = move_detail(analyzed_item, &before, &position);
        let mover_result = result_for_mover(result, side);
        let eval_error_cp = detail.loss_cp;
        writeln!(
            file,
            "move\tply={}\tside={}\tkey={}\tfen={}\tmove={}\tsan={}\tresult={}\tbefore_cp={}\tafter_cp={}\tloss_cp={}\taccuracy={}\teval_error_cp={}\treason={}",
            index + 1,
            color_code(side),
            field(&key),
            field(&before.to_fen()),
            chess_move.to_uci(),
            field(&san),
            mover_result.as_str(),
            opt_i32(detail.before_score_cp),
            opt_i32(detail.after_score_cp),
            opt_i32(detail.loss_cp),
            detail
                .accuracy
                .map(|value| format!("{value:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            opt_i32(eval_error_cp),
            field(&detail.reason),
        )
        .map_err(|error| format!("experience book write error: {error}"))?;
        written += 1;
    }
    writeln!(file, "endgame\tmoves={written}")
        .map_err(|error| format!("experience book write error: {error}"))?;
    Ok(written)
}

#[derive(Clone, Debug)]
struct MoveDetail {
    before_score_cp: Option<i32>,
    after_score_cp: Option<i32>,
    loss_cp: Option<i32>,
    accuracy: Option<f32>,
    reason: String,
}

fn move_detail(analysis_item: Option<&MoveAnalysis>, before: &Position, after: &Position) -> MoveDetail {
    if let Some(item) = analysis_item {
        return MoveDetail {
            before_score_cp: item.before_score_cp,
            after_score_cp: item.after_score_cp.map(|value| -value),
            loss_cp: item.loss_cp,
            accuracy: item.accuracy,
            reason: reason_from_loss(item.loss_cp),
        };
    }
    let before_score = evaluate_tactical_for_side_to_move(before);
    let after_raw = evaluate_tactical_for_side_to_move(after);
    let after_from_mover_view = -after_raw;
    let loss = (before_score - after_from_mover_view).max(0);
    MoveDetail {
        before_score_cp: Some(before_score),
        after_score_cp: Some(after_from_mover_view),
        loss_cp: Some(loss),
        accuracy: Some(move_accuracy_from_loss_cp(loss)),
        reason: reason_from_loss(Some(loss)),
    }
}

fn reason_from_loss(loss_cp: Option<i32>) -> String {
    match loss_cp.unwrap_or(0) {
        0..=49 => "held-engine-eval".to_string(),
        50..=149 => "small-eval-loss".to_string(),
        150..=299 => "medium-eval-loss".to_string(),
        300..=799 => "large-eval-loss".to_string(),
        _ => "decisive-eval-loss".to_string(),
    }
}

fn experience_decision_is_better(current: &ExperienceDecision, previous: Option<&ExperienceDecision>) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let current_key = (
        current.record.experience_score(),
        current.record.games,
        current.chosen_score,
        std::cmp::Reverse(current.chosen_root_index),
    );
    let previous_key = (
        previous.record.experience_score(),
        previous.record.games,
        previous.chosen_score,
        std::cmp::Reverse(previous.chosen_root_index),
    );
    current_key > previous_key
}

fn parse_fields(line: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for field in line.split('\t').skip(1) {
        if let Some((name, value)) = field.split_once('=') {
            result.insert(name.to_string(), unfield(value));
        }
    }
    result
}

fn field(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\t', "%09")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}

fn unfield(value: &str) -> String {
    value
        .replace("%09", "\t")
        .replace("%0A", "\n")
        .replace("%0D", "\r")
        .replace("%25", "%")
}

fn parse_optional_i32(value: &str) -> Option<i32> {
    if value == "-" {
        None
    } else {
        value.parse::<i32>().ok()
    }
}

fn opt_i32(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn result_for_mover(result: &str, side: Color) -> MoveResult {
    match (result, side) {
        ("1-0", Color::White) | ("0-1", Color::Black) => MoveResult::Win,
        ("1-0", Color::Black) | ("0-1", Color::White) => MoveResult::Loss,
        ("1/2-1/2", _) => MoveResult::Draw,
        _ => MoveResult::Unknown,
    }
}

fn color_code(color: Color) -> &'static str {
    match color {
        Color::White => "w",
        Color::Black => "b",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chess::STARTPOS_FEN;

    #[test]
    fn parses_move_samples_into_position_move_records() {
        let text = "move\tkey=abc\tmove=e2e4\tresult=win\tloss_cp=12\teval_error_cp=20\nmove\tkey=abc\tmove=e2e4\tresult=loss\tloss_cp=50\n";
        let book = ExperienceBook::parse(text);
        let record = book.records.get(&("abc".to_string(), "e2e4".to_string())).copied().unwrap();
        assert_eq!(record.games, 2);
        assert_eq!(record.wins, 1);
        assert_eq!(record.losses, 1);
        assert_eq!(record.average_loss_cp(), Some(31));
    }

    #[test]
    fn chooses_only_inside_search_tolerance() {
        let position = Position::startpos();
        let e4 = position.parse_uci_move("e2e4").unwrap();
        let d4 = position.parse_uci_move("d2d4").unwrap();
        let key = position.repetition_key();
        let mut book = ExperienceBook::default();
        book.add_sample(ExperienceSample {
            key,
            chess_move: d4.to_uci(),
            result: MoveResult::Win,
            loss_cp: Some(0),
            eval_error_cp: Some(0),
        });
        book.add_sample(ExperienceSample {
            key: position.repetition_key(),
            chess_move: d4.to_uci(),
            result: MoveResult::Win,
            loss_cp: Some(0),
            eval_error_cp: Some(0),
        });
        let candidates = vec![
            RootCandidate { root_index: 0, chess_move: e4, score: 20 },
            RootCandidate { root_index: 1, chess_move: d4, score: 5 },
        ];
        assert!(book.choose_move(&position, &candidates, 2, 10).is_none());
        let decision = book.choose_move(&position, &candidates, 2, 20).unwrap();
        assert_eq!(decision.chosen_move, d4);
    }

    #[test]
    fn appends_human_readable_experience_protocol() {
        let temp = std::env::temp_dir().join("rchess_experience_test.rxp");
        let _ = fs::remove_file(&temp);
        let position = Position::startpos();
        let chess_move = position.parse_uci_move("e2e4").unwrap();
        append_game_to_experience_book(&temp, STARTPOS_FEN, &[chess_move], "1-0", "white", "black", None).unwrap();
        let text = fs::read_to_string(&temp).unwrap();
        assert!(text.contains("protocol=rchess-experience-v1"));
        assert!(text.contains("move=e2e4"));
        assert!(text.contains("result=win"));
        let _ = fs::remove_file(&temp);
    }
}
