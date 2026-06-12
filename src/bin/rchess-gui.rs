use std::collections::VecDeque;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use rchess::analysis::{format_accuracy, format_cp, format_cp_value, AnalysisJob, GameAnalysis};
use rchess::chess::{square_name, ChessMove, Color, DrawReason, PieceKind, Position, STARTPOS_FEN};
use rchess::experience::{append_game_to_experience_book, ExperienceConfig};
use rchess::matchplay::{uci_position_command_from_history, EngineMatchController, SearchLimit, UciEngineSlot};
use rchess::pgn::{export_pgn, move_to_san, parse_pgn, position_after_moves};
use rchess::search::evaluate_tactical_for_side_to_move;

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


#[derive(Clone, Copy, PartialEq, Eq)]
enum PiecePreset {
    StandardUnicode,
    FilledUnicode,
    Letters,
    Custom,
}

impl PiecePreset {
    fn label(self) -> &'static str {
        match self {
            PiecePreset::StandardUnicode => "Standard Unicode",
            PiecePreset::FilledUnicode => "Filled Unicode",
            PiecePreset::Letters => "Letter pieces",
            PiecePreset::Custom => "Custom glyph set",
        }
    }
}

#[derive(Clone)]
struct PieceGlyphSet {
    white_king: String,
    white_queen: String,
    white_rook: String,
    white_bishop: String,
    white_knight: String,
    white_pawn: String,
    black_king: String,
    black_queen: String,
    black_rook: String,
    black_bishop: String,
    black_knight: String,
    black_pawn: String,
}

impl PieceGlyphSet {
    fn standard_unicode() -> Self {
        Self::from_tokens(["♔", "♕", "♖", "♗", "♘", "♙", "♚", "♛", "♜", "♝", "♞", "♟"])
    }

    fn filled_unicode() -> Self {
        Self::from_tokens(["♚", "♛", "♜", "♝", "♞", "♟", "♚", "♛", "♜", "♝", "♞", "♟"])
    }

    fn letters() -> Self {
        Self::from_tokens(["K", "Q", "R", "B", "N", "P", "k", "q", "r", "b", "n", "p"])
    }

    fn from_tokens(tokens: [&str; 12]) -> Self {
        Self {
            white_king: tokens[0].to_string(),
            white_queen: tokens[1].to_string(),
            white_rook: tokens[2].to_string(),
            white_bishop: tokens[3].to_string(),
            white_knight: tokens[4].to_string(),
            white_pawn: tokens[5].to_string(),
            black_king: tokens[6].to_string(),
            black_queen: tokens[7].to_string(),
            black_rook: tokens[8].to_string(),
            black_bishop: tokens[9].to_string(),
            black_knight: tokens[10].to_string(),
            black_pawn: tokens[11].to_string(),
        }
    }

    fn parse(text: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = text
            .lines()
            .flat_map(|line| line.split('#').next().unwrap_or("").split_whitespace())
            .collect();
        if tokens.len() != 12 {
            return Err(format!(
                "custom piece preset needs 12 whitespace-separated glyphs: WK WQ WR WB WN WP BK BQ BR BB BN BP; got {}",
                tokens.len()
            ));
        }
        Ok(Self {
            white_king: tokens[0].to_string(),
            white_queen: tokens[1].to_string(),
            white_rook: tokens[2].to_string(),
            white_bishop: tokens[3].to_string(),
            white_knight: tokens[4].to_string(),
            white_pawn: tokens[5].to_string(),
            black_king: tokens[6].to_string(),
            black_queen: tokens[7].to_string(),
            black_rook: tokens[8].to_string(),
            black_bishop: tokens[9].to_string(),
            black_knight: tokens[10].to_string(),
            black_pawn: tokens[11].to_string(),
        })
    }

    fn to_preset_text(&self) -> String {
        format!(
            "{} {} {} {} {} {}\n{} {} {} {} {} {}\n",
            self.white_king,
            self.white_queen,
            self.white_rook,
            self.white_bishop,
            self.white_knight,
            self.white_pawn,
            self.black_king,
            self.black_queen,
            self.black_rook,
            self.black_bishop,
            self.black_knight,
            self.black_pawn
        )
    }

    fn glyph(&self, piece: rchess::chess::Piece) -> &str {
        match (piece.color, piece.kind) {
            (Color::White, PieceKind::King) => &self.white_king,
            (Color::White, PieceKind::Queen) => &self.white_queen,
            (Color::White, PieceKind::Rook) => &self.white_rook,
            (Color::White, PieceKind::Bishop) => &self.white_bishop,
            (Color::White, PieceKind::Knight) => &self.white_knight,
            (Color::White, PieceKind::Pawn) => &self.white_pawn,
            (Color::Black, PieceKind::King) => &self.black_king,
            (Color::Black, PieceKind::Queen) => &self.black_queen,
            (Color::Black, PieceKind::Rook) => &self.black_rook,
            (Color::Black, PieceKind::Bishop) => &self.black_bishop,
            (Color::Black, PieceKind::Knight) => &self.black_knight,
            (Color::Black, PieceKind::Pawn) => &self.black_pawn,
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
    move_animation: Option<MoveAnimation>,
    animate_moves: bool,
    move_animation_ms: u32,
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
    deterministic_multithread: bool,
    planned_threads: u16,
    search_granularity: u16,
    planned_hash_mb: u32,
    avoid_draws: bool,
    resource_settings_status: String,
    experience_book_enabled: bool,
    experience_book_path: String,
    experience_min_games: u32,
    experience_score_tolerance_cp: i32,
    experience_status: String,
    match_white_path: String,
    match_black_path: String,
    match_white_depth: u8,
    match_black_depth: u8,
    match_white_movetime_ms: u64,
    match_black_movetime_ms: u64,
    match_white_options: String,
    match_black_options: String,
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
    light_square_color: egui::Color32,
    dark_square_color: egui::Color32,
    selected_square_color: egui::Color32,
    drag_target_color: egui::Color32,
    check_square_color: egui::Color32,
    legal_move_color: egui::Color32,
    legal_capture_color: egui::Color32,
    coordinate_light_color: egui::Color32,
    coordinate_dark_color: egui::Color32,
    white_piece_color: egui::Color32,
    black_piece_color: egui::Color32,
    piece_shadow_color: egui::Color32,
    show_coordinates: bool,
    piece_scale: f32,
    board_style_status: String,
    piece_preset: PiecePreset,
    custom_piece_glyphs: PieceGlyphSet,
    custom_piece_preset_text: String,
    custom_piece_preset_path: String,
}

#[derive(Clone)]
struct PromotionRequest {
    from: u8,
    to: u8,
    options: Vec<ChessMove>,
}

#[derive(Clone)]
struct MoveAnimation {
    chess_move: ChessMove,
    piece: rchess::chess::Piece,
    start: Instant,
    duration: Duration,
}

impl MoveAnimation {
    fn progress(&self) -> f32 {
        let duration = self.duration.as_secs_f32().max(0.001);
        (self.start.elapsed().as_secs_f32() / duration).clamp(0.0, 1.0)
    }

    fn eased_progress(&self) -> f32 {
        let t = self.progress();
        t * t * (3.0 - 2.0 * t)
    }

    fn is_finished(&self) -> bool {
        self.progress() >= 1.0
    }
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
        let default_threads = default_parallel_threads();
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
            move_animation: None,
            animate_moves: true,
            move_animation_ms: 180,
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
            deterministic_multithread: default_threads > 1,
            planned_threads: default_threads,
            search_granularity: 1,
            planned_hash_mb: 64,
            avoid_draws: false,
            resource_settings_status: format!("Internal rchess defaults: deterministic_multithread={}, max_threads={}, granularity=1, Hash=64 MB", default_threads > 1, default_threads),
            experience_book_enabled: false,
            experience_book_path: ExperienceConfig::default().path,
            experience_min_games: ExperienceConfig::default().min_games,
            experience_score_tolerance_cp: ExperienceConfig::default().score_tolerance_cp,
            experience_status: "Experience book is disabled".to_string(),
            match_white_path: String::new(),
            match_black_path: String::new(),
            match_white_depth: 3,
            match_black_depth: 3,
            match_white_movetime_ms: 0,
            match_black_movetime_ms: 0,
            match_white_options: String::new(),
            match_black_options: String::new(),
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
            light_square_color: egui::Color32::from_rgb(235, 220, 190),
            dark_square_color: egui::Color32::from_rgb(125, 88, 62),
            selected_square_color: egui::Color32::from_rgb(190, 170, 80),
            drag_target_color: egui::Color32::from_rgb(165, 150, 95),
            check_square_color: egui::Color32::from_rgb(175, 80, 70),
            legal_move_color: egui::Color32::from_rgb(80, 120, 70),
            legal_capture_color: egui::Color32::from_rgb(80, 120, 70),
            coordinate_light_color: egui::Color32::from_rgb(100, 80, 60),
            coordinate_dark_color: egui::Color32::from_rgb(220, 205, 180),
            white_piece_color: egui::Color32::from_rgb(245, 245, 235),
            black_piece_color: egui::Color32::from_rgb(15, 15, 15),
            piece_shadow_color: egui::Color32::from_rgb(25, 25, 25),
            show_coordinates: true,
            piece_scale: 0.66,
            board_style_status: "Board appearance uses the built-in wood theme".to_string(),
            piece_preset: PiecePreset::StandardUnicode,
            custom_piece_glyphs: PieceGlyphSet::standard_unicode(),
            custom_piece_preset_text: PieceGlyphSet::standard_unicode().to_preset_text(),
            custom_piece_preset_path: String::new(),
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
        self.move_animation = None;
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
                self.move_animation = None;
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
                self.move_animation = None;
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
        } else if self.current_draw_reason().is_some() {
            "1/2-1/2".to_string()
        } else {
            "*".to_string()
        }
    }

