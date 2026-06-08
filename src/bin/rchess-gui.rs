use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui;
use rchess::chess::{square_name, ChessMove, Color, PieceKind, Position, STARTPOS_FEN};
use rchess::pgn::{export_pgn, move_to_san, parse_pgn, position_after_moves};

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
        };
        app.refresh_game_status();
        app
    }

    fn new_game(&mut self) {
        self.position = Position::startpos();
        self.fen_input = STARTPOS_FEN.to_string();
        self.game_start_fen = STARTPOS_FEN.to_string();
        self.selected = None;
        self.selected_moves.clear();
        self.dragging_from = None;
        self.drag_pointer = None;
        self.promotion_request = None;
        self.played_moves.clear();
        self.pgn_text.clear();
        self.pending_engine = false;
        self.engine_status = "New game".to_string();
        self.send_to_engine("ucinewgame");
        self.refresh_game_status();

        if self.should_auto_engine_move() {
            self.request_engine_move();
        }
    }

    fn load_fen(&mut self) {
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
                self.pending_engine = false;
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
        match parse_pgn(&self.pgn_text).and_then(|game| {
            let position = position_after_moves(&game.start_fen, &game.moves)?;
            Ok((game, position))
        }) {
            Ok((game, position)) => {
                self.game_start_fen = game.start_fen;
                self.played_moves = game.moves;
                self.position = position;
                self.fen_input = self.position.to_fen();
                self.selected = None;
                self.selected_moves.clear();
                self.dragging_from = None;
                self.drag_pointer = None;
                self.promotion_request = None;
                self.pending_engine = false;
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
        if self.pending_engine || self.promotion_request.is_some() {
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
        if self.pending_engine || self.promotion_request.is_some() {
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
            && self.position.side_to_move() != self.player_color
            && !self.position.is_checkmate()
            && !self.position.is_stalemate()
    }

    fn try_apply_selected_to(&mut self, to: u8) -> bool {
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
                self.played_moves.push(chess_move);
                self.fen_input = self.position.to_fen();
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

    fn stop_engine(&mut self, status: impl Into<String>) {
        self.engine = None;
        self.engine_rx = None;
        self.pending_engine = false;
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
        if self.pending_engine {
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
                    self.played_moves.push(chess_move);
                    self.fen_input = self.position.to_fen();
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
        let previous_player_color = self.player_color;
        let previous_auto_engine = self.auto_engine;

        egui::TopBottomPanel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("New game").clicked() {
                    self.new_game();
                }
                if ui
                    .add_enabled(!self.pending_engine, egui::Button::new("Engine move"))
                    .clicked()
                {
                    self.request_engine_move();
                }
                ui.add(egui::Slider::new(&mut self.search_depth, 1..=8).text("Depth"));
                ui.checkbox(&mut self.auto_engine, "Auto engine reply");
                ui.checkbox(&mut self.flipped, "Flip board");
                egui::ComboBox::from_id_salt("player_color")
                    .selected_text(color_name(self.player_color))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.player_color, Color::White, "White");
                        ui.selectable_value(&mut self.player_color, Color::Black, "Black");
                    });
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
            .default_width(360.0)
            .show_inside(ui, |ui| {
                ui.heading("Position");
                ui.label(&self.game_status);
                ui.label(&self.engine_status);
                ui.separator();

                ui.label("FEN");
                ui.text_edit_multiline(&mut self.fen_input);
                ui.horizontal(|ui| {
                    if ui.button("Load FEN").clicked() {
                        self.load_fen();
                    }
                    if ui.button("Copy current").clicked() {
                        self.fen_input = self.position.to_fen();
                    }
                });

                ui.separator();
                ui.heading("PGN");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Export PGN").clicked() {
                        self.export_pgn_to_text();
                    }
                    if ui.button("Copy PGN").clicked() {
                        self.copy_pgn_to_clipboard(ui.ctx());
                    }
                    if ui.button("Load PGN").clicked() {
                        self.load_pgn_from_text();
                    }
                    if ui.button("Clear PGN").clicked() {
                        self.pgn_text.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.text_edit_singleline(&mut self.pgn_path);
                });
                ui.horizontal(|ui| {
                    if ui.button("Open PGN file").clicked() {
                        self.open_pgn_from_file();
                    }
                    if ui.button("Save PGN file").clicked() {
                        self.save_pgn_to_file();
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("pgn_text_scroll")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.pgn_text)
                                .font(egui::TextStyle::Monospace)
                                .desired_rows(7),
                        );
                    });

                ui.separator();
                ui.heading("Engine backend");
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
                        ui.label("Optional external engine. Build the vendored Stockfish 10 source, then point this field to the resulting executable.");
                        ui.horizontal(|ui| {
                            ui.label("Stockfish 10");
                            ui.text_edit_singleline(&mut self.stockfish10_path);
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Detect Stockfish 10").clicked() {
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

                ui.horizontal(|ui| {
                    if ui.button("Restart UCI child").clicked() {
                        self.stop_engine("Restarting UCI child");
                        match self.ensure_engine() {
                            Ok(()) => {}
                            Err(error) => self.engine_status = error,
                        }
                    }
                    if ui.button("Stop engine").clicked() {
                        self.stop_engine("UCI child stopped");
                    }
                });

                ui.separator();
                ui.heading("Moves SAN");
                egui::ScrollArea::vertical()
                    .id_salt("moves_scroll")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for row in self.san_move_rows() {
                            ui.monospace(row);
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.heading("UCI log");
                    if ui.button("Clear").clicked() {
                        self.engine_log.clear();
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("uci_log_scroll")
                    .max_height(260.0)
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

    fn show_board(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                let max_size = ui.available_width().min(ui.available_height() - 28.0);
                let board_size = max_size.clamp(320.0, 640.0);
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(board_size, board_size),
                    egui::Sense::click_and_drag(),
                );

                if response.drag_started() {
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

                self.paint_board(ui, rect);

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
                } else if response.clicked() {
                    if let Some(pointer_pos) = response.interact_pointer_pos() {
                        if let Some(square) = pointer_to_square(rect, pointer_pos, self.flipped) {
                            self.select_square(square);
                        }
                    }
                }

                ui.add_space(8.0);
                ui.monospace(self.position.to_fen());
            });
        });
    }

    fn paint_board(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter_at(rect);
        let square_size = rect.width() / 8.0;
        let selected = self.selected;
        let legal_targets: Vec<u8> = self.selected_moves.iter().map(|chess_move| chess_move.to).collect();
        let check_square = self.checked_king_square();
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
                    if self.position.piece_at(square).is_some() {
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
                    if let Some(piece) = self.position.piece_at(square) {
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
            if let Some(piece) = self.position.piece_at(from) {
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

    fn checked_king_square(&self) -> Option<u8> {
        let color = self.position.side_to_move();
        if !self.position.is_in_check(color) {
            return None;
        }

        for square in 0..64 {
            if let Some(piece) = self.position.piece_at(square) {
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
        self.show_top_panel(ui);
        self.show_side_panel(ui);
        self.show_board(ui);
        self.show_promotion_dialog(ui.ctx());

        if self.pending_engine {
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

fn start_fullmove_number(fen: &str) -> u32 {
    fen.split_whitespace()
        .nth(5)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
}
