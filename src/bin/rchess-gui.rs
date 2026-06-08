use std::collections::VecDeque;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui;
use rchess::analysis::{format_accuracy, format_cp, format_cp_value, AnalysisJob, GameAnalysis};
use rchess::chess::{square_name, ChessMove, Color, PieceKind, Position, STARTPOS_FEN};
use rchess::matchplay::{EngineMatchController, SearchLimit, UciEngineSlot};
use rchess::pgn::{export_pgn, move_to_san, parse_pgn, position_after_moves};
use rchess::search::evaluate_for_side_to_move;

fn main() -> eframe::Result<()> {
    if env::args().any(|arg| arg == "--engine-mode") {
        rchess::uci::run();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1120.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "rchess GUI",
        options,
        Box::new(|cc| Ok(Box::new(RChessGui::new(cc)))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EngineBackend {
    RChess,
    Stockfish10,
    CustomUci,
}

impl EngineBackend {
    fn label(self) -> &'static str {
        match self {
            EngineBackend::RChess => "rchess internal UCI",
            EngineBackend::Stockfish10 => "Stockfish 10",
            EngineBackend::CustomUci => "Custom UCI executable",
        }
    }
}

struct RChessGui {
    position: Position,
    fen_input: String,
    game_start_fen: String,
    pgn_text: String,
    pgn_path: String,
    selected: Option<u8>,
    selected_moves: Vec<ChessMove>,
    dragging_from: Option<u8>,
    drag_pointer: Option<egui::Pos2>,
    promotion_request: Option<PromotionRequest>,
    played_moves: Vec<ChessMove>,
    redo_moves: Vec<ChessMove>,
    history_view_ply: Option<usize>,
    player_color: Color,
    auto_engine: bool,
    flipped: bool,
    search_depth: u8,
    pending_engine: bool,
    engine_status: String,
    game_status: String,
    engine_backend: EngineBackend,
    stockfish10_path: String,
    stockfish10_status: String,
    engine_path: String,
    engine: Option<UciEngine>,
    engine_rx: Option<Receiver<String>>,
    engine_log: Vec<String>,
    last_engine_info: String,
    last_engine_score_cp: Option<i32>,
    planned_threads: u16,
    planned_hash_mb: u32,
    resource_settings_status: String,
    match_white_path: String,
    match_black_path: String,
    match_depth: u8,
    match_movetime_ms: u64,
    match_max_plies: u32,
    match_controller: Option<EngineMatchController>,
    match_white_engine: Option<UciEngine>,
    match_black_engine: Option<UciEngine>,
    match_white_rx: Option<Receiver<String>>,
    match_black_rx: Option<Receiver<String>>,
    match_waiting_for: Option<Color>,
    match_running: bool,
    match_status: String,
    match_log: Vec<String>,
    match_pgn_text: String,
    analysis_depth: u8,
    analysis: Option<GameAnalysis>,
    analysis_engine: Option<UciEngine>,
    analysis_rx: Option<Receiver<String>>,
    analysis_jobs: VecDeque<AnalysisJob>,
    analysis_current_job: Option<AnalysisJob>,
    analysis_last_score_cp: Option<i32>,
    analysis_running: bool,
    analysis_status: String,
    analysis_log: Vec<String>,
}

#[derive(Clone)]
struct PromotionRequest {
    from: u8,
    to: u8,
    options: Vec<ChessMove>,
}

impl RChessGui {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let position = Position::startpos();
        let detected_stockfish10 = detect_stockfish10_path().unwrap_or_default();
        let stockfish10_status = if detected_stockfish10.is_empty() {
            "Stockfish 10 binary not detected yet".to_string()
        } else {
            format!("Stockfish 10 candidate: {detected_stockfish10}")
        };
        let mut app = Self {
            fen_input: STARTPOS_FEN.to_string(),
            game_start_fen: STARTPOS_FEN.to_string(),
            pgn_text: String::new(),
            pgn_path: String::new(),
            position,
            selected: None,
            selected_moves: Vec::new(),
            dragging_from: None,
            drag_pointer: None,
            promotion_request: None,
            played_moves: Vec::new(),
            redo_moves: Vec::new(),
            history_view_ply: None,
            player_color: Color::White,
            auto_engine: true,
            flipped: false,
            search_depth: 4,
            pending_engine: false,
            engine_status: "UCI child process is not started yet".to_string(),
            game_status: String::new(),
            engine_backend: EngineBackend::RChess,
            stockfish10_path: detected_stockfish10,
            stockfish10_status,
            engine_path: String::new(),
            engine: None,
            engine_rx: None,
            engine_log: Vec::new(),
            last_engine_info: String::new(),
            last_engine_score_cp: None,
            planned_threads: 1,
            planned_hash_mb: 64,
            resource_settings_status: "Resource controls are placeholders; current search remains single-threaded".to_string(),
            match_white_path: String::new(),
            match_black_path: String::new(),
            match_depth: 3,
            match_movetime_ms: 0,
            match_max_plies: 160,
            match_controller: None,
            match_white_engine: None,
            match_black_engine: None,
            match_white_rx: None,
            match_black_rx: None,
            match_waiting_for: None,
            match_running: false,
            match_status: "Engine match is idle".to_string(),
            match_log: Vec::new(),
            match_pgn_text: String::new(),
            analysis_depth: 3,
            analysis: None,
            analysis_engine: None,
            analysis_rx: None,
            analysis_jobs: VecDeque::new(),
            analysis_current_job: None,
            analysis_last_score_cp: None,
            analysis_running: false,
            analysis_status: "Analysis is idle".to_string(),
            analysis_log: Vec::new(),
        };
        app.refresh_game_status();
        app
    }

    fn new_game(&mut self) {
        self.stop_analysis("Analysis stopped by new game");
        self.stop_engine_match("Engine match stopped by new game");
        self.position = Position::startpos();
        self.fen_input = STARTPOS_FEN.to_string();
        self.game_start_fen = STARTPOS_FEN.to_string();
        self.selected = None;
        self.selected_moves.clear();
        self.dragging_from = None;
        self.drag_pointer = None;
        self.promotion_request = None;
        self.played_moves.clear();
        self.redo_moves.clear();
        self.history_view_ply = None;
        self.pgn_text.clear();
        self.pending_engine = false;
        self.last_engine_score_cp = None;
        self.engine_status = "New game".to_string();
        self.send_to_engine("ucinewgame");
        self.refresh_game_status();

        if self.should_auto_engine_move() {
            self.request_engine_move();
        }
    }

    fn load_fen(&mut self) {
        self.stop_analysis("Analysis stopped by FEN load");
        self.stop_engine_match("Engine match stopped by FEN load");
        match Position::from_fen(self.fen_input.trim()) {
            Ok(position) => {
                self.position = position;
                self.game_start_fen = self.position.to_fen();
                self.pgn_text.clear();
                self.selected = None;
                self.selected_moves.clear();
                self.dragging_from = None;
                self.drag_pointer = None;
                self.promotion_request = None;
                self.played_moves.clear();
                self.redo_moves.clear();
                self.history_view_ply = None;
                self.pending_engine = false;
                self.last_engine_score_cp = None;
                self.engine_status = "FEN loaded".to_string();
                self.refresh_game_status();
            }
            Err(error) => {
                self.engine_status = format!("FEN error: {error}");
            }
        }
    }

    fn export_pgn_to_text(&mut self) {
        let result = self.current_result();
        match export_pgn(&self.game_start_fen, &self.played_moves, &result) {
            Ok(text) => {
                self.pgn_text = text;
                self.engine_status = "PGN exported".to_string();
            }
            Err(error) => {
                self.engine_status = format!("PGN export error: {error}");
            }
        }
    }

    fn copy_pgn_to_clipboard(&mut self, ctx: &egui::Context) {
        if self.pgn_text.trim().is_empty() {
            self.export_pgn_to_text();
        }
        ctx.copy_text(self.pgn_text.clone());
        self.engine_status = "PGN copied to clipboard".to_string();
    }

    fn save_pgn_to_file(&mut self) {
        let path = self.pgn_path.trim().to_string();
        if path.is_empty() {
            self.engine_status = "PGN path is empty".to_string();
            return;
        }
        if self.pgn_text.trim().is_empty() {
            self.export_pgn_to_text();
        }
        match fs::write(&path, &self.pgn_text) {
            Ok(()) => self.engine_status = format!("PGN saved to {path}"),
            Err(error) => self.engine_status = format!("PGN save error: {error}"),
        }
    }

    fn open_pgn_from_file(&mut self) {
        let path = self.pgn_path.trim().to_string();
        if path.is_empty() {
            self.engine_status = "PGN path is empty".to_string();
            return;
        }
        match fs::read_to_string(&path) {
            Ok(text) => {
                self.pgn_text = text;
                self.load_pgn_from_text();
            }
            Err(error) => self.engine_status = format!("PGN open error: {error}"),
        }
    }

    fn load_pgn_from_text(&mut self) {
        self.stop_analysis("Analysis stopped by PGN load");
        self.stop_engine_match("Engine match stopped by PGN load");
        match parse_pgn(&self.pgn_text).and_then(|game| {
            let position = position_after_moves(&game.start_fen, &game.moves)?;
            Ok((game, position))
        }) {
            Ok((game, position)) => {
                self.game_start_fen = game.start_fen;
                self.played_moves = game.moves;
                self.redo_moves.clear();
                self.history_view_ply = None;
                self.position = position;
                self.fen_input = self.position.to_fen();
                self.selected = None;
                self.selected_moves.clear();
                self.dragging_from = None;
                self.drag_pointer = None;
                self.promotion_request = None;
                self.pending_engine = false;
                self.last_engine_score_cp = None;
                self.engine_status = format!("PGN loaded, result {}", game.result);
                self.refresh_game_status();

                if self.should_auto_engine_move() {
                    self.request_engine_move();
                }
            }
            Err(error) => {
                self.engine_status = format!("PGN error: {error}");
            }
        }
    }

    fn current_result(&self) -> String {
        if self.position.is_checkmate() {
            match self.position.side_to_move() {
                Color::White => "0-1".to_string(),
                Color::Black => "1-0".to_string(),
            }
        } else if self.position.is_stalemate() {
            "1/2-1/2".to_string()
        } else {
            "*".to_string()
        }
    }

    fn select_square(&mut self, square: u8) {
        if self.pending_engine || self.match_running || self.promotion_request.is_some() || !self.is_history_view_live() {
            return;
        }

        if let Some(selected) = self.selected {
            if selected == square {
                self.clear_selection();
                return;
            }

            if self.try_apply_selected_to(square) {
                return;
            }
        }

        if self.select_piece(square) {
            return;
        }

        self.clear_selection();
    }

    fn select_piece(&mut self, square: u8) -> bool {
        if self.pending_engine || self.match_running || self.promotion_request.is_some() || !self.is_history_view_live() {
            return false;
        }
        let Some(piece) = self.position.piece_at(square) else {
            return false;
        };
        if piece.color != self.position.side_to_move() {
            return false;
        }

        self.selected = Some(square);
        self.selected_moves = self
            .position
            .legal_moves()
            .into_iter()
            .filter(|chess_move| chess_move.from == square)
            .collect();
        true
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.selected_moves.clear();
        self.dragging_from = None;
        self.drag_pointer = None;
    }

    fn should_auto_engine_move(&self) -> bool {
        self.auto_engine
            && !self.pending_engine
            && !self.match_running
            && self.position.side_to_move() != self.player_color
            && !self.position.is_checkmate()
            && !self.position.is_stalemate()
    }

    fn try_apply_selected_to(&mut self, to: u8) -> bool {
        if !self.is_history_view_live() {
            self.engine_status = "Return to live position before making a move".to_string();
            return false;
        }
        let Some(from) = self.selected else {
            return false;
        };
        let candidates: Vec<ChessMove> = self
            .selected_moves
            .iter()
            .copied()
            .filter(|chess_move| chess_move.to == to)
            .collect();
        if candidates.is_empty() {
            return false;
        }

        if candidates.iter().any(|chess_move| chess_move.promotion.is_some()) {
            self.promotion_request = Some(PromotionRequest { from, to, options: candidates });
            self.engine_status = "Choose promotion piece".to_string();
        } else {
            self.apply_user_move(candidates[0]);
        }
        true
    }

    fn apply_promotion_choice(&mut self, kind: PieceKind) {
        let Some(request) = self.promotion_request.take() else {
            return;
        };
        match request.options.into_iter().find(|chess_move| chess_move.promotion == Some(kind)) {
            Some(chess_move) => self.apply_user_move(chess_move),
            None => self.engine_status = "Selected promotion is not legal".to_string(),
        }
    }

    fn apply_user_move(&mut self, chess_move: ChessMove) {
        match self.position.make_legal_move(chess_move) {
            Ok(()) => {
                self.record_applied_move(chess_move);
                self.clear_selection();
                self.promotion_request = None;
                self.refresh_game_status();

                if self.should_auto_engine_move() {
                    self.request_engine_move();
                }
            }
            Err(error) => {
                self.engine_status = error;
            }
        }
    }

    fn record_applied_move(&mut self, chess_move: ChessMove) {
        self.played_moves.push(chess_move);
        self.redo_moves.clear();
        self.history_view_ply = None;
        self.last_engine_score_cp = None;
        self.fen_input = self.position.to_fen();
    }

    fn rebuild_position_from_history(&mut self) -> Result<(), String> {
        let mut position = Position::from_fen(&self.game_start_fen)?;
        for chess_move in &self.played_moves {
            position.make_legal_move(*chess_move)?;
        }
        self.position = position;
        self.fen_input = self.position.to_fen();
        self.history_view_ply = None;
        self.clear_selection();
        self.promotion_request = None;
        self.refresh_game_status();
        Ok(())
    }

    fn undo_move(&mut self) {
        if self.pending_engine || self.match_running {
            self.engine_status = "Cannot undo while an engine is thinking".to_string();
            return;
        }
        let Some(chess_move) = self.played_moves.pop() else {
            self.engine_status = "Nothing to undo".to_string();
            return;
        };
        self.redo_moves.push(chess_move);
        match self.rebuild_position_from_history() {
            Ok(()) => self.engine_status = format!("Undid {}", chess_move.to_uci()),
            Err(error) => self.engine_status = format!("Undo error: {error}"),
        }
    }

    fn redo_move(&mut self) {
        if self.pending_engine || self.match_running {
            self.engine_status = "Cannot redo while an engine is thinking".to_string();
            return;
        }
        let Some(chess_move) = self.redo_moves.pop() else {
            self.engine_status = "Nothing to redo".to_string();
            return;
        };
        match self.position.make_legal_move(chess_move) {
            Ok(()) => {
                self.played_moves.push(chess_move);
                self.history_view_ply = None;
                self.last_engine_score_cp = None;
                self.fen_input = self.position.to_fen();
                self.clear_selection();
                self.promotion_request = None;
                self.refresh_game_status();
                self.engine_status = format!("Redid {}", chess_move.to_uci());
            }
            Err(error) => {
                self.redo_moves.push(chess_move);
                self.engine_status = format!("Redo error: {error}");
            }
        }
    }

    fn stop_engine(&mut self, status: impl Into<String>) {
        self.engine = None;
        self.engine_rx = None;
        self.pending_engine = false;
        self.last_engine_score_cp = None;
        self.engine_status = status.into();
    }

    fn current_engine_command(&self) -> Result<UciCommand, String> {
        match self.engine_backend {
            EngineBackend::RChess => {
                let current = env::current_exe()
                    .map_err(|error| format!("cannot locate current executable: {error}"))?;
                Ok(UciCommand {
                    label: EngineBackend::RChess.label().to_string(),
                    program: current,
                    args: vec!["--engine-mode".to_string()],
                })
            }
            EngineBackend::Stockfish10 => {
                let path = normalize_path_input(&self.stockfish10_path);
                if path.is_empty() {
                    return Err("Stockfish 10 path is empty. Build it from third_party/stockfish-sf_10/src or point to an existing stockfish executable.".to_string());
                }
                validate_executable_path(&path, "Stockfish 10")?;
                Ok(UciCommand {
                    label: EngineBackend::Stockfish10.label().to_string(),
                    program: PathBuf::from(path),
                    args: Vec::new(),
                })
            }
            EngineBackend::CustomUci => {
                let path = normalize_path_input(&self.engine_path);
                if path.is_empty() {
                    return Err("Custom UCI executable path is empty".to_string());
                }
                validate_executable_path(&path, "custom UCI engine")?;
                Ok(UciCommand {
                    label: EngineBackend::CustomUci.label().to_string(),
                    program: PathBuf::from(path),
                    args: Vec::new(),
                })
            }
        }
    }

    fn request_engine_move(&mut self) {
        if self.pending_engine || self.match_running {
            return;
        }
        if self.position.is_checkmate() || self.position.is_stalemate() {
            self.refresh_game_status();
            return;
        }
        if let Err(error) = self.ensure_engine() {
            self.engine_status = error;
            return;
        }

        self.pending_engine = true;
        self.engine_status = format!("Engine is thinking at depth {}", self.search_depth);
        self.send_to_engine(&format!("position fen {}", self.position.to_fen()));
        self.send_to_engine(&format!("go depth {}", self.search_depth));
    }

    fn ensure_engine(&mut self) -> Result<(), String> {
        if self.engine.is_some() {
            return Ok(());
        }

        let command = self.current_engine_command()?;
        let label = command.label.clone();
        let (engine, rx) = UciEngine::spawn(command)?;
        self.engine = Some(engine);
        self.engine_rx = Some(rx);
        self.send_to_engine("uci");
        self.send_to_engine("isready");
        self.engine_status = format!("UCI child process started: {label}");
        Ok(())
    }

    fn send_to_engine(&mut self, command: &str) {
        if let Some(engine) = &mut self.engine {
            if let Err(error) = engine.send(command) {
                self.engine_status = format!("UCI write error: {error}");
                self.engine = None;
                self.engine_rx = None;
                self.pending_engine = false;
            }
        }
    }

    fn poll_engine(&mut self) {
        let mut lines = Vec::new();
        if let Some(rx) = &self.engine_rx {
            while let Ok(line) = rx.try_recv() {
                lines.push(line);
            }
        }

        for line in lines {
            self.handle_engine_line(line);
        }
    }

    fn handle_engine_line(&mut self, line: String) {
        self.engine_log.push(line.clone());
        if self.engine_log.len() > 160 {
            let extra = self.engine_log.len() - 160;
            self.engine_log.drain(0..extra);
        }

        if let Some(name) = line.strip_prefix("id name ") {
            self.engine_status = format!("UCI engine: {name}");
            return;
        }
        if line == "uciok" {
            self.engine_status = format!("UCI handshake complete: {}", self.engine_backend.label());
            return;
        }
        if line == "readyok" {
            self.engine_status = "Engine is ready".to_string();
            return;
        }
        if line.starts_with("info ") {
            self.last_engine_info = compact_uci_info_line(&line);
            if let Some(score_cp) = parse_uci_score_cp(&line) {
                self.last_engine_score_cp = Some(score_cp);
            }
            return;
        }

        let Some(rest) = line.strip_prefix("bestmove ") else {
            return;
        };
        self.pending_engine = false;
        let move_text = rest.split_whitespace().next().unwrap_or("0000");
        if move_text == "0000" {
            self.engine_status = "Engine returned no legal move".to_string();
            self.refresh_game_status();
            return;
        }

        match self.position.parse_uci_move(move_text) {
            Some(chess_move) => match self.position.make_legal_move(chess_move) {
                Ok(()) => {
                    self.record_applied_move(chess_move);
                    self.engine_status = format!("Engine played {move_text}");
                    self.clear_selection();
                    self.refresh_game_status();
                }
                Err(error) => {
                    self.engine_status = format!("Engine move apply error: {error}");
                }
            },
            None => {
                self.engine_status = format!("Engine returned illegal move: {move_text}");
            }
        }
    }


    fn start_engine_match(&mut self) {
        if self.pending_engine {
            self.engine_status = "Stop the single-engine search before starting a match".to_string();
            return;
        }
        self.stop_engine_match("Restarting engine match");

        let white_command = match self.match_command(&self.match_white_path, "White") {
            Ok(command) => command,
            Err(error) => {
                self.match_status = error;
                return;
            }
        };
        let black_command = match self.match_command(&self.match_black_path, "Black") {
            Ok(command) => command,
            Err(error) => {
                self.match_status = error;
                return;
            }
        };

        let limit = SearchLimit::depth_or_movetime(self.match_depth, self.match_movetime_ms);
        let white_name = white_command.label.clone();
        let black_name = black_command.label.clone();
        let white_slot = UciEngineSlot::new(white_name.clone(), white_command.program.to_string_lossy().to_string())
            .with_args(white_command.args.clone())
            .with_limit(limit.clone());
        let black_slot = UciEngineSlot::new(black_name.clone(), black_command.program.to_string_lossy().to_string())
            .with_args(black_command.args.clone())
            .with_limit(limit);

        let start_fen = self.position.to_fen();
        let controller = match EngineMatchController::from_fen(&start_fen, white_slot, black_slot) {
            Ok(controller) => controller,
            Err(error) => {
                self.match_status = format!("Match start FEN error: {error}");
                return;
            }
        };

        let (white_engine, white_rx) = match UciEngine::spawn(white_command) {
            Ok(value) => value,
            Err(error) => {
                self.match_status = format!("White engine start error: {error}");
                return;
            }
        };
        let (black_engine, black_rx) = match UciEngine::spawn(black_command) {
            Ok(value) => value,
            Err(error) => {
                self.match_status = format!("Black engine start error: {error}");
                return;
            }
        };

        self.game_start_fen = start_fen;
        self.played_moves.clear();
        self.redo_moves.clear();
        self.history_view_ply = None;
        self.match_controller = Some(controller);
        self.match_white_engine = Some(white_engine);
        self.match_black_engine = Some(black_engine);
        self.match_white_rx = Some(white_rx);
        self.match_black_rx = Some(black_rx);
        self.match_waiting_for = None;
        self.match_running = true;
        self.match_log.clear();
        self.match_pgn_text.clear();
        self.match_status = format!("Match started: {white_name} vs {black_name}");

        let _ = self.send_to_match_engine(Color::White, "uci");
        let _ = self.send_to_match_engine(Color::White, "isready");
        let _ = self.send_to_match_engine(Color::Black, "uci");
        let _ = self.send_to_match_engine(Color::Black, "isready");
        self.request_next_match_move();
    }

    fn match_command(&self, path: &str, color_label: &str) -> Result<UciCommand, String> {
        let normalized = normalize_path_input(path);
        if normalized.is_empty() {
            let current = env::current_exe()
                .map_err(|error| format!("cannot locate current executable for {color_label}: {error}"))?;
            Ok(UciCommand {
                label: format!("rchess {color_label}"),
                program: current,
                args: vec!["--engine-mode".to_string()],
            })
        } else {
            validate_executable_path(&normalized, color_label)?;
            let program = PathBuf::from(&normalized);
            let label = program
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| format!("{color_label} {value}"))
                .unwrap_or_else(|| format!("{color_label} UCI"));
            Ok(UciCommand { label, program, args: Vec::new() })
        }
    }

    fn stop_engine_match(&mut self, status: impl Into<String>) {
        self.match_white_engine = None;
        self.match_black_engine = None;
        self.match_white_rx = None;
        self.match_black_rx = None;
        self.match_waiting_for = None;
        self.match_running = false;
        self.match_status = status.into();
    }

    fn request_next_match_move(&mut self) {
        if !self.match_running || self.match_waiting_for.is_some() {
            return;
        }

        let Some(controller) = self.match_controller.as_mut() else {
            self.match_status = "No match controller".to_string();
            self.match_running = false;
            return;
        };

        if controller.result != "*" || controller.position.is_checkmate() || controller.position.is_stalemate() {
            self.match_running = false;
            self.match_status = format!("Match finished: {}", controller.result);
            self.update_match_pgn_text();
            return;
        }
        if controller.played_moves.len() as u32 >= self.match_max_plies {
            self.match_running = false;
            self.match_status = format!("Match stopped after {} plies", self.match_max_plies);
            self.update_match_pgn_text();
            return;
        }

        let color = controller.position.side_to_move();
        let position_command = controller.position_command();
        let go_command = controller.current_go_command();
        controller.start_thinking();

        if let Err(error) = self.send_to_match_engine(color, &position_command) {
            self.match_status = error;
            self.match_running = false;
            return;
        }
        if let Err(error) = self.send_to_match_engine(color, &go_command) {
            self.match_status = error;
            self.match_running = false;
            return;
        }
        self.match_waiting_for = Some(color);
        self.match_status = format!("{} engine is thinking: {go_command}", color_name(color));
    }

    fn send_to_match_engine(&mut self, color: Color, command: &str) -> Result<(), String> {
        let engine = match color {
            Color::White => &mut self.match_white_engine,
            Color::Black => &mut self.match_black_engine,
        };
        let Some(engine) = engine else {
            return Err(format!("{} match engine is not running", color_name(color)));
        };
        engine
            .send(command)
            .map_err(|error| format!("{} match engine write error: {error}", color_name(color)))
    }

    fn poll_match_engines(&mut self) {
        let mut white_lines = Vec::new();
        if let Some(rx) = &self.match_white_rx {
            while let Ok(line) = rx.try_recv() {
                white_lines.push(line);
            }
        }
        for line in white_lines {
            self.handle_match_engine_line(Color::White, line);
        }

        let mut black_lines = Vec::new();
        if let Some(rx) = &self.match_black_rx {
            while let Ok(line) = rx.try_recv() {
                black_lines.push(line);
            }
        }
        for line in black_lines {
            self.handle_match_engine_line(Color::Black, line);
        }
    }

    fn handle_match_engine_line(&mut self, color: Color, line: String) {
        self.match_log.push(format!("[{}] {line}", color_name(color)));
        if self.match_log.len() > 220 {
            let extra = self.match_log.len() - 220;
            self.match_log.drain(0..extra);
        }

        if let Some(name) = line.strip_prefix("id name ") {
            self.match_status = format!("{} engine: {name}", color_name(color));
            return;
        }
        if line == "uciok" || line == "readyok" || line.starts_with("info ") {
            if line.starts_with("info ") {
                self.match_status = format!("{} {}", color_name(color), compact_uci_info_line(&line));
            }
            return;
        }

        if !line.starts_with("bestmove ") {
            return;
        }
        if self.match_waiting_for != Some(color) {
            self.match_status = format!("Ignored out-of-turn bestmove from {}", color_name(color));
            return;
        }
        self.match_waiting_for = None;

        let result = if let Some(controller) = self.match_controller.as_mut() {
            controller.record_bestmove(&line).map(|_| {
                (
                    controller.position.clone(),
                    controller.played_moves.clone(),
                    controller.result.clone(),
                )
            })
        } else {
            Err("No match controller".to_string())
        };

        match result {
            Ok((position, moves, result)) => {
                self.position = position;
                self.played_moves = moves;
                self.redo_moves.clear();
                self.history_view_ply = None;
                self.fen_input = self.position.to_fen();
                self.refresh_game_status();
                self.update_match_pgn_text();
                if result == "*" {
                    self.request_next_match_move();
                } else {
                    self.match_running = false;
                    self.match_status = format!("Match finished: {result}");
                }
            }
            Err(error) => {
                self.match_running = false;
                self.match_status = format!("Match move error: {error}");
            }
        }
    }

    fn update_match_pgn_text(&mut self) {
        if let Some(controller) = &self.match_controller {
            match controller.pgn_log() {
                Ok(text) => {
                    self.match_pgn_text = text;
                    self.pgn_text = self.match_pgn_text.clone();
                }
                Err(error) => self.match_status = format!("Match PGN error: {error}"),
            }
        }
    }


    fn start_game_analysis(&mut self) {
        if self.match_running {
            self.analysis_status = "Stop engine-vs-engine match before analysis".to_string();
            return;
        }
        self.stop_analysis("Restarting analysis");

        let (start_fen, moves) = match self.analysis_source_history() {
            Ok(value) => value,
            Err(error) => {
                self.analysis_status = error;
                return;
            }
        };
        if moves.is_empty() {
            self.analysis_status = "No moves to analyse".to_string();
            return;
        }

        let analysis = match GameAnalysis::from_history(&start_fen, &moves) {
            Ok(analysis) => analysis,
            Err(error) => {
                self.analysis_status = format!("Analysis setup error: {error}");
                return;
            }
        };
        match position_after_moves(&start_fen, &moves) {
            Ok(position) => {
                self.game_start_fen = start_fen.clone();
                self.played_moves = moves.clone();
                self.redo_moves.clear();
                self.position = position;
                self.fen_input = self.position.to_fen();
                self.history_view_ply = None;
                self.refresh_game_status();
            }
            Err(error) => {
                self.analysis_status = format!("Analysis position error: {error}");
                return;
            }
        }
        let jobs: VecDeque<AnalysisJob> = analysis.jobs().into_iter().collect();
        if jobs.is_empty() {
            self.analysis_status = "No analysis jobs".to_string();
            return;
        }

        let command = match self.current_engine_command() {
            Ok(command) => command,
            Err(error) => {
                self.analysis_status = format!("Analysis engine error: {error}");
                return;
            }
        };
        let label = command.label.clone();
        let (engine, rx) = match UciEngine::spawn(command) {
            Ok(value) => value,
            Err(error) => {
                self.analysis_status = format!("Analysis engine start error: {error}");
                return;
            }
        };

        self.analysis = Some(analysis);
        self.analysis_jobs = jobs;
        self.analysis_engine = Some(engine);
        self.analysis_rx = Some(rx);
        self.analysis_current_job = None;
        self.analysis_last_score_cp = None;
        self.analysis_running = true;
        self.analysis_log.clear();
        self.analysis_status = format!("Analysing with {label} at depth {}", self.analysis_depth);
        self.send_to_analysis_engine("uci");
        self.send_to_analysis_engine("isready");
        self.request_next_analysis_job();
    }

    fn analysis_source_history(&self) -> Result<(String, Vec<ChessMove>), String> {
        if !self.pgn_text.trim().is_empty() {
            let game = parse_pgn(&self.pgn_text)?;
            Ok((game.start_fen, game.moves))
        } else {
            Ok((self.game_start_fen.clone(), self.played_moves.clone()))
        }
    }

    fn stop_analysis(&mut self, status: impl Into<String>) {
        self.analysis_engine = None;
        self.analysis_rx = None;
        self.analysis_jobs.clear();
        self.analysis_current_job = None;
        self.analysis_last_score_cp = None;
        self.analysis_running = false;
        self.analysis_status = status.into();
    }

    fn send_to_analysis_engine(&mut self, command: &str) {
        let result = if let Some(engine) = &mut self.analysis_engine {
            engine.send(command).map_err(|error| error.to_string())
        } else {
            Ok(())
        };
        if let Err(error) = result {
            self.analysis_status = format!("Analysis UCI write error: {error}");
            self.stop_analysis("Analysis stopped after UCI write error");
        }
    }

    fn request_next_analysis_job(&mut self) {
        if !self.analysis_running || self.analysis_current_job.is_some() {
            return;
        }
        let Some(job) = self.analysis_jobs.pop_front() else {
            self.analysis_running = false;
            self.analysis_status = self
                .analysis
                .as_ref()
                .map(|analysis| format!("Analysis finished: {}", analysis.summary().verdict))
                .unwrap_or_else(|| "Analysis finished".to_string());
            return;
        };
        self.analysis_last_score_cp = None;
        self.send_to_analysis_engine(&format!("position fen {}", job.fen));
        self.send_to_analysis_engine(&format!("go depth {}", self.analysis_depth));
        let ply = job.item_index + 1;
        self.analysis_status = format!("Analysing ply {ply} at depth {}", self.analysis_depth);
        self.analysis_current_job = Some(job);
    }

    fn poll_analysis_engine(&mut self) {
        let mut lines = Vec::new();
        if let Some(rx) = &self.analysis_rx {
            while let Ok(line) = rx.try_recv() {
                lines.push(line);
            }
        }
        for line in lines {
            self.handle_analysis_line(line);
        }
    }

    fn handle_analysis_line(&mut self, line: String) {
        self.analysis_log.push(line.clone());
        if self.analysis_log.len() > 220 {
            let extra = self.analysis_log.len() - 220;
            self.analysis_log.drain(0..extra);
        }

        if line.starts_with("info ") {
            if let Some(score_cp) = parse_uci_score_cp(&line) {
                self.analysis_last_score_cp = Some(score_cp);
            }
            return;
        }
        if line == "uciok" || line == "readyok" || line.starts_with("id ") {
            return;
        }
        if !line.starts_with("bestmove ") {
            return;
        }

        let Some(job) = self.analysis_current_job.take() else {
            return;
        };
        let score_cp = self.analysis_last_score_cp.unwrap_or(0);
        if let Some(analysis) = self.analysis.as_mut() {
            analysis.set_score(&job, score_cp);
        }
        self.request_next_analysis_job();
    }

    fn copy_analysis_report(&mut self, ctx: &egui::Context) {
        let Some(analysis) = &self.analysis else {
            self.analysis_status = "No analysis report yet".to_string();
            return;
        };
        ctx.copy_text(analysis.report());
        self.analysis_status = "Analysis report copied to clipboard".to_string();
    }

    fn refresh_game_status(&mut self) {
        let side = color_name(self.position.side_to_move());
        self.game_status = if self.position.is_checkmate() {
            format!("Checkmate. {side} has no legal move")
        } else if self.position.is_stalemate() {
            format!("Stalemate. {side} has no legal move")
        } else if self.position.is_in_check(self.position.side_to_move()) {
            format!("{side} to move, in check")
        } else {
            format!("{side} to move")
        };
    }

    fn history_view_ply(&self) -> usize {
        self.history_view_ply.unwrap_or(self.played_moves.len()).min(self.played_moves.len())
    }

    fn is_history_view_live(&self) -> bool {
        self.history_view_ply().min(self.played_moves.len()) == self.played_moves.len()
    }

    fn history_view_label(&self) -> String {
        let ply = self.history_view_ply();
        let total = self.played_moves.len();
        if ply == total {
            format!("Live position, ply {ply}/{total}")
        } else {
            format!("History view, ply {ply}/{total}")
        }
    }

    fn position_at_ply(&self, ply: usize) -> Result<Position, String> {
        let mut position = Position::from_fen(&self.game_start_fen)?;
        for chess_move in self.played_moves.iter().take(ply.min(self.played_moves.len())) {
            position.make_legal_move(*chess_move)?;
        }
        Ok(position)
    }

    fn display_position(&self) -> Position {
        self.position_at_ply(self.history_view_ply()).unwrap_or_else(|_| self.position.clone())
    }

    fn navigate_history_to(&mut self, ply: usize) {
        let total = self.played_moves.len();
        let clamped = ply.min(total);
        self.history_view_ply = if clamped == total { None } else { Some(clamped) };
        self.clear_selection();
        self.promotion_request = None;
        self.engine_status = self.history_view_label();
    }

    fn history_to_start(&mut self) {
        self.navigate_history_to(0);
    }

    fn history_previous(&mut self) {
        let current = self.history_view_ply();
        self.navigate_history_to(current.saturating_sub(1));
    }

    fn history_next(&mut self) {
        let current = self.history_view_ply();
        self.navigate_history_to(current.saturating_add(1));
    }

    fn history_to_live(&mut self) {
        self.navigate_history_to(self.played_moves.len());
    }

    fn handle_history_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let previous = ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft));
        let next = ctx.input(|input| input.key_pressed(egui::Key::ArrowRight));
        let home = ctx.input(|input| input.key_pressed(egui::Key::Home));
        let end = ctx.input(|input| input.key_pressed(egui::Key::End));
        if previous {
            self.history_previous();
        }
        if next {
            self.history_next();
        }
        if home {
            self.history_to_start();
        }
        if end {
            self.history_to_live();
        }
    }

    fn analysis_score_for_ply_white(&self, ply: usize) -> Option<i32> {
        let analysis = self.analysis.as_ref()?;
        if ply == 0 {
            let item = analysis.items.first()?;
            return item
                .before_score_cp
                .map(|score| score_from_fen_side_to_move_to_white(&item.before_fen, score));
        }
        let item = analysis.items.get(ply.saturating_sub(1))?;
        item.after_score_cp
            .map(|score| score_from_fen_side_to_move_to_white(&item.after_fen, score))
    }

    fn display_eval_cp_white(&self, display_position: &Position) -> i32 {
        let ply = self.history_view_ply();
        if let Some(score) = self.analysis_score_for_ply_white(ply) {
            return score;
        }
        if self.is_history_view_live() {
            if let Some(score) = self.last_engine_score_cp {
                return score_from_side_to_move_to_white(display_position.side_to_move(), score);
            }
        }
        score_from_side_to_move_to_white(display_position.side_to_move(), evaluate_for_side_to_move(display_position))
    }


    fn san_move_rows(&self) -> Vec<String> {
        let mut position = match Position::from_fen(&self.game_start_fen) {
            Ok(position) => position,
            Err(_) => {
                return self
                    .played_moves
                    .chunks(2)
                    .enumerate()
                    .map(|(index, pair)| {
                        let white_move = pair.first().map(|chess_move| chess_move.to_uci()).unwrap_or_default();
                        let black_move = pair.get(1).map(|chess_move| chess_move.to_uci()).unwrap_or_default();
                        format!("{}. {:<8} {}", index + 1, white_move, black_move)
                    })
                    .collect();
            }
        };

        let mut move_number = start_fullmove_number(&self.game_start_fen);
        let mut current_row = String::new();
        let mut rows = Vec::new();

        for chess_move in &self.played_moves {
            let side = position.side_to_move();
            let san = match move_to_san(&position, *chess_move) {
                Ok(san) => san,
                Err(_) => chess_move.to_uci(),
            };

            if side == Color::White {
                if !current_row.is_empty() {
                    rows.push(current_row);
                }
                current_row = format!("{move_number}. {san}");
            } else {
                if current_row.is_empty() {
                    current_row = format!("{move_number}... {san}");
                } else {
                    current_row.push_str(&format!(" {san}"));
                }
                rows.push(current_row);
                current_row = String::new();
                move_number += 1;
            }

            if position.make_legal_move(*chess_move).is_err() {
                break;
            }
        }

        if !current_row.is_empty() {
            rows.push(current_row);
        }
        rows
    }


    fn legal_move_rows(&self) -> Vec<String> {
        let position = self.display_position();
        let mut rows = Vec::new();
        let mut moves = position.legal_moves();
        moves.sort_by_key(|chess_move| chess_move.to_uci());
        for chess_move in moves {
            let san = move_to_san(&position, chess_move).unwrap_or_else(|_| chess_move.to_uci());
            rows.push(format!("{:<8} {}", san, chess_move.to_uci()));
        }
        rows
    }

    fn show_history_navigation(&mut self, ui: &mut egui::Ui) {
        ui.heading("Game navigation");
        ui.label(self.history_view_label());
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.history_view_ply() > 0, egui::Button::new("|<"))
                .clicked()
            {
                self.history_to_start();
            }
            if ui
                .add_enabled(self.history_view_ply() > 0, egui::Button::new("<"))
                .clicked()
            {
                self.history_previous();
            }
            if ui
                .add_enabled(self.history_view_ply() < self.played_moves.len(), egui::Button::new(">"))
                .clicked()
            {
                self.history_next();
            }
            if ui
                .add_enabled(self.history_view_ply() < self.played_moves.len(), egui::Button::new(">|"))
                .clicked()
            {
                self.history_to_live();
            }
        });
        ui.label("Keyboard: Left/Right, Home/End when no text field is focused.");
        if !self.is_history_view_live() {
            ui.label("Board is read-only while browsing history.");
        }
    }

    fn show_promotion_dialog(&mut self, ctx: &egui::Context) {
        let Some(request) = self.promotion_request.clone() else {
            return;
        };

        let mut chosen = None;
        let mut cancel = false;
        egui::Window::new("Promotion")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!("{} -> {}", square_name(request.from), square_name(request.to)));
                ui.horizontal(|ui| {
                    for (kind, label) in [
                        (PieceKind::Queen, "Queen"),
                        (PieceKind::Rook, "Rook"),
                        (PieceKind::Bishop, "Bishop"),
                        (PieceKind::Knight, "Knight"),
                    ] {
                        if request.options.iter().any(|chess_move| chess_move.promotion == Some(kind))
                            && ui.button(label).clicked()
                        {
                            chosen = Some(kind);
                        }
                    }
                });
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });

        if let Some(kind) = chosen {
            self.apply_promotion_choice(kind);
        } else if cancel {
            self.promotion_request = None;
            self.clear_selection();
        }
    }

    fn show_top_panel(&mut self, ui: &mut egui::Ui) {
        egui::TopBottomPanel::top("menu_panel").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New game").clicked() {
                        self.new_game();
                    }
                    if ui.button("Export PGN to text").clicked() {
                        self.export_pgn_to_text();
                    }
                    if ui.button("Copy PGN").clicked() {
                        self.copy_pgn_to_clipboard(ui.ctx());
                    }
                    if ui.button("Load PGN from text").clicked() {
                        self.load_pgn_from_text();
                    }
                    if ui.button("Open PGN path").clicked() {
                        self.open_pgn_from_file();
                    }
                    if ui.button("Save PGN path").clicked() {
                        self.save_pgn_to_file();
                    }
                });

                ui.menu_button("Game", |ui| {
                    if ui
                        .add_enabled(!self.pending_engine && !self.match_running, egui::Button::new("Engine move"))
                        .clicked()
                    {
                        self.request_engine_move();
                    }
                    if ui
                        .add_enabled(!self.pending_engine && !self.match_running && !self.played_moves.is_empty(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.undo_move();
                    }
                    if ui
                        .add_enabled(!self.pending_engine && !self.match_running && !self.redo_moves.is_empty(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.redo_move();
                    }
                    if ui.button("Flip board").clicked() {
                        self.flipped = !self.flipped;
                    }
                });

                ui.menu_button("Engine", |ui| {
                    if ui.button("Restart UCI child").clicked() {
                        self.stop_engine("Restarting UCI child");
                        if let Err(error) = self.ensure_engine() {
                            self.engine_status = error;
                        }
                    }
                    if ui.button("Stop engine").clicked() {
                        self.stop_engine("UCI child stopped");
                    }
                    ui.separator();
                    ui.label("Backend and planned CPU/RAM placeholders are configured in the right panel.");
                });

                ui.menu_button("Match", |ui| {
                    if ui
                        .add_enabled(!self.match_running && !self.pending_engine, egui::Button::new("Start engine match"))
                        .clicked()
                    {
                        self.start_engine_match();
                    }
                    if ui
                        .add_enabled(self.match_running, egui::Button::new("Stop engine match"))
                        .clicked()
                    {
                        self.stop_engine_match("Engine match stopped");
                    }
                    if ui.button("Copy match PGN").clicked() {
                        if self.match_pgn_text.trim().is_empty() {
                            self.update_match_pgn_text();
                        }
                        ui.ctx().copy_text(self.match_pgn_text.clone());
                        self.match_status = "Match PGN copied to clipboard".to_string();
                    }
                });

                ui.menu_button("Analysis", |ui| {
                    if ui
                        .add_enabled(!self.analysis_running, egui::Button::new("Start PGN/game analysis"))
                        .clicked()
                    {
                        self.start_game_analysis();
                    }
                    if ui
                        .add_enabled(self.analysis_running, egui::Button::new("Stop analysis"))
                        .clicked()
                    {
                        self.stop_analysis("Analysis stopped");
                    }
                    if ui.button("Copy analysis report").clicked() {
                        self.copy_analysis_report(ui.ctx());
                    }
                });

                ui.separator();
                ui.label(&self.game_status);
                ui.separator();
                ui.label(&self.engine_status);
            });
        });
    }

    fn show_left_panel(&mut self, ui: &mut egui::Ui) {
        let previous_player_color = self.player_color;
        let previous_auto_engine = self.auto_engine;

        egui::SidePanel::left("left_panel")
            .resizable(true)
            .default_width(250.0)
            .show_inside(ui, |ui| {
                ui.heading("Board controls");
                ui.label(&self.game_status);
                ui.separator();
                self.show_history_navigation(ui);
                ui.separator();

                ui.horizontal_wrapped(|ui| {
                    if ui.button("New").clicked() {
                        self.new_game();
                    }
                    if ui
                        .add_enabled(!self.pending_engine && !self.match_running, egui::Button::new("Engine"))
                        .clicked()
                    {
                        self.request_engine_move();
                    }
                    if ui
                        .add_enabled(!self.pending_engine && !self.match_running && !self.played_moves.is_empty(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.undo_move();
                    }
                    if ui
                        .add_enabled(!self.pending_engine && !self.match_running && !self.redo_moves.is_empty(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.redo_move();
                    }
                });

                ui.add(egui::Slider::new(&mut self.search_depth, 1..=8).text("Search depth"));
                ui.checkbox(&mut self.auto_engine, "Auto engine reply");
                ui.checkbox(&mut self.flipped, "Flip board");
                egui::ComboBox::from_id_salt("left_player_color")
                    .selected_text(color_name(self.player_color))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.player_color, Color::White, "White");
                        ui.selectable_value(&mut self.player_color, Color::Black, "Black");
                    });

                ui.separator();
                ui.heading("Position");
                ui.add(
                    egui::TextEdit::multiline(&mut self.fen_input)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(3),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Load FEN").clicked() {
                        self.load_fen();
                    }
                    if ui.button("Copy FEN").clicked() {
                        self.fen_input = self.position.to_fen();
                        ui.ctx().copy_text(self.fen_input.clone());
                    }
                });

                ui.separator();
                ui.heading("Legal moves");
                egui::ScrollArea::vertical()
                    .id_salt("left_legal_moves_scroll")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for row in self.legal_move_rows() {
                            ui.monospace(row);
                        }
                    });
            });

        let auto_was_enabled = !previous_auto_engine && self.auto_engine;
        let player_color_changed = previous_player_color != self.player_color;
        if (auto_was_enabled || player_color_changed) && self.should_auto_engine_move() {
            self.request_engine_move();
        }
    }

    fn show_side_panel(&mut self, ui: &mut egui::Ui) {
        let previous_engine_backend = self.engine_backend;

        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(390.0)
            .show_inside(ui, |ui| {
                ui.heading("Workspace");
                ui.label(&self.engine_status);
                ui.separator();

                ui.collapsing("PGN", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Export").clicked() {
                            self.export_pgn_to_text();
                        }
                        if ui.button("Copy").clicked() {
                            self.copy_pgn_to_clipboard(ui.ctx());
                        }
                        if ui.button("Load text").clicked() {
                            self.load_pgn_from_text();
                        }
                        if ui.button("Clear").clicked() {
                            self.pgn_text.clear();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("File");
                        ui.text_edit_singleline(&mut self.pgn_path);
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Open path").clicked() {
                            self.open_pgn_from_file();
                        }
                        if ui.button("Save path").clicked() {
                            self.save_pgn_to_file();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("pgn_text_scroll")
                        .max_height(170.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.pgn_text)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_rows(8),
                            );
                        });
                });

                ui.collapsing("Engine backend", |ui| {
                    egui::ComboBox::from_id_salt("engine_backend")
                        .selected_text(self.engine_backend.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.engine_backend, EngineBackend::RChess, EngineBackend::RChess.label());
                            ui.selectable_value(&mut self.engine_backend, EngineBackend::Stockfish10, EngineBackend::Stockfish10.label());
                            ui.selectable_value(&mut self.engine_backend, EngineBackend::CustomUci, EngineBackend::CustomUci.label());
                        });

                    match self.engine_backend {
                        EngineBackend::RChess => {
                            ui.label("Uses this GUI binary as a separate UCI child process with --engine-mode.");
                        }
                        EngineBackend::Stockfish10 => {
                            ui.label("Build the vendored Stockfish 10 source, then point this field to the resulting executable.");
                            ui.horizontal(|ui| {
                                ui.label("Stockfish 10");
                                ui.text_edit_singleline(&mut self.stockfish10_path);
                            });
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Detect").clicked() {
                                    match detect_stockfish10_path() {
                                        Some(path) => {
                                            self.stockfish10_path = path.clone();
                                            self.stockfish10_status = format!("Detected {path}");
                                        }
                                        None => {
                                            self.stockfish10_status = "No Stockfish 10 binary found near project or executable".to_string();
                                        }
                                    }
                                }
                                if ui.button("Use third_party path").clicked() {
                                    self.stockfish10_path = default_stockfish10_build_path();
                                    self.stockfish10_status = "Set to the expected build output path".to_string();
                                }
                            });
                            ui.label(&self.stockfish10_status);
                        }
                        EngineBackend::CustomUci => {
                            ui.label("Path to any UCI-compatible engine executable.");
                            ui.text_edit_singleline(&mut self.engine_path);
                        }
                    }

                    self.show_engine_resource_settings(ui);

                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Restart UCI child").clicked() {
                            self.stop_engine("Restarting UCI child");
                            if let Err(error) = self.ensure_engine() {
                                self.engine_status = error;
                            }
                        }
                        if ui.button("Stop engine").clicked() {
                            self.stop_engine("UCI child stopped");
                        }
                    });
                });

                ui.collapsing("Game analysis", |ui| {
                    self.show_analysis_panel(ui);
                });

                ui.collapsing("Engine vs engine", |ui| {
                    self.show_engine_match_panel(ui);
                });

                ui.collapsing("Move history", |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("moves_scroll")
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for row in self.san_move_rows() {
                                ui.monospace(row);
                            }
                        });
                });

                ui.collapsing("Engine info and logs", |ui| {
                    ui.heading("Engine info");
                    if self.last_engine_info.is_empty() {
                        ui.label("No search info yet");
                    } else {
                        ui.monospace(&self.last_engine_info);
                    }
                    ui.horizontal(|ui| {
                        ui.heading("UCI log");
                        if ui.button("Clear").clicked() {
                            self.engine_log.clear();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("uci_log_scroll")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for line in &self.engine_log {
                                ui.monospace(line);
                            }
                        });
                });
            });

        if previous_engine_backend != self.engine_backend {
            self.stop_engine("Engine backend changed");
        }
    }


    fn show_engine_resource_settings(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Resource settings, planned");
        ui.label("These controls are intentionally GUI-only for now. The current rchess search stays single-threaded and does not allocate a hash table yet.");
        ui.add(egui::Slider::new(&mut self.planned_threads, 1..=32).text("CPU threads target"));
        ui.horizontal(|ui| {
            ui.label("Hash target MB");
            ui.add(egui::DragValue::new(&mut self.planned_hash_mb).range(16..=4096).speed(16.0));
        });
        if ui.button("Store planned resource profile").clicked() {
            self.resource_settings_status = format!(
                "Stored future profile only: {} thread(s), {} MB hash target. Not applied to search yet.",
                self.planned_threads, self.planned_hash_mb
            );
        }
        ui.label(&self.resource_settings_status);
    }

    fn show_analysis_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(&self.analysis_status);
        ui.add(egui::Slider::new(&mut self.analysis_depth, 1..=8).text("Analysis depth"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.analysis_running, egui::Button::new("Start analysis"))
                .clicked()
            {
                self.start_game_analysis();
            }
            if ui
                .add_enabled(self.analysis_running, egui::Button::new("Stop"))
                .clicked()
            {
                self.stop_analysis("Analysis stopped");
            }
            if ui.button("Copy report").clicked() {
                self.copy_analysis_report(ui.ctx());
            }
        });

        if let Some(analysis) = &self.analysis {
            let done = analysis.completed_jobs();
            let total = analysis.total_jobs();
            let progress = if total == 0 { 0.0 } else { done as f32 / total as f32 };
            ui.add(egui::ProgressBar::new(progress).text(format!("{done}/{total} evals")));
            let summary = analysis.summary();
            ui.label(format!("White accuracy: {}", format_accuracy(summary.white_accuracy)));
            ui.label(format!("Black accuracy: {}", format_accuracy(summary.black_accuracy)));
            ui.label(summary.verdict);
            ui.separator();
            let current_view_ply = self.history_view_ply();
            let mut requested_ply = None;
            egui::ScrollArea::vertical()
                .id_salt("analysis_rows_scroll")
                .max_height(220.0)
                .show(ui, |ui| {
                    ui.monospace("ply side move eval-before eval-after loss acc");
                    for item in &analysis.items {
                        let side = color_name(item.side);
                        let before = format_cp(item.before_score_cp);
                        let after = format_cp(item.after_score_cp.map(|value| -value));
                        let loss = item.loss_cp.map(|value| value.to_string()).unwrap_or_else(|| "-".to_string());
                        let accuracy = format_accuracy(item.accuracy);
                        let row = format!(
                            "{:<3} {:<5} {:<8} {:<10} {:<10} {:<5} {}",
                            item.ply, side, item.san, before, after, loss, accuracy
                        );
                        if ui
                            .selectable_label(current_view_ply == item.ply, egui::RichText::new(row).monospace())
                            .clicked()
                        {
                            requested_ply = Some(item.ply);
                        }
                    }
                });
            if let Some(ply) = requested_ply {
                self.navigate_history_to(ply);
            }
        } else {
            ui.label("Paste/load a PGN or play a game, then start analysis.");
        }

        ui.collapsing("Analysis UCI log", |ui| {
            if ui.button("Clear analysis log").clicked() {
                self.analysis_log.clear();
            }
            egui::ScrollArea::vertical()
                .id_salt("analysis_log_scroll")
                .max_height(160.0)
                .show(ui, |ui| {
                    for line in &self.analysis_log {
                        ui.monospace(line);
                    }
                });
        });
    }

    fn show_engine_match_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Engine vs engine");
        ui.label(&self.match_status);
        ui.label("Empty path uses this GUI binary as rchess --engine-mode.");
        ui.horizontal(|ui| {
            ui.label("White");
            ui.text_edit_singleline(&mut self.match_white_path);
        });
        ui.horizontal(|ui| {
            ui.label("Black");
            ui.text_edit_singleline(&mut self.match_black_path);
        });
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.match_depth, 1..=8).text("Depth"));
            ui.add(egui::DragValue::new(&mut self.match_movetime_ms).speed(50.0).prefix("ms "));
        });
        ui.horizontal(|ui| {
            ui.label("Max plies");
            ui.add(egui::DragValue::new(&mut self.match_max_plies).range(2..=600).speed(2.0));
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.match_running && !self.pending_engine, egui::Button::new("Start match"))
                .clicked()
            {
                self.start_engine_match();
            }
            if ui
                .add_enabled(self.match_running, egui::Button::new("Stop match"))
                .clicked()
            {
                self.stop_engine_match("Engine match stopped");
            }
            if ui.button("Copy match PGN").clicked() {
                if self.match_pgn_text.trim().is_empty() {
                    self.update_match_pgn_text();
                }
                ui.ctx().copy_text(self.match_pgn_text.clone());
                self.match_status = "Match PGN copied to clipboard".to_string();
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("match_log_scroll")
            .max_height(100.0)
            .show(ui, |ui| {
                for line in &self.match_log {
                    ui.monospace(line);
                }
            });
    }

    fn show_board(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                let display_position = self.display_position();
                let eval_cp = self.display_eval_cp_white(&display_position);
                let max_size = (ui.available_width() - 46.0).min(ui.available_height() - 54.0);
                let board_size = max_size.clamp(320.0, 640.0);

                ui.horizontal_centered(|ui| {
                    self.paint_evaluation_bar(ui, board_size, eval_cp);

                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(board_size, board_size),
                        egui::Sense::click_and_drag(),
                    );

                    if self.is_history_view_live() && response.drag_started() {
                        if let Some(pointer_pos) = response.interact_pointer_pos() {
                            if let Some(square) = pointer_to_square(rect, pointer_pos, self.flipped) {
                                if self.select_piece(square) {
                                    self.dragging_from = Some(square);
                                    self.drag_pointer = Some(pointer_pos);
                                }
                            }
                        }
                    }

                    if self.dragging_from.is_some() {
                        if let Some(pointer_pos) = response.interact_pointer_pos() {
                            self.drag_pointer = Some(pointer_pos);
                        }
                    }

                    self.paint_board(ui, rect, &display_position);

                    if response.drag_stopped() {
                        if self.dragging_from.is_some() {
                            if let Some(pointer_pos) = self.drag_pointer.or_else(|| response.interact_pointer_pos()) {
                                if let Some(square) = pointer_to_square(rect, pointer_pos, self.flipped) {
                                    self.try_apply_selected_to(square);
                                } else {
                                    self.clear_selection();
                                }
                            }
                        }
                        self.dragging_from = None;
                        self.drag_pointer = None;
                    } else if self.is_history_view_live() && response.clicked() {
                        if let Some(pointer_pos) = response.interact_pointer_pos() {
                            if let Some(square) = pointer_to_square(rect, pointer_pos, self.flipped) {
                                self.select_square(square);
                            }
                        }
                    }
                });

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(format!("{} | eval {}", self.history_view_label(), format_cp_value(eval_cp)));
                    if !self.is_history_view_live() && ui.button("Return live").clicked() {
                        self.history_to_live();
                    }
                });
                ui.monospace(display_position.to_fen());
            });
        });
    }

    fn paint_evaluation_bar(&self, ui: &mut egui::Ui, board_size: f32, score_cp: i32) {
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, board_size), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(20, 20, 20));

            let clamped = score_cp.clamp(-1000, 1000) as f32;
            let white_ratio = (0.5 + clamped / 2000.0).clamp(0.03, 0.97);
            let white_height = rect.height() * white_ratio;
            let white_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - white_height),
                rect.right_bottom(),
            );
            painter.rect_filled(white_rect, 3.0, egui::Color32::from_rgb(235, 235, 225));
            painter.rect_stroke(
                rect,
                3.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 90, 90)),
                egui::StrokeKind::Outside,
            );
            ui.add_space(4.0);
            ui.monospace(format_cp_value(score_cp));
        });
    }

    fn paint_board(&self, ui: &mut egui::Ui, rect: egui::Rect, display_position: &Position) {
        let painter = ui.painter_at(rect);
        let square_size = rect.width() / 8.0;
        let selected = if self.is_history_view_live() { self.selected } else { None };
        let legal_targets: Vec<u8> = if self.is_history_view_live() {
            self.selected_moves.iter().map(|chess_move| chess_move.to).collect()
        } else {
            Vec::new()
        };
        let check_square = self.checked_king_square(display_position);
        let drag_target = self
            .drag_pointer
            .and_then(|pointer| pointer_to_square(rect, pointer, self.flipped));

        for row in 0..8 {
            for col in 0..8 {
                let square = view_square(row, col, self.flipped);
                let min = egui::pos2(
                    rect.left() + col as f32 * square_size,
                    rect.top() + row as f32 * square_size,
                );
                let square_rect = egui::Rect::from_min_size(min, egui::vec2(square_size, square_size));

                let is_light = (row + col) % 2 == 0;
                let mut fill = if is_light {
                    egui::Color32::from_rgb(235, 220, 190)
                } else {
                    egui::Color32::from_rgb(125, 88, 62)
                };

                if check_square == Some(square) {
                    fill = egui::Color32::from_rgb(175, 80, 70);
                } else if selected == Some(square) {
                    fill = egui::Color32::from_rgb(190, 170, 80);
                } else if self.dragging_from.is_some() && drag_target == Some(square) {
                    fill = egui::Color32::from_rgb(165, 150, 95);
                }

                painter.rect_filled(square_rect, 0.0, fill);

                if legal_targets.contains(&square) {
                    let center = square_rect.center();
                    if display_position.piece_at(square).is_some() {
                        painter.circle_stroke(
                            center,
                            square_size * 0.36,
                            egui::Stroke::new(4.0, egui::Color32::from_rgb(80, 120, 70)),
                        );
                    } else {
                        painter.circle_filled(
                            center,
                            square_size * 0.13,
                            egui::Color32::from_rgb(80, 120, 70),
                        );
                    }
                }

                if self.dragging_from != Some(square) {
                    if let Some(piece) = display_position.piece_at(square) {
                        self.paint_piece(&painter, piece, square_rect.center(), square_size);
                    }
                }

                self.paint_square_coordinates(&painter, square_rect, square, row, col, square_size);
            }
        }

        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 40)),
            egui::StrokeKind::Outside,
        );

        if let (Some(from), Some(pointer)) = (self.dragging_from, self.drag_pointer) {
            if let Some(piece) = display_position.piece_at(from) {
                painter.circle_filled(
                    pointer,
                    square_size * 0.42,
                    egui::Color32::from_rgba_premultiplied(240, 230, 205, 40),
                );
                self.paint_piece(&painter, piece, pointer, square_size);
            }
        }
    }

    fn paint_piece(
        &self,
        painter: &egui::Painter,
        piece: rchess::chess::Piece,
        center: egui::Pos2,
        square_size: f32,
    ) {
        let glyph = piece.unicode().to_string();
        let font = egui::FontId::proportional(square_size * 0.66);
        match piece.color {
            Color::White => {
                painter.text(
                    center + egui::vec2(1.4, 1.4),
                    egui::Align2::CENTER_CENTER,
                    &glyph,
                    font.clone(),
                    egui::Color32::from_rgb(25, 25, 25),
                );
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    glyph,
                    font,
                    egui::Color32::from_rgb(245, 245, 235),
                );
            }
            Color::Black => {
                painter.text(
                    center + egui::vec2(1.2, 1.2),
                    egui::Align2::CENTER_CENTER,
                    &glyph,
                    font.clone(),
                    egui::Color32::from_rgb(230, 220, 200),
                );
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    glyph,
                    font,
                    egui::Color32::from_rgb(15, 15, 15),
                );
            }
        }
    }

    fn paint_square_coordinates(
        &self,
        painter: &egui::Painter,
        square_rect: egui::Rect,
        square: u8,
        row: usize,
        col: usize,
        square_size: f32,
    ) {
        let file = (b'a' + square % 8) as char;
        let rank = (b'1' + square / 8) as char;
        let is_light = (row + col) % 2 == 0;
        let text_color = if is_light {
            egui::Color32::from_rgb(100, 80, 60)
        } else {
            egui::Color32::from_rgb(220, 205, 180)
        };
        let font = egui::FontId::proportional((square_size * 0.15).max(9.0));

        if col == 0 {
            painter.text(
                square_rect.left_top() + egui::vec2(4.0, 3.0),
                egui::Align2::LEFT_TOP,
                rank,
                font.clone(),
                text_color,
            );
        }
        if row == 7 {
            painter.text(
                square_rect.right_bottom() - egui::vec2(4.0, 3.0),
                egui::Align2::RIGHT_BOTTOM,
                file,
                font,
                text_color,
            );
        }
    }

    fn checked_king_square(&self, position: &Position) -> Option<u8> {
        let color = position.side_to_move();
        if !position.is_in_check(color) {
            return None;
        }

        for square in 0..64 {
            if let Some(piece) = position.piece_at(square) {
                if piece.color == color && piece.kind == PieceKind::King {
                    return Some(square);
                }
            }
        }
        None
    }

}

