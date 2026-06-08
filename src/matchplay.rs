use std::collections::BTreeMap;

use crate::chess::{ChessMove, Color, Position, STARTPOS_FEN};
use crate::pgn::export_pgn_with_tags;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchLimit {
    pub depth: Option<u8>,
    pub movetime_ms: Option<u64>,
}

impl SearchLimit {
    pub fn depth(depth: u8) -> Self {
        Self {
            depth: Some(depth.max(1)),
            movetime_ms: None,
        }
    }

    pub fn movetime(movetime_ms: u64) -> Self {
        Self {
            depth: None,
            movetime_ms: Some(movetime_ms.max(1)),
        }
    }

    pub fn depth_or_movetime(depth: u8, movetime_ms: u64) -> Self {
        if movetime_ms > 0 {
            Self::movetime(movetime_ms)
        } else {
            Self::depth(depth)
        }
    }

    pub fn go_command(&self) -> String {
        if let Some(movetime_ms) = self.movetime_ms {
            format!("go movetime {movetime_ms}")
        } else {
            format!("go depth {}", self.depth.unwrap_or(1).max(1))
        }
    }
}

impl Default for SearchLimit {
    fn default() -> Self {
        Self::depth(4)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UciEngineSlot {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub limit: SearchLimit,
}

impl UciEngineSlot {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            limit: SearchLimit::default(),
        }
    }

    pub fn with_limit(mut self, limit: SearchLimit) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchStatus {
    Ready,
    Thinking(Color),
    Finished(String),
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineMatchController {
    pub start_fen: String,
    pub position: Position,
    pub played_moves: Vec<ChessMove>,
    pub white: UciEngineSlot,
    pub black: UciEngineSlot,
    pub status: MatchStatus,
    pub result: String,
}

impl EngineMatchController {
    pub fn new(white: UciEngineSlot, black: UciEngineSlot) -> Self {
        Self::from_fen(STARTPOS_FEN, white, black).expect("STARTPOS_FEN must be valid")
    }

    pub fn from_fen(
        start_fen: &str,
        white: UciEngineSlot,
        black: UciEngineSlot,
    ) -> Result<Self, String> {
        let position = Position::from_fen(start_fen.trim())?;
        Ok(Self {
            start_fen: position.to_fen(),
            position,
            played_moves: Vec::new(),
            white,
            black,
            status: MatchStatus::Ready,
            result: "*".to_string(),
        })
    }

    pub fn current_slot(&self) -> &UciEngineSlot {
        match self.position.side_to_move() {
            Color::White => &self.white,
            Color::Black => &self.black,
        }
    }

    pub fn current_go_command(&self) -> String {
        self.current_slot().limit.go_command()
    }

    pub fn position_command(&self) -> String {
        format!("position fen {}", self.position.to_fen())
    }

    pub fn record_bestmove(&mut self, bestmove: &str) -> Result<(), String> {
        let move_text = bestmove
            .strip_prefix("bestmove ")
            .unwrap_or(bestmove)
            .split_whitespace()
            .next()
            .unwrap_or("0000");
        if move_text == "0000" {
            self.finish_if_terminal();
            if self.result == "*" {
                self.status = MatchStatus::Error("engine returned 0000 before terminal position".to_string());
            }
            return Ok(());
        }

        let chess_move = self
            .position
            .parse_uci_move(move_text)
            .ok_or_else(|| format!("illegal UCI move from match engine: {move_text}"))?;
        self.position.make_legal_move(chess_move)?;
        self.played_moves.push(chess_move);
        self.finish_if_terminal();
        if self.result == "*" {
            self.status = MatchStatus::Ready;
        }
        Ok(())
    }

    pub fn start_thinking(&mut self) {
        self.status = MatchStatus::Thinking(self.position.side_to_move());
    }

    pub fn pgn_log(&self) -> Result<String, String> {
        let mut tags = BTreeMap::new();
        tags.insert("Event".to_string(), "rchess engine match".to_string());
        tags.insert("Site".to_string(), "?".to_string());
        tags.insert("Date".to_string(), "????.??.??".to_string());
        tags.insert("Round".to_string(), "?".to_string());
        tags.insert("White".to_string(), self.white.name.clone());
        tags.insert("Black".to_string(), self.black.name.clone());
        tags.insert("Result".to_string(), self.result.clone());
        if self.start_fen.trim() != STARTPOS_FEN {
            tags.insert("SetUp".to_string(), "1".to_string());
            tags.insert("FEN".to_string(), self.start_fen.clone());
        }
        export_pgn_with_tags(&self.start_fen, &self.played_moves, &self.result, &tags)
    }

    fn finish_if_terminal(&mut self) {
        if self.position.is_checkmate() {
            self.result = match self.position.side_to_move() {
                Color::White => "0-1".to_string(),
                Color::Black => "1-0".to_string(),
            };
            self.status = MatchStatus::Finished(self.result.clone());
        } else if self.position.is_stalemate() {
            self.result = "1/2-1/2".to_string();
            self.status = MatchStatus::Finished(self.result.clone());
        } else {
            self.result = "*".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_limit_formats_uci_go_commands() {
        assert_eq!(SearchLimit::depth(5).go_command(), "go depth 5");
        assert_eq!(SearchLimit::movetime(250).go_command(), "go movetime 250");
        assert_eq!(SearchLimit::depth_or_movetime(3, 0).go_command(), "go depth 3");
    }

    #[test]
    fn controller_records_moves_and_exports_pgn_log() {
        let white = UciEngineSlot::new("rchess-a", "rchess").with_limit(SearchLimit::depth(2));
        let black = UciEngineSlot::new("rchess-b", "rchess").with_limit(SearchLimit::movetime(100));
        let mut controller = EngineMatchController::new(white, black);

        assert_eq!(controller.current_slot().name, "rchess-a");
        assert_eq!(controller.current_go_command(), "go depth 2");
        controller.record_bestmove("bestmove e2e4").unwrap();
        assert_eq!(controller.current_slot().name, "rchess-b");
        assert_eq!(controller.current_go_command(), "go movetime 100");
        controller.record_bestmove("e7e5").unwrap();

        let pgn = controller.pgn_log().unwrap();
        assert!(pgn.contains("[White \"rchess-a\"]"));
        assert!(pgn.contains("[Black \"rchess-b\"]"));
        assert!(pgn.contains("1. e4 e5 *"));
    }
}