    fn current_draw_reason(&self) -> Option<DrawReason> {
        Position::draw_reason_from_history(&self.game_start_fen, &self.played_moves)
            .ok()
            .flatten()
    }

    fn current_position_command(&self) -> String {
        uci_position_command_from_history(&self.game_start_fen, &self.played_moves)
    }

    fn draw_reason_for_ply(&self, ply: usize) -> Option<DrawReason> {
        let clamped = ply.min(self.played_moves.len());
        Position::draw_reason_from_history(&self.game_start_fen, &self.played_moves[..clamped])
            .ok()
            .flatten()
    }

    fn select_square(&mut self, square: u8) {
        if self.pending_engine
            || self.match_running
            || self.promotion_request.is_some()
            || !self.is_history_view_live()
            || self.current_result() != "*"
        {
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
        if self.pending_engine
            || self.match_running
            || self.promotion_request.is_some()
            || !self.is_history_view_live()
            || self.current_result() != "*"
        {
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

    fn start_move_animation(&mut self, chess_move: ChessMove) {
        if !self.animate_moves || self.move_animation_ms == 0 || !self.is_history_view_live() {
            self.move_animation = None;
            return;
        }
        let Some(piece) = self.position.piece_at(chess_move.to) else {
            self.move_animation = None;
            return;
        };
        self.move_animation = Some(MoveAnimation {
            chess_move,
            piece,
            start: Instant::now(),
            duration: Duration::from_millis(self.move_animation_ms.clamp(30, 1200) as u64),
        });
    }

    fn finish_move_animation_if_done(&mut self) {
        let finished = self
            .move_animation
            .as_ref()
            .map(|animation| animation.is_finished())
            .unwrap_or(false);
        if finished {
            self.move_animation = None;
        }
    }

    fn should_auto_engine_move(&self) -> bool {
        self.auto_engine
            && !self.pending_engine
            && !self.match_running
            && self.position.side_to_move() != self.player_color
            && self.current_result() == "*"
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
                self.start_move_animation(chess_move);
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
        self.move_animation = None;
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
                self.start_move_animation(chess_move);
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
                    is_internal_rchess: true,
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
                    is_internal_rchess: false,
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
                    is_internal_rchess: false,
                })
            }
        }
    }

    fn request_engine_move(&mut self) {
        if self.pending_engine || self.match_running {
            return;
        }
        if self.current_result() != "*" {
            self.refresh_game_status();
            return;
        }
        if let Err(error) = self.ensure_engine() {
            self.engine_status = error;
            return;
        }

        self.send_primary_engine_resource_options();
        self.pending_engine = true;
        self.engine_status = format!("Engine is thinking at depth {}", self.search_depth);
        self.send_to_engine(&self.current_position_command());
        self.send_to_engine(&format!("go depth {}", self.search_depth));
    }

    fn ensure_engine(&mut self) -> Result<(), String> {
        if self.engine.is_some() {
            return Ok(());
        }

        let command = self.current_engine_command()?;
        let label = command.label.clone();
        let is_internal_rchess = command.is_internal_rchess;
        let (mut engine, rx) = UciEngine::spawn(command)?;
        if is_internal_rchess {
            let _ = send_rchess_resource_options(
                &mut engine,
                self.deterministic_multithread,
                self.planned_threads,
                self.search_granularity,
                self.planned_hash_mb,
                self.avoid_draws,
            );
            let _ = send_rchess_experience_options(&mut engine, &self.experience_config());
        }
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

    fn send_primary_engine_resource_options(&mut self) {
        if self.engine_backend != EngineBackend::RChess {
            self.resource_settings_status = "Resource settings are only applied to the internal rchess backend for now".to_string();
            return;
        }
        let config = self.experience_config();
        let Some(engine) = &mut self.engine else {
            return;
        };
        let resource_result = send_rchess_resource_options(
            engine,
            self.deterministic_multithread,
            self.planned_threads,
            self.search_granularity,
            self.planned_hash_mb,
            self.avoid_draws,
        )
        .and_then(|_| send_rchess_experience_options(engine, &config));
        match resource_result {
            Ok(()) => {
                self.resource_settings_status = format!(
                    "Applied to internal rchess: deterministic_multithread={}, max_threads={}, granularity={}, Hash={} MB, AvoidDraws={}, experience_book={}",
                    self.deterministic_multithread, self.planned_threads, self.search_granularity, self.planned_hash_mb, self.avoid_draws, self.experience_book_enabled
                );
                self.experience_status = format!(
                    "Experience config applied: enabled={}, path={}, min_games={}, tolerance={} cp",
                    self.experience_book_enabled, self.experience_book_path, self.experience_min_games, self.experience_score_tolerance_cp
                );
            }
            Err(error) => {
                self.resource_settings_status = format!("Resource/experience option write error: {error}");
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
                    self.start_move_animation(chess_move);
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


    fn experience_config(&self) -> ExperienceConfig {
        ExperienceConfig {
            enabled: self.experience_book_enabled,
            path: self.experience_book_path.clone(),
            min_games: self.experience_min_games,
            score_tolerance_cp: self.experience_score_tolerance_cp,
        }
        .normalized()
    }

    fn append_current_match_to_experience_book(&mut self) {
        let Some(controller) = &self.match_controller else {
            self.experience_status = "No engine match is available to export".to_string();
            return;
        };
        if controller.played_moves.is_empty() {
            self.experience_status = "Current match has no moves".to_string();
            return;
        }
        let analysis = self.analysis.as_ref();
        match append_game_to_experience_book(
            &self.experience_book_path,
            &controller.start_fen,
            &controller.played_moves,
            &controller.result,
            &controller.white.name,
            &controller.black.name,
            analysis,
        ) {
            Ok(count) => {
                self.experience_status = format!(
                    "Appended {count} move samples to {}. Run normal analysis first if you want engine-depth scores instead of tactical fallback scores.",
                    self.experience_book_path
                );
            }
            Err(error) => {
                self.experience_status = format!("Experience export error: {error}");
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

        let white_limit = SearchLimit::depth_or_movetime(self.match_white_depth, self.match_white_movetime_ms);
        let black_limit = SearchLimit::depth_or_movetime(self.match_black_depth, self.match_black_movetime_ms);
        let white_name = white_command.label.clone();
        let black_name = black_command.label.clone();
        let white_is_internal_rchess = white_command.is_internal_rchess;
        let black_is_internal_rchess = black_command.is_internal_rchess;
        let white_slot = UciEngineSlot::new(white_name.clone(), white_command.program.to_string_lossy().to_string())
            .with_args(white_command.args.clone())
            .with_limit(white_limit);
        let black_slot = UciEngineSlot::new(black_name.clone(), black_command.program.to_string_lossy().to_string())
            .with_args(black_command.args.clone())
            .with_limit(black_limit);

        let start_fen = self.position.to_fen();
        let controller = match EngineMatchController::from_fen(&start_fen, white_slot, black_slot) {
            Ok(controller) => controller,
            Err(error) => {
                self.match_status = format!("Match start FEN error: {error}");
                return;
            }
        };

        let (mut white_engine, white_rx) = match UciEngine::spawn(white_command) {
            Ok(value) => value,
            Err(error) => {
                self.match_status = format!("White engine start error: {error}");
                return;
            }
        };
        let (mut black_engine, black_rx) = match UciEngine::spawn(black_command) {
            Ok(value) => value,
            Err(error) => {
                self.match_status = format!("Black engine start error: {error}");
                return;
            }
        };

        if let Err(error) = white_engine.send("uci") {
            self.match_status = format!("White match UCI startup error: {error}");
            return;
        }
        if let Err(error) = black_engine.send("uci") {
            self.match_status = format!("Black match UCI startup error: {error}");
            return;
        }

        if let Err(error) = send_match_engine_startup_options(
            &mut white_engine,
            white_is_internal_rchess,
            self.deterministic_multithread,
            self.planned_threads,
            self.search_granularity,
            self.planned_hash_mb,
            self.avoid_draws,
            &self.experience_config(),
            &self.match_white_options,
        ) {
            self.match_status = format!("White match option error: {error}");
            return;
        }
        if let Err(error) = send_match_engine_startup_options(
            &mut black_engine,
            black_is_internal_rchess,
            self.deterministic_multithread,
            self.planned_threads,
            self.search_granularity,
            self.planned_hash_mb,
            self.avoid_draws,
            &self.experience_config(),
            &self.match_black_options,
        ) {
            self.match_status = format!("Black match option error: {error}");
            return;
        }

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
        self.match_status = format!(
            "Match started: {white_name} ({}) vs {black_name} ({})",
            SearchLimit::depth_or_movetime(self.match_white_depth, self.match_white_movetime_ms).go_command(),
            SearchLimit::depth_or_movetime(self.match_black_depth, self.match_black_movetime_ms).go_command()
        );

        let _ = self.send_to_match_engine(Color::White, "isready");
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
                is_internal_rchess: true,
            })
        } else {
            validate_executable_path(&normalized, color_label)?;
            let program = PathBuf::from(&normalized);
            let label = program
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| format!("{color_label} {value}"))
                .unwrap_or_else(|| format!("{color_label} UCI"));
            Ok(UciCommand { label, program, args: Vec::new(), is_internal_rchess: false })
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
                let last_match_move = moves.last().copied();
                self.position = position;
                self.played_moves = moves;
                self.redo_moves.clear();
                self.history_view_ply = None;
                self.last_engine_score_cp = None;
                self.fen_input = self.position.to_fen();
                if let Some(chess_move) = last_match_move {
                    self.start_move_animation(chess_move);
                }
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
        let is_internal_rchess = command.is_internal_rchess;
        let (mut engine, rx) = match UciEngine::spawn(command) {
            Ok(value) => value,
            Err(error) => {
                self.analysis_status = format!("Analysis engine start error: {error}");
                return;
            }
        };

        if is_internal_rchess {
            let _ = send_rchess_resource_options(
                &mut engine,
                self.deterministic_multithread,
                self.planned_threads,
                self.search_granularity,
                self.planned_hash_mb,
                false,
            );
        }

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
        let score_cp = self
            .analysis_last_score_cp
            .unwrap_or_else(|| score_for_fen_side_to_move(&job.fen));
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
        } else if let Some(reason) = self.current_draw_reason() {
            format!("Draw by {}", reason.label())
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
        self.move_animation = None;
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
        if let Some(score) = terminal_score_white(display_position) {
            return score;
        }
        if self.draw_reason_for_ply(ply).is_some() {
            return 0;
        }

        if let Some(score) = self.analysis_score_for_ply_white(ply) {
            return score;
        }
        if self.is_history_view_live() {
            if let Some(score) = self.last_engine_score_cp {
                return score_from_side_to_move_to_white(display_position.side_to_move(), score);
            }
        }
        score_from_side_to_move_to_white(display_position.side_to_move(), evaluate_tactical_for_side_to_move(display_position))
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

            ui.menu_button("View", |ui| {
                if ui.button("Restore default board theme").clicked() {
                    self.restore_default_board_theme();
                }
                if ui.button("Use standard Unicode pieces").clicked() {
                    self.piece_preset = PiecePreset::StandardUnicode;
                    self.board_style_status = "Using standard Unicode piece preset".to_string();
                }
                if ui.button("Use filled Unicode pieces").clicked() {
                    self.piece_preset = PiecePreset::FilledUnicode;
                    self.board_style_status = "Using filled Unicode piece preset".to_string();
                }
                if ui.button("Use letter pieces").clicked() {
                    self.piece_preset = PiecePreset::Letters;
                    self.board_style_status = "Using letter piece preset".to_string();
                }
                ui.separator();
                ui.label("Detailed visual settings are in Workspace / Board appearance.");
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
                ui.label("Backend and deterministic resource settings are configured in the right panel.");
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
    }

    fn show_left_panel(&mut self, ui: &mut egui::Ui) {
        let previous_player_color = self.player_color;
        let previous_auto_engine = self.auto_engine;

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

        let auto_was_enabled = !previous_auto_engine && self.auto_engine;
        let player_color_changed = previous_player_color != self.player_color;
        if (auto_was_enabled || player_color_changed) && self.should_auto_engine_move() {
            self.request_engine_move();
        }
    }

    fn show_side_panel(&mut self, ui: &mut egui::Ui) {
        let previous_engine_backend = self.engine_backend;

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

                ui.collapsing("Board appearance", |ui| {
                    self.show_board_appearance_panel(ui);
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

        if previous_engine_backend != self.engine_backend {
            self.stop_engine("Engine backend changed");
        }
    }


    fn show_board_appearance_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(&self.board_style_status);
        ui.separator();
        ui.heading("Squares");
        color_row(ui, "Light squares", &mut self.light_square_color);
        color_row(ui, "Dark squares", &mut self.dark_square_color);
        color_row(ui, "Selected square", &mut self.selected_square_color);
        color_row(ui, "Drag target", &mut self.drag_target_color);
        color_row(ui, "Check square", &mut self.check_square_color);
        color_row(ui, "Legal quiet move", &mut self.legal_move_color);
        color_row(ui, "Legal capture", &mut self.legal_capture_color);

        ui.separator();
        ui.heading("Pieces");
        egui::ComboBox::from_id_salt("piece_preset")
            .selected_text(self.piece_preset.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.piece_preset, PiecePreset::StandardUnicode, PiecePreset::StandardUnicode.label());
                ui.selectable_value(&mut self.piece_preset, PiecePreset::FilledUnicode, PiecePreset::FilledUnicode.label());
                ui.selectable_value(&mut self.piece_preset, PiecePreset::Letters, PiecePreset::Letters.label());
                ui.selectable_value(&mut self.piece_preset, PiecePreset::Custom, PiecePreset::Custom.label());
            });
        ui.add(egui::Slider::new(&mut self.piece_scale, 0.45..=0.95).text("Piece scale"));
        ui.checkbox(&mut self.animate_moves, "Animate moves");
        ui.add(egui::Slider::new(&mut self.move_animation_ms, 30..=800).text("Move animation ms"));
        color_row(ui, "White pieces", &mut self.white_piece_color);
        color_row(ui, "Black pieces", &mut self.black_piece_color);
        color_row(ui, "Piece shadow", &mut self.piece_shadow_color);

        ui.separator();
        ui.heading("Coordinates");
        ui.checkbox(&mut self.show_coordinates, "Show coordinates");
        color_row(ui, "Coordinate text on light", &mut self.coordinate_light_color);
        color_row(ui, "Coordinate text on dark", &mut self.coordinate_dark_color);

        ui.separator();
        ui.heading("Custom piece preset");
        ui.label("Format: 12 whitespace-separated glyphs: WK WQ WR WB WN WP BK BQ BR BB BN BP.");
        ui.add(
            egui::TextEdit::multiline(&mut self.custom_piece_preset_text)
                .font(egui::TextStyle::Monospace)
                .desired_rows(3),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Load custom from text").clicked() {
                self.load_custom_piece_preset_from_text();
            }
            if ui.button("Export active custom text").clicked() {
                self.custom_piece_preset_text = self.custom_piece_glyphs.to_preset_text();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Preset file");
            ui.text_edit_singleline(&mut self.custom_piece_preset_path);
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open preset file").clicked() {
                self.open_custom_piece_preset_file();
            }
            if ui.button("Save preset file").clicked() {
                self.save_custom_piece_preset_file();
            }
            if ui.button("Restore defaults").clicked() {
                self.restore_default_board_theme();
            }
        });
    }

    fn restore_default_board_theme(&mut self) {
        self.light_square_color = egui::Color32::from_rgb(235, 220, 190);
        self.dark_square_color = egui::Color32::from_rgb(125, 88, 62);
        self.selected_square_color = egui::Color32::from_rgb(190, 170, 80);
        self.drag_target_color = egui::Color32::from_rgb(165, 150, 95);
        self.check_square_color = egui::Color32::from_rgb(175, 80, 70);
        self.legal_move_color = egui::Color32::from_rgb(80, 120, 70);
        self.legal_capture_color = egui::Color32::from_rgb(80, 120, 70);
        self.coordinate_light_color = egui::Color32::from_rgb(100, 80, 60);
        self.coordinate_dark_color = egui::Color32::from_rgb(220, 205, 180);
        self.white_piece_color = egui::Color32::from_rgb(245, 245, 235);
        self.black_piece_color = egui::Color32::from_rgb(15, 15, 15);
        self.piece_shadow_color = egui::Color32::from_rgb(25, 25, 25);
        self.show_coordinates = true;
        self.piece_scale = 0.66;
        self.piece_preset = PiecePreset::StandardUnicode;
        self.custom_piece_glyphs = PieceGlyphSet::standard_unicode();
        self.custom_piece_preset_text = self.custom_piece_glyphs.to_preset_text();
        self.board_style_status = "Board appearance restored to defaults".to_string();
    }

    fn load_custom_piece_preset_from_text(&mut self) {
        match PieceGlyphSet::parse(&self.custom_piece_preset_text) {
            Ok(glyphs) => {
                self.custom_piece_glyphs = glyphs;
                self.piece_preset = PiecePreset::Custom;
                self.board_style_status = "Custom piece preset loaded from text".to_string();
            }
            Err(error) => {
                self.board_style_status = error;
            }
        }
    }

    fn open_custom_piece_preset_file(&mut self) {
        let path = normalize_path_input(&self.custom_piece_preset_path);
        match fs::read_to_string(&path) {
            Ok(text) => {
                self.custom_piece_preset_text = text;
                self.load_custom_piece_preset_from_text();
                if self.piece_preset == PiecePreset::Custom {
                    self.board_style_status = format!("Custom piece preset loaded from {path}");
                }
            }
            Err(error) => {
                self.board_style_status = format!("cannot read custom piece preset: {error}");
            }
        }
    }

    fn save_custom_piece_preset_file(&mut self) {
        let path = normalize_path_input(&self.custom_piece_preset_path);
        if path.is_empty() {
            self.board_style_status = "custom piece preset path is empty".to_string();
            return;
        }
        self.custom_piece_preset_text = self.custom_piece_glyphs.to_preset_text();
        match fs::write(&path, &self.custom_piece_preset_text) {
            Ok(()) => {
                self.board_style_status = format!("Custom piece preset saved to {path}");
            }
            Err(error) => {
                self.board_style_status = format!("cannot save custom piece preset: {error}");
            }
        }
    }

    fn piece_glyph(&self, piece: rchess::chess::Piece) -> String {
        match self.piece_preset {
            PiecePreset::StandardUnicode => piece.unicode().to_string(),
            PiecePreset::FilledUnicode => PieceGlyphSet::filled_unicode().glyph(piece).to_string(),
            PiecePreset::Letters => PieceGlyphSet::letters().glyph(piece).to_string(),
            PiecePreset::Custom => self.custom_piece_glyphs.glyph(piece).to_string(),
        }
    }

    fn show_engine_resource_settings(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Search resources");
        ui.label("These settings are active for the internal rchess UCI backend. They are intentionally deterministic: root moves are split in a fixed order and the shared transposition table uses atomic replace-by-depth+age entries.");
        ui.checkbox(&mut self.deterministic_multithread, "deterministic_multithread");
        ui.add(egui::Slider::new(&mut self.planned_threads, 1..=32).text("max_threads"));
        ui.add(egui::Slider::new(&mut self.search_granularity, 1..=16).text("granularity"));
        ui.horizontal(|ui| {
            ui.label("Hash MB");
            ui.add(egui::DragValue::new(&mut self.planned_hash_mb).range(1..=4096).speed(16.0));
        });
        ui.checkbox(&mut self.avoid_draws, "Avoid draws for internal rchess");
        ui.separator();
        ui.heading("Experience book");
        ui.label("Deterministic mode: the book can only break ties between root moves inside the configured score tolerance. It does not change evaluation weights.");
        ui.checkbox(&mut self.experience_book_enabled, "Use experience book for internal rchess");
        ui.horizontal(|ui| {
            ui.label("Book file");
            ui.text_edit_singleline(&mut self.experience_book_path);
        });
        ui.horizontal(|ui| {
            ui.label("Min games");
            ui.add(egui::DragValue::new(&mut self.experience_min_games).range(1..=10_000).speed(1.0));
            ui.label("Tolerance cp");
            ui.add(egui::DragValue::new(&mut self.experience_score_tolerance_cp).range(0..=1000).speed(5.0));
        });
        if ui.button("Apply to running rchess child").clicked() {
            self.send_primary_engine_resource_options();
        }
        ui.label(&self.resource_settings_status);
        ui.label(&self.experience_status);
    }

    fn analysis_chart_points(&self) -> Vec<(usize, i32)> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let mut points = Vec::with_capacity(analysis.items.len() + 1);
        if let Some(first) = analysis.items.first() {
            if let Some(score) = first.before_score_cp {
                points.push((0, score_from_fen_side_to_move_to_white(&first.before_fen, score)));
            }
        }
        for item in &analysis.items {
            if let Some(score) = item.after_score_cp {
                points.push((item.ply, score_from_fen_side_to_move_to_white(&item.after_fen, score)));
            }
        }
        points
    }

    fn show_analysis_chart(&mut self, ui: &mut egui::Ui) {
        let points = self.analysis_chart_points();
        ui.label(egui::RichText::new("Evaluation dynamics").strong());
        if points.len() < 2 {
            ui.small("The chart appears as analysed scores arrive. It plots the evaluation after each ply from White's perspective.");
            return;
        }

        let desired_size = egui::vec2(ui.available_width().max(260.0), 170.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        painter.rect_filled(rect, 6.0, visuals.extreme_bg_color);
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );

        let plot_rect = rect.shrink2(egui::vec2(12.0, 14.0));
        let max_ply = points.last().map(|(ply, _)| *ply).unwrap_or(1).max(1);
        let max_abs_cp = points
            .iter()
            .map(|(_, score)| analysis_chart_clamp_cp(*score).abs())
            .max()
            .unwrap_or(300)
            .clamp(300, ANALYSIS_CHART_ABS_CP_CAP);

        let zero_y = analysis_chart_y(plot_rect, 0, max_abs_cp);
        painter.line_segment(
            [egui::pos2(plot_rect.left(), zero_y), egui::pos2(plot_rect.right(), zero_y)],
            egui::Stroke::new(1.0, visuals.widgets.noninteractive.fg_stroke.color.gamma_multiply(0.45)),
        );
        let current_ply = self.history_view_ply();
        let current_x = analysis_chart_x(plot_rect, current_ply.min(max_ply), max_ply);
        painter.line_segment(
            [egui::pos2(current_x, plot_rect.top()), egui::pos2(current_x, plot_rect.bottom())],
            egui::Stroke::new(1.0, visuals.selection.stroke.color.gamma_multiply(0.8)),
        );

        let polyline: Vec<egui::Pos2> = points
            .iter()
            .map(|(ply, score)| {
                egui::pos2(
                    analysis_chart_x(plot_rect, *ply, max_ply),
                    analysis_chart_y(plot_rect, *score, max_abs_cp),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            polyline,
            egui::Stroke::new(2.0, visuals.hyperlink_color),
        ));

        for (ply, score) in &points {
            let point = egui::pos2(
                analysis_chart_x(plot_rect, *ply, max_ply),
                analysis_chart_y(plot_rect, *score, max_abs_cp),
            );
            let radius = if *ply == current_ply { 4.5 } else { 2.8 };
            let fill = if *score >= 0 {
                visuals.selection.bg_fill
            } else {
                egui::Color32::from_rgb(220, 110, 90)
            };
            painter.circle_filled(point, radius, fill);
        }

        let hovered = response
            .hover_pos()
            .map(|pointer| nearest_analysis_point(pointer, plot_rect, &points, max_ply, max_abs_cp))
            .or_else(|| points.iter().find(|(ply, _)| *ply == current_ply).copied());

        if let Some((ply, score)) = hovered {
            let point = egui::pos2(
                analysis_chart_x(plot_rect, ply, max_ply),
                analysis_chart_y(plot_rect, score, max_abs_cp),
            );
            painter.circle_stroke(point, 6.5, egui::Stroke::new(1.0, visuals.text_color()));
            let label = format!("ply {ply}: {}", format_eval_cp_value(score));
            painter.text(
                egui::pos2(plot_rect.left(), plot_rect.top() - 2.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::TextStyle::Small.resolve(ui.style()),
                visuals.text_color(),
            );
            if response.clicked() {
                self.navigate_history_to(ply);
            }
        }

        painter.text(
            egui::pos2(plot_rect.left(), plot_rect.bottom() + 2.0),
            egui::Align2::LEFT_TOP,
            "0",
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
        painter.text(
            egui::pos2(plot_rect.right(), plot_rect.bottom() + 2.0),
            egui::Align2::RIGHT_TOP,
            format!("{} plies", max_ply),
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
    }

    fn show_analysis_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Game analysis");
        ui.label(&self.analysis_status);
        ui.small("Flow: paste or open a PGN below, then press Start analysis. If the PGN buffer is empty, the current board history is analysed instead.");
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
            if ui.button("Use current game PGN").clicked() {
                self.export_pgn_to_text();
            }
        });

        ui.collapsing("Analysis source (PGN)", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Load current PGN text").clicked() {
                    self.load_pgn_from_text();
                }
                if ui.button("Open PGN path").clicked() {
                    self.open_pgn_from_file();
                }
                if ui.button("Save PGN path").clicked() {
                    self.save_pgn_to_file();
                }
                if ui.button("Clear PGN text").clicked() {
                    self.pgn_text.clear();
                }
            });
            ui.horizontal(|ui| {
                ui.label("PGN path");
                ui.text_edit_singleline(&mut self.pgn_path);
            });
            ui.add(
                egui::TextEdit::multiline(&mut self.pgn_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(8),
            );
        });

        if let Some(analysis) = self.analysis.clone() {
            let done = analysis.completed_jobs();
            let total = analysis.total_jobs();
            let progress = if total == 0 { 0.0 } else { done as f32 / total as f32 };
            ui.add(egui::ProgressBar::new(progress).text(format!("{done}/{total} evals")));
            let summary = analysis.summary();
            ui.group(|ui| {
                ui.label(egui::RichText::new("Accuracy summary").strong());
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(format!("White: {}", format_accuracy(summary.white_accuracy)));
                    ui.separator();
                    ui.monospace(format!("Black: {}", format_accuracy(summary.black_accuracy)));
                });
                ui.label(summary.verdict.clone());
            });
            ui.add_space(4.0);
            self.show_analysis_chart(ui);
            ui.separator();
            ui.label(egui::RichText::new("Analysed moves").strong());
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
        ui.separator();
        ui.label("Per-side power. Movetime > 0 overrides depth for that side.");
        ui.horizontal(|ui| {
            ui.label("White power");
            ui.add(egui::Slider::new(&mut self.match_white_depth, 1..=8).text("depth"));
            ui.add(egui::DragValue::new(&mut self.match_white_movetime_ms).range(0..=60_000).speed(50.0).prefix("ms "));
        });
        ui.horizontal(|ui| {
            ui.label("Black power");
            ui.add(egui::Slider::new(&mut self.match_black_depth, 1..=8).text("depth"));
            ui.add(egui::DragValue::new(&mut self.match_black_movetime_ms).range(0..=60_000).speed(50.0).prefix("ms "));
        });
        ui.collapsing("Per-side UCI options", |ui| {
            ui.label("One option per line. Accepted forms: `Skill Level=0` or `setoption name Skill Level value 0`.");
            ui.columns(2, |columns| {
                columns[0].label("White options");
                columns[0].text_edit_multiline(&mut self.match_white_options);
                columns[1].label("Black options");
                columns[1].text_edit_multiline(&mut self.match_black_options);
            });
        });
        ui.horizontal(|ui| {
            ui.label("Max plies");
            ui.add(egui::DragValue::new(&mut self.match_max_plies).range(2..=600).speed(2.0));
        });
        ui.checkbox(&mut self.avoid_draws, "Internal rchess should avoid draw loops");
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
            if ui
                .add_enabled(!self.match_running && self.match_controller.is_some(), egui::Button::new("Append match to experience book"))
                .clicked()
            {
                self.append_current_match_to_experience_book();
            }
        });
        ui.label(&self.experience_status);
        egui::ScrollArea::vertical()
            .id_salt("match_log_scroll")
            .max_height(100.0)
            .show(ui, |ui| {
                for line in &self.match_log {
                    ui.monospace(line);
                }
            });
    }


    fn show_center_board_panel(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(14, 14, 14));
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 48, 48)),
            egui::StrokeKind::Outside,
        );

        let inner = rect.shrink(10.0);
        if inner.width() < 260.0 || inner.height() < 300.0 {
            painter.text(
                inner.center(),
                egui::Align2::CENTER_CENTER,
                "Window is too small for the board",
                egui::FontId::proportional(16.0),
                egui::Color32::from_rgb(180, 180, 180),
            );
            return;
        }

        let display_position = self.display_position();
        let eval_cp = self.display_eval_cp_white(&display_position);
        let eval_width = 30.0;
        let eval_gap = 8.0;
        let footer_height = 58.0;
        let max_board = (inner.width() - eval_width - eval_gap)
            .min(inner.height() - footer_height)
            .floor();
        let board_size = max_board.clamp(220.0, 720.0);
        let board_size = board_size.min(inner.width() - eval_width - eval_gap).min(inner.height() - footer_height);

        if board_size < 160.0 {
            painter.text(
                inner.center(),
                egui::Align2::CENTER_CENTER,
                "Not enough room for the board",
                egui::FontId::proportional(16.0),
                egui::Color32::from_rgb(180, 180, 180),
            );
            return;
        }

        let group_width = eval_width + eval_gap + board_size;
        let group_left = inner.left() + ((inner.width() - group_width) * 0.5).max(0.0);
        let group_top = inner.top() + ((inner.height() - footer_height - board_size) * 0.40).max(0.0);
        let eval_rect = egui::Rect::from_min_size(
            egui::pos2(group_left, group_top),
            egui::vec2(eval_width, board_size),
        );
        let board_rect = egui::Rect::from_min_size(
            egui::pos2(group_left + eval_width + eval_gap, group_top),
            egui::vec2(board_size, board_size),
        );

        self.paint_evaluation_bar_in_rect(ui, eval_rect, eval_cp);

        let response = ui.interact(
            board_rect,
            ui.make_persistent_id("rchess_main_board_interact"),
            egui::Sense::click_and_drag(),
        );

        if self.is_history_view_live() && response.drag_started() {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                if let Some(square) = pointer_to_square(board_rect, pointer_pos, self.flipped) {
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

        self.paint_board(ui, board_rect, &display_position);

        if response.drag_stopped() {
            if self.dragging_from.is_some() {
                if let Some(pointer_pos) = self.drag_pointer.or_else(|| response.interact_pointer_pos()) {
                    if let Some(square) = pointer_to_square(board_rect, pointer_pos, self.flipped) {
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
                if let Some(square) = pointer_to_square(board_rect, pointer_pos, self.flipped) {
                    self.select_square(square);
                }
            }
        }

        let footer_top = board_rect.bottom() + 8.0;
        let footer_rect = egui::Rect::from_min_max(
            egui::pos2(inner.left(), footer_top),
            egui::pos2(inner.right(), inner.bottom()),
        );
        ui.allocate_ui_at_rect(footer_rect, |ui| {
            ui.set_clip_rect(footer_rect);
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format!("{} | eval {}", self.history_view_label(), format_eval_cp_value(eval_cp)));
                if !self.is_history_view_live() && ui.button("Return live").clicked() {
                    self.history_to_live();
                }
            });
            egui::ScrollArea::horizontal()
                .id_salt("display_fen_scroll_fixed_center")
                .max_height(24.0)
                .show(ui, |ui| {
                    ui.monospace(display_position.to_fen());
                });
        });
    }

    fn paint_evaluation_bar_in_rect(&self, ui: &mut egui::Ui, rect: egui::Rect, score_cp: i32) {
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
        painter.text(
            rect.center_bottom() + egui::vec2(0.0, 18.0),
            egui::Align2::CENTER_CENTER,
            format_eval_cp_value(score_cp),
            egui::FontId::monospace(12.0),
            egui::Color32::from_rgb(190, 190, 190),
        );
    }

    fn show_board(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            let display_position = self.display_position();
            let eval_cp = self.display_eval_cp_white(&display_position);
            let available_width = ui.available_width().max(320.0);
            let available_height = ui.available_height().max(320.0);
            let board_size = (available_width - 46.0)
                .min(available_height - 74.0)
                .clamp(260.0, 720.0);
            let row_width = board_size + 38.0;
            let leading_space = ((available_width - row_width) * 0.5).max(0.0);

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(leading_space);
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
                ui.monospace(format!("{} | eval {}", self.history_view_label(), format_eval_cp_value(eval_cp)));
                if !self.is_history_view_live() && ui.button("Return live").clicked() {
                    self.history_to_live();
                }
            });
            egui::ScrollArea::horizontal()
                .id_salt("display_fen_scroll")
                .max_height(24.0)
                .show(ui, |ui| {
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
            ui.monospace(format_eval_cp_value(score_cp));
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
        let active_animation = self
            .move_animation
            .as_ref()
            .filter(|animation| self.is_history_view_live() && !animation.is_finished());
        let animated_to = active_animation.map(|animation| animation.chess_move.to);
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
                    self.light_square_color
                } else {
                    self.dark_square_color
                };

                if check_square == Some(square) {
                    fill = self.check_square_color;
                } else if selected == Some(square) {
                    fill = self.selected_square_color;
                } else if self.dragging_from.is_some() && drag_target == Some(square) {
                    fill = self.drag_target_color;
                }

                painter.rect_filled(square_rect, 0.0, fill);

                if legal_targets.contains(&square) {
                    let center = square_rect.center();
                    if display_position.piece_at(square).is_some() {
                        painter.circle_stroke(
                            center,
                            square_size * 0.36,
                            egui::Stroke::new(4.0, self.legal_capture_color),
                        );
                    } else {
                        painter.circle_filled(
                            center,
                            square_size * 0.13,
                            self.legal_move_color,
                        );
                    }
                }

                if self.dragging_from != Some(square) && animated_to != Some(square) {
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

        if let Some(animation) = active_animation {
            let from = square_center(rect, animation.chess_move.from, self.flipped);
            let to = square_center(rect, animation.chess_move.to, self.flipped);
            let center = interpolate_pos(from, to, animation.eased_progress());
            painter.circle_filled(
                center,
                square_size * 0.44,
                egui::Color32::from_rgba_premultiplied(245, 235, 210, 36),
            );
            self.paint_piece(&painter, animation.piece, center, square_size);
        }

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
        let glyph = self.piece_glyph(piece);
        let font = egui::FontId::proportional(square_size * self.piece_scale);
        let piece_color = match piece.color {
            Color::White => self.white_piece_color,
            Color::Black => self.black_piece_color,
        };
        painter.text(
            center + egui::vec2(1.3, 1.3),
            egui::Align2::CENTER_CENTER,
            &glyph,
            font.clone(),
            self.piece_shadow_color,
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            glyph,
            font,
            piece_color,
        );
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
        if !self.show_coordinates {
            return;
        }
        let text_color = if is_light {
            self.coordinate_light_color
        } else {
            self.coordinate_dark_color
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
        self.finish_move_animation_if_done();
        self.handle_history_keyboard(ui.ctx());

        self.show_top_panel(ui);
        ui.separator();

        let available = ui.available_size_before_wrap();
        let available = egui::vec2(available.x.max(640.0), available.y.max(420.0));
        let (root_rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());

        let gap = 8.0;
        let root_width = root_rect.width();
        let root_height = root_rect.height();
        let mut left_width = if root_width >= 980.0 { 220.0 } else { 190.0 };
        let mut right_width = if root_width >= 1180.0 {
            340.0
        } else if root_width >= 1000.0 {
            300.0
        } else if root_width >= 860.0 {
            240.0
        } else {
            0.0
        };
        let min_center_width = 360.0;
        if root_width - left_width - right_width - gap * 2.0 < min_center_width {
            right_width = (root_width - left_width - gap * 2.0 - min_center_width)
                .max(0.0)
                .min(right_width);
        }
        if root_width - left_width - right_width - gap * 2.0 < min_center_width {
            left_width = (root_width - right_width - gap * 2.0 - min_center_width)
                .max(150.0)
                .min(left_width);
        }
        let center_width = (root_width - left_width - right_width - gap * 2.0).max(0.0);

        let left_rect = egui::Rect::from_min_size(
            root_rect.left_top(),
            egui::vec2(left_width, root_height),
        );
        let center_rect = egui::Rect::from_min_size(
            egui::pos2(left_rect.right() + gap, root_rect.top()),
            egui::vec2(center_width, root_height),
        );
        let right_rect = if right_width > 1.0 {
            Some(egui::Rect::from_min_size(
                egui::pos2(center_rect.right() + gap, root_rect.top()),
                egui::vec2(right_width, root_height),
            ))
        } else {
            None
        };

        ui.allocate_ui_at_rect(left_rect, |ui| {
            ui.set_clip_rect(left_rect);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width((left_width - 12.0).max(120.0));
                egui::ScrollArea::vertical()
                    .id_salt("left_panel_scroll_fixed")
                    .max_height(root_height - 8.0)
                    .show(ui, |ui| self.show_left_panel(ui));
            });
        });

        ui.allocate_ui_at_rect(center_rect, |ui| {
            ui.set_clip_rect(center_rect);
            self.show_center_board_panel(ui, center_rect);
        });

        if let Some(right_rect) = right_rect {
            ui.allocate_ui_at_rect(right_rect, |ui| {
                ui.set_clip_rect(right_rect);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width((right_width - 12.0).max(140.0));
                    egui::ScrollArea::vertical()
                        .id_salt("right_workspace_scroll_fixed")
                        .max_height(root_height - 8.0)
                        .show(ui, |ui| self.show_side_panel(ui));
                });
            });
        }

        self.show_promotion_dialog(ui.ctx());

        if self.move_animation.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        if self.pending_engine || self.match_running || self.analysis_running {
            ui.ctx().request_repaint_after(Duration::from_millis(80));
        }
    }
}