impl eframe::App for RChessGui {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_engine();
        self.poll_match_engines();
        self.poll_analysis_engine();
        self.handle_history_keyboard(ui.ctx());
        self.show_top_panel(ui);
        self.show_left_panel(ui);
        self.show_side_panel(ui);
        self.show_board(ui);
        self.show_promotion_dialog(ui.ctx());

        if self.pending_engine || self.match_running || self.analysis_running {
            ui.ctx().request_repaint_after(Duration::from_millis(80));
        }
    }
}

struct UciCommand {
    label: String,
    program: PathBuf,
    args: Vec<String>,
}

struct UciEngine {
    child: Child,
    stdin: ChildStdin,
}

impl UciEngine {
    fn spawn(spec: UciCommand) -> Result<(Self, Receiver<String>), String> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot start UCI child process: {error}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "cannot open UCI stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "cannot open UCI stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "cannot open UCI stderr".to_string())?;

        let (tx, rx) = mpsc::channel::<String>();
        let stdout_tx = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let _ = stdout_tx.send(line);
            }
        });
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(format!("stderr: {line}"));
            }
        });

        Ok((Self { child, stdin }, rx))
    }

    fn send(&mut self, command: &str) -> std::io::Result<()> {
        writeln!(self.stdin, "{command}")?;
        self.stdin.flush()
    }
}

