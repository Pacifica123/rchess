use std::env;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui;
use rchess::chess::{square_name, ChessMove, Color, PieceKind, Position, STARTPOS_FEN};

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

struct RChessGui {
    position: Position,
    fen_input: String,
    selected: Option<u8>,
    selected_moves: Vec<ChessMove>,
    played_moves: Vec<String>,
    player_color: Color,
    auto_engine: bool,
    flipped: bool,
    search_depth: u8,
    pending_engine: bool,
    engine_status: String,
    game_status: String,
    engine_path: String,
    engine: Option<UciEngine>,
    engine_rx: Option<Receiver<String>>,
    engine_log: Vec<String>,
}

impl RChessGui {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let position = Position::startpos();
        let mut app = Self {
            fen_input: STARTPOS_FEN.to_string(),
            position,
            selected: None,
            selected_moves: Vec::new(),
            played_moves: Vec::new(),
            player_color: Color::White,
            auto_engine: true,
            flipped: false,
            search_depth: 4,
            pending_engine: false,
            engine_status: "UCI child process is not started yet".to_string(),
            game_status: String::new(),
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
        self.selected = None;
        self.selected_moves.clear();
        self.played_moves.clear();
        self.pending_engine = false;
        self.engine_status = "New game".to_string();
        self.send_to_engine("ucinewgame");
        self.refresh_game_status();

        if self.auto_engine && self.position.side_to_move() != self.player_color {
            self.request_engine_move();
        }
    }

    fn load_fen(&mut self) {
        match Position::from_fen(self.fen_input.trim()) {
            Ok(position) => {
                self.position = position;
                self.selected = None;
                self.selected_moves.clear();
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

    fn select_square(&mut self, square: u8) {
        if self.pending_engine {
            return;
        }

        if let Some(selected) = self.selected {
            if selected == square {
                self.clear_selection();
                return;
            }

            if let Some(chess_move) = self.move_from_selected_to(square) {
                self.apply_user_move(chess_move);
                return;
            }
        }

        if let Some(piece) = self.position.piece_at(square) {
            if piece.color == self.position.side_to_move() {
                self.selected = Some(square);
                self.selected_moves = self
                    .position
                    .legal_moves()
                    .into_iter()
                    .filter(|chess_move| chess_move.from == square)
                    .collect();
                return;
            }
        }

        self.clear_selection();
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.selected_moves.clear();
    }

    fn move_from_selected_to(&self, to: u8) -> Option<ChessMove> {
        let mut candidates = self
            .selected_moves
            .iter()
            .copied()
            .filter(|chess_move| chess_move.to == to);
        let first = candidates.next()?;
        if first.promotion == Some(PieceKind::Queen) {
            return Some(first);
        }
        candidates
            .find(|chess_move| chess_move.promotion == Some(PieceKind::Queen))
            .or(Some(first))
    }

    fn apply_user_move(&mut self, chess_move: ChessMove) {
        let move_text = chess_move.to_uci();
        match self.position.make_legal_move(chess_move) {
            Ok(()) => {
                self.played_moves.push(move_text);
                self.fen_input = self.position.to_fen();
                self.clear_selection();
                self.refresh_game_status();

                if self.auto_engine
                    && self.position.side_to_move() != self.player_color
                    && !self.position.is_checkmate()
                    && !self.position.is_stalemate()
                {
                    self.request_engine_move();
                }
            }
            Err(error) => {
                self.engine_status = error;
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

        let (engine, rx) = UciEngine::spawn(self.engine_path.trim())?;
        self.engine = Some(engine);
        self.engine_rx = Some(rx);
        self.send_to_engine("uci");
        self.send_to_engine("isready");
        self.engine_status = "UCI child process started".to_string();
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

        if line == "uciok" {
            self.engine_status = "UCI handshake complete".to_string();
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
                    self.played_moves.push(move_text.to_string());
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

    fn show_top_panel(&mut self, ui: &mut egui::Ui) {
        egui::TopBottomPanel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("New game").clicked() {
                    self.new_game();
                }
                if ui.button("Engine move").clicked() {
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
    }

    fn show_side_panel(&mut self, ui: &mut egui::Ui) {
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
                ui.label("External engine path. Leave empty to run this GUI binary as a UCI child process.");
                ui.text_edit_singleline(&mut self.engine_path);
                if ui.button("Restart UCI child").clicked() {
                    self.engine = None;
                    self.engine_rx = None;
                    self.pending_engine = false;
                    match self.ensure_engine() {
                        Ok(()) => {}
                        Err(error) => self.engine_status = error,
                    }
                }

                ui.separator();
                ui.heading("Moves");
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for (index, move_text) in self.played_moves.iter().enumerate() {
                            if index % 2 == 0 {
                                ui.label(format!("{}. {}", index / 2 + 1, move_text));
                            } else {
                                ui.label(format!("   ... {move_text}"));
                            }
                        }
                    });

                ui.separator();
                ui.heading("UCI log");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for line in &self.engine_log {
                        ui.monospace(line);
                    }
                });
            });
    }

    fn show_board(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                egui::Grid::new("board_grid")
                    .spacing(egui::vec2(0.0, 0.0))
                    .show(ui, |ui| {
                        for row in 0..8 {
                            for col in 0..8 {
                                let square = view_square(row, col, self.flipped);
                                let clicked = self.square_button(ui, square, row, col).clicked();
                                if clicked {
                                    self.select_square(square);
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
                ui.monospace(self.position.to_fen());
            });
        });
    }

    fn square_button(&self, ui: &mut egui::Ui, square: u8, row: usize, col: usize) -> egui::Response {
        let piece = self.position.piece_at(square);
        let mut text = piece
            .map(|piece| piece.unicode().to_string())
            .unwrap_or_else(|| " ".to_string());
        if text == " " {
            text.push(' ');
        }

        let is_light = (row + col) % 2 == 0;
        let is_selected = self.selected == Some(square);
        let is_target = self.selected_moves.iter().any(|chess_move| chess_move.to == square);
        let fill = if is_selected {
            egui::Color32::from_rgb(190, 170, 80)
        } else if is_target {
            egui::Color32::from_rgb(120, 150, 90)
        } else if is_light {
            egui::Color32::from_rgb(235, 220, 190)
        } else {
            egui::Color32::from_rgb(120, 85, 60)
        };
        let text_color = if is_light {
            egui::Color32::from_rgb(25, 25, 25)
        } else {
            egui::Color32::from_rgb(240, 240, 240)
        };
        let label = egui::RichText::new(text).size(42.0).color(text_color);

        ui.add(
            egui::Button::new(label)
                .min_size(egui::vec2(64.0, 64.0))
                .fill(fill),
        )
        .on_hover_text(square_name(square))
    }
}

impl eframe::App for RChessGui {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_engine();
        self.show_top_panel(ui);
        self.show_side_panel(ui);
        self.show_board(ui);

        if self.pending_engine {
            ui.ctx().request_repaint_after(Duration::from_millis(80));
        }
    }
}

struct UciEngine {
    child: Child,
    stdin: ChildStdin,
}

impl UciEngine {
    fn spawn(engine_path: &str) -> Result<(Self, Receiver<String>), String> {
        let mut command = if engine_path.is_empty() {
            let current = env::current_exe().map_err(|error| format!("cannot locate current executable: {error}"))?;
            let mut command = Command::new(current);
            command.arg("--engine-mode");
            command
        } else {
            Command::new(engine_path)
        };

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