struct UciCommand {
    label: String,
    program: PathBuf,
    args: Vec<String>,
    is_internal_rchess: bool,
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

fn color_row(ui: &mut egui::Ui, label: &str, color: &mut egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.color_edit_button_srgba(color);
        ui.monospace(format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b()));
    });
}

fn send_rchess_resource_options(
    engine: &mut UciEngine,
    deterministic_multithread: bool,
    max_threads: u16,
    granularity: u16,
    hash_mb: u32,
    avoid_draws: bool,
) -> std::io::Result<()> {
    engine.send(&format!(
        "setoption name deterministic_multithread value {}",
        deterministic_multithread
    ))?;
    engine.send(&format!("setoption name max_threads value {}", max_threads.max(1)))?;
    engine.send(&format!("setoption name granularity value {}", granularity.max(1)))?;
    engine.send(&format!("setoption name Hash value {}", hash_mb.max(1)))?;
    engine.send(&format!("setoption name AvoidDraws value {}", avoid_draws))?;
    Ok(())
}


fn send_rchess_experience_options(engine: &mut UciEngine, config: &ExperienceConfig) -> std::io::Result<()> {
    let config = config.clone().normalized();
    engine.send(&format!("setoption name UseExperienceBook value {}", config.enabled))?;
    engine.send(&format!("setoption name ExperienceBookPath value {}", config.path))?;
    engine.send(&format!("setoption name ExperienceMinGames value {}", config.min_games))?;
    engine.send(&format!(
        "setoption name ExperienceScoreToleranceCp value {}",
        config.score_tolerance_cp
    ))?;
    Ok(())
}