impl Drop for UciEngine {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "quit");
        let _ = self.stdin.flush();
        let _ = self.child.kill();
    }
}


fn parse_uci_score_cp(line: &str) -> Option<i32> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let score_index = parts.iter().position(|part| *part == "score")?;
    let score_kind = *parts.get(score_index + 1)?;
    let score_value = *parts.get(score_index + 2)?;
    match score_kind {
        "cp" => score_value.parse::<i32>().ok(),
        "mate" => {
            let mate = score_value.parse::<i32>().ok()?;
            if mate >= 0 {
                Some(32_000 - mate.abs().min(100) * 100)
            } else {
                Some(-32_000 + mate.abs().min(100) * 100)
            }
        }
        _ => None,
    }
}

fn compact_uci_info_line(line: &str) -> String {
    let mut result = Vec::new();
    let parts: Vec<&str> = line.split_whitespace().collect();
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "depth" | "seldepth" | "nodes" | "nps" | "time" => {
                if let Some(value) = parts.get(index + 1) {
                    result.push(format!("{} {}", parts[index], value));
                    index += 2;
                } else {
                    index += 1;
                }
            }
            "score" => {
                if let (Some(kind), Some(value)) = (parts.get(index + 1), parts.get(index + 2)) {
                    result.push(format!("score {} {}", kind, value));
                    index += 3;
                } else {
                    index += 1;
                }
            }
            "pv" => {
                let pv = parts[index + 1..].join(" ");
                if !pv.is_empty() {
                    result.push(format!("pv {pv}"));
                }
                break;
            }
            _ => index += 1,
        }
    }
    if result.is_empty() {
        line.to_string()
    } else {
        result.join(" | ")
    }
}

fn normalize_path_input(value: &str) -> String {
    value.trim().trim_matches('\"').trim_matches('\'').to_string()
}

fn validate_executable_path(path: &str, label: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    if candidate.is_file() {
        Ok(())
    } else {
        Err(format!("{label} executable not found: {path}"))
    }
}

fn default_stockfish10_build_path() -> String {
    let mut path = PathBuf::from("third_party/stockfish-sf_10/src");
    path.push(stockfish_executable_name());
    path.to_string_lossy().to_string()
}

fn stockfish_executable_name() -> &'static str {
    if cfg!(windows) { "stockfish.exe" } else { "stockfish" }
}

fn detect_stockfish10_path() -> Option<String> {
    let executable = stockfish_executable_name();
    let mut candidates = vec![
        PathBuf::from("third_party/stockfish-sf_10/src").join(executable),
        PathBuf::from("engines/stockfish-sf_10/src").join(executable),
        PathBuf::from("stockfish-sf_10/src").join(executable),
    ];

    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            candidates.push(dir.join("stockfish-sf_10").join("src").join(executable));
            candidates.push(dir.join("engines").join("stockfish-sf_10").join("src").join(executable));
            candidates.push(dir.join("third_party").join("stockfish-sf_10").join("src").join(executable));
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        candidates.push(current_dir.join("third_party").join("stockfish-sf_10").join("src").join(executable));
    }

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| path.to_string_lossy().to_string())
}