fn send_match_engine_startup_options(
    engine: &mut UciEngine,
    is_internal_rchess: bool,
    deterministic_multithread: bool,
    max_threads: u16,
    granularity: u16,
    hash_mb: u32,
    avoid_draws: bool,
    experience: &ExperienceConfig,
    extra_options: &str,
) -> Result<(), String> {
    if is_internal_rchess {
        send_rchess_resource_options(
            engine,
            deterministic_multithread,
            max_threads,
            granularity,
            hash_mb,
            avoid_draws,
        )
        .map_err(|error| error.to_string())?;
        send_rchess_experience_options(engine, experience).map_err(|error| error.to_string())?;
    }
    send_uci_option_lines(engine, extra_options)
}

fn send_uci_option_lines(engine: &mut UciEngine, options: &str) -> Result<(), String> {
    for line in options.lines() {
        let Some(command) = normalize_uci_option_line(line) else {
            continue;
        };
        engine.send(&command).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn normalize_uci_option_line(line: &str) -> Option<String> {
    let trimmed = line.split('#').next().unwrap_or("").trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.to_ascii_lowercase().starts_with("setoption ") {
        return Some(trimmed.to_string());
    }
    if trimmed.to_ascii_lowercase().starts_with("name ") {
        return Some(format!("setoption {trimmed}"));
    }
    if let Some((name, value)) = trimmed.split_once('=') {
        return Some(format!("setoption name {} value {}", name.trim(), value.trim()));
    }
    Some(format!("setoption name {trimmed}"))
}

fn default_parallel_threads() -> u16 {
    std::thread::available_parallelism()
        .map(|threads| threads.get().clamp(1, 32) as u16)
        .unwrap_or(1)
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

fn square_center(rect: egui::Rect, square: u8, flipped: bool) -> egui::Pos2 {
    let rank = square / 8;
    let file = square % 8;
    let (row, col) = if flipped {
        (rank, 7 - file)
    } else {
        (7 - rank, file)
    };
    let square_size = rect.width() / 8.0;
    egui::pos2(
        rect.left() + (col as f32 + 0.5) * square_size,
        rect.top() + (row as f32 + 0.5) * square_size,
    )
}

fn interpolate_pos(from: egui::Pos2, to: egui::Pos2, t: f32) -> egui::Pos2 {
    from + (to - from) * t.clamp(0.0, 1.0)
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

const GUI_MATE_SCORE_CP: i32 = 32_000;
const ANALYSIS_CHART_ABS_CP_CAP: i32 = 1_200;

fn analysis_chart_clamp_cp(score_cp: i32) -> i32 {
    if score_cp >= GUI_MATE_SCORE_CP / 2 {
        ANALYSIS_CHART_ABS_CP_CAP
    } else if score_cp <= -GUI_MATE_SCORE_CP / 2 {
        -ANALYSIS_CHART_ABS_CP_CAP
    } else {
        score_cp.clamp(-ANALYSIS_CHART_ABS_CP_CAP, ANALYSIS_CHART_ABS_CP_CAP)
    }
}

fn analysis_chart_x(rect: egui::Rect, ply: usize, max_ply: usize) -> f32 {
    if max_ply == 0 {
        return rect.left();
    }
    rect.left() + rect.width() * (ply as f32 / max_ply as f32)
}

fn analysis_chart_y(rect: egui::Rect, score_cp: i32, max_abs_cp: i32) -> f32 {
    let clamped = analysis_chart_clamp_cp(score_cp) as f32;
    let span = max_abs_cp.max(1) as f32;
    let normalized = (clamped / span).clamp(-1.0, 1.0);
    rect.center().y - normalized * rect.height() * 0.5
}

fn nearest_analysis_point(
    pointer: egui::Pos2,
    rect: egui::Rect,
    points: &[(usize, i32)],
    max_ply: usize,
    max_abs_cp: i32,
) -> (usize, i32) {
    points
        .iter()
        .copied()
        .min_by(|(ply_a, score_a), (ply_b, score_b)| {
            let point_a = egui::pos2(
                analysis_chart_x(rect, *ply_a, max_ply),
                analysis_chart_y(rect, *score_a, max_abs_cp),
            );
            let point_b = egui::pos2(
                analysis_chart_x(rect, *ply_b, max_ply),
                analysis_chart_y(rect, *score_b, max_abs_cp),
            );
            point_a
                .distance_sq(pointer)
                .partial_cmp(&point_b.distance_sq(pointer))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or((0, 0))
}

fn terminal_score_white(position: &Position) -> Option<i32> {
    if position.is_checkmate() {
        Some(match position.side_to_move() {
            Color::White => -GUI_MATE_SCORE_CP,
            Color::Black => GUI_MATE_SCORE_CP,
        })
    } else if position.is_stalemate() || position.is_fifty_move_rule_draw() {
        Some(0)
    } else {
        None
    }
}

fn terminal_score_side_to_move(position: &Position) -> Option<i32> {
    if position.is_checkmate() {
        Some(-GUI_MATE_SCORE_CP)
    } else if position.is_stalemate() || position.is_fifty_move_rule_draw() {
        Some(0)
    } else {
        None
    }
}

fn format_eval_cp_value(score: i32) -> String {
    if score >= GUI_MATE_SCORE_CP / 2 {
        "# White".to_string()
    } else if score <= -GUI_MATE_SCORE_CP / 2 {
        "# Black".to_string()
    } else {
        format_cp_value(score)
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

fn score_for_fen_side_to_move(fen: &str) -> i32 {
    match Position::from_fen(fen) {
        Ok(position) => terminal_score_side_to_move(&position)
            .unwrap_or_else(|| evaluate_tactical_for_side_to_move(&position)),
        Err(_) => 0,
    }
}

fn start_fullmove_number(fen: &str) -> u32 {
    fen.split_whitespace()
        .nth(5)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
}