fn pointer_to_square(rect: egui::Rect, pointer_pos: egui::Pos2, flipped: bool) -> Option<u8> {
    if !rect.contains(pointer_pos) {
        return None;
    }

    let square_size = rect.width() / 8.0;
    let col = ((pointer_pos.x - rect.left()) / square_size).floor() as usize;
    let row = ((pointer_pos.y - rect.top()) / square_size).floor() as usize;
    if row >= 8 || col >= 8 {
        return None;
    }

    Some(view_square(row, col, flipped))
}

fn view_square(row: usize, col: usize, flipped: bool) -> u8 {
    let (rank, file) = if flipped {
        (row as u8, 7 - col as u8)
    } else {
        (7 - row as u8, col as u8)
    };
    rank * 8 + file
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

fn score_from_side_to_move_to_white(side_to_move: Color, score_cp: i32) -> i32 {
    match side_to_move {
        Color::White => score_cp,
        Color::Black => -score_cp,
    }
}

fn score_from_fen_side_to_move_to_white(fen: &str, score_cp: i32) -> i32 {
    match fen.split_whitespace().nth(1) {
        Some("b") => -score_cp,
        _ => score_cp,
    }
}


fn start_fullmove_number(fen: &str) -> u32 {
    fen.split_whitespace()
        .nth(5)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
}
