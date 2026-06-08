use std::fmt;

pub const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    pub fn fen(self) -> char {
        match self {
            Self::White => 'w',
            Self::Black => 'b',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    pub fn material_value(self) -> i32 {
        match self {
            Self::Pawn => 100,
            Self::Knight => 320,
            Self::Bishop => 330,
            Self::Rook => 500,
            Self::Queen => 900,
            Self::King => 0,
        }
    }

    pub fn from_promotion_char(value: char) -> Option<Self> {
        match value.to_ascii_lowercase() {
            'n' => Some(Self::Knight),
            'b' => Some(Self::Bishop),
            'r' => Some(Self::Rook),
            'q' => Some(Self::Queen),
            _ => None,
        }
    }

    pub fn promotion_char(self) -> Option<char> {
        match self {
            Self::Knight => Some('n'),
            Self::Bishop => Some('b'),
            Self::Rook => Some('r'),
            Self::Queen => Some('q'),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

impl Piece {
    pub fn from_fen(value: char) -> Option<Self> {
        let color = if value.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        let kind = match value.to_ascii_lowercase() {
            'p' => PieceKind::Pawn,
            'n' => PieceKind::Knight,
            'b' => PieceKind::Bishop,
            'r' => PieceKind::Rook,
            'q' => PieceKind::Queen,
            'k' => PieceKind::King,
            _ => return None,
        };
        Some(Self { color, kind })
    }

    pub fn fen(self) -> char {
        let value = match self.kind {
            PieceKind::Pawn => 'p',
            PieceKind::Knight => 'n',
            PieceKind::Bishop => 'b',
            PieceKind::Rook => 'r',
            PieceKind::Queen => 'q',
            PieceKind::King => 'k',
        };
        match self.color {
            Color::White => value.to_ascii_uppercase(),
            Color::Black => value,
        }
    }

    pub fn unicode(self) -> char {
        match (self.color, self.kind) {
            (Color::White, PieceKind::King) => '♔',
            (Color::White, PieceKind::Queen) => '♕',
            (Color::White, PieceKind::Rook) => '♖',
            (Color::White, PieceKind::Bishop) => '♗',
            (Color::White, PieceKind::Knight) => '♘',
            (Color::White, PieceKind::Pawn) => '♙',
            (Color::Black, PieceKind::King) => '♚',
            (Color::Black, PieceKind::Queen) => '♛',
            (Color::Black, PieceKind::Rook) => '♜',
            (Color::Black, PieceKind::Bishop) => '♝',
            (Color::Black, PieceKind::Knight) => '♞',
            (Color::Black, PieceKind::Pawn) => '♟',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastlingRights {
    pub white_king_side: bool,
    pub white_queen_side: bool,
    pub black_king_side: bool,
    pub black_queen_side: bool,
}

impl CastlingRights {
    pub fn none() -> Self {
        Self {
            white_king_side: false,
            white_queen_side: false,
            black_king_side: false,
            black_queen_side: false,
        }
    }

    pub fn from_fen(value: &str) -> Result<Self, String> {
        let mut rights = Self::none();
        if value == "-" {
            return Ok(rights);
        }
        for ch in value.chars() {
            match ch {
                'K' => rights.white_king_side = true,
                'Q' => rights.white_queen_side = true,
                'k' => rights.black_king_side = true,
                'q' => rights.black_queen_side = true,
                _ => return Err(format!("bad castling rights: {value}")),
            }
        }
        Ok(rights)
    }

    pub fn to_fen(self) -> String {
        let mut result = String::new();
        if self.white_king_side {
            result.push('K');
        }
        if self.white_queen_side {
            result.push('Q');
        }
        if self.black_king_side {
            result.push('k');
        }
        if self.black_queen_side {
            result.push('q');
        }
        if result.is_empty() {
            result.push('-');
        }
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChessMove {
    pub from: u8,
    pub to: u8,
    pub promotion: Option<PieceKind>,
}

impl ChessMove {
    pub fn new(from: u8, to: u8, promotion: Option<PieceKind>) -> Self {
        Self { from, to, promotion }
    }

    pub fn to_uci(self) -> String {
        let mut value = format!("{}{}", square_name(self.from), square_name(self.to));
        if let Some(kind) = self.promotion.and_then(PieceKind::promotion_char) {
            value.push(kind);
        }
        value
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    board: [Option<Piece>; 64],
    side_to_move: Color,
    castling: CastlingRights,
    en_passant: Option<u8>,
    halfmove_clock: u32,
    fullmove_number: u32,
}

impl Position {
    pub fn empty() -> Self {
        Self {
            board: [None; 64],
            side_to_move: Color::White,
            castling: CastlingRights::none(),
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        }
    }

    pub fn startpos() -> Self {
        Self::from_fen(STARTPOS_FEN).expect("built-in start position must be valid")
    }

    pub fn from_fen(fen: &str) -> Result<Self, String> {
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 || parts.len() > 6 {
            return Err("FEN must contain 4 to 6 fields".to_string());
        }

        let mut position = Self::empty();
        let ranks: Vec<&str> = parts[0].split('/').collect();
        if ranks.len() != 8 {
            return Err("FEN board must contain 8 ranks".to_string());
        }

        for (rank_index, rank_text) in ranks.iter().enumerate() {
            let rank = 7_i32 - rank_index as i32;
            let mut file = 0_i32;
            for ch in rank_text.chars() {
                if let Some(empty_count) = ch.to_digit(10) {
                    if empty_count == 0 || empty_count > 8 {
                        return Err(format!("bad empty-square count in FEN: {ch}"));
                    }
                    file += empty_count as i32;
                } else if let Some(piece) = Piece::from_fen(ch) {
                    if file >= 8 {
                        return Err("too many squares in FEN rank".to_string());
                    }
                    let square = index(file, rank).ok_or_else(|| "bad FEN square".to_string())?;
                    position.board[square as usize] = Some(piece);
                    file += 1;
                } else {
                    return Err(format!("bad piece in FEN: {ch}"));
                }
            }
            if file != 8 {
                return Err("FEN rank does not contain 8 squares".to_string());
            }
        }

        position.side_to_move = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err("bad side-to-move field".to_string()),
        };
        position.castling = CastlingRights::from_fen(parts[2])?;
        position.en_passant = if parts[3] == "-" {
            None
        } else {
            let square = parse_square(parts[3]).ok_or_else(|| "bad en-passant square".to_string())?;
            let rank = rank_of(square);
            if rank != 2 && rank != 5 {
                return Err("bad en-passant rank".to_string());
            }
            Some(square)
        };
        position.halfmove_clock = if parts.len() >= 5 {
            parts[4]
                .parse::<u32>()
                .map_err(|_| "bad halfmove clock".to_string())?
        } else {
            0
        };
        position.fullmove_number = if parts.len() >= 6 {
            parts[5]
                .parse::<u32>()
                .map_err(|_| "bad fullmove number".to_string())?
        } else {
            1
        };
        if position.king_square(Color::White).is_none() || position.king_square(Color::Black).is_none() {
            return Err("FEN must contain both kings".to_string());
        }
        Ok(position)
    }

    pub fn to_fen(&self) -> String {
        let mut board_part = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                let square = (rank * 8 + file) as usize;
                if let Some(piece) = self.board[square] {
                    if empty > 0 {
                        board_part.push_str(&empty.to_string());
                        empty = 0;
                    }
                    board_part.push(piece.fen());
                } else {
                    empty += 1;
                }
            }
            if empty > 0 {
                board_part.push_str(&empty.to_string());
            }
            if rank > 0 {
                board_part.push('/');
            }
        }
        format!(
            "{} {} {} {} {} {}",
            board_part,
            self.side_to_move.fen(),
            self.castling.to_fen(),
            self.en_passant.map(square_name).unwrap_or_else(|| "-".to_string()),
            self.halfmove_clock,
            self.fullmove_number
        )
    }

    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub fn piece_at(&self, square: u8) -> Option<Piece> {
        self.board[square as usize]
    }

    pub fn legal_moves(&self) -> Vec<ChessMove> {
        let mut result = Vec::new();
        for chess_move in self.pseudo_legal_moves() {
            let mut next = self.clone();
            if next.apply_unchecked(chess_move).is_ok() && !next.is_in_check(self.side_to_move) {
                result.push(chess_move);
            }
        }
        result
    }

    pub fn legal_captures(&self) -> Vec<ChessMove> {
        self.legal_moves()
            .into_iter()
            .filter(|chess_move| self.is_capture(*chess_move))
            .collect()
    }

    pub fn make_legal_move(&mut self, chess_move: ChessMove) -> Result<(), String> {
        if self.legal_moves().into_iter().any(|candidate| candidate == chess_move) {
            self.apply_unchecked(chess_move)
        } else {
            Err(format!("illegal move: {}", chess_move.to_uci()))
        }
    }

    pub fn parse_uci_move(&self, value: &str) -> Option<ChessMove> {
        if value.len() != 4 && value.len() != 5 {
            return None;
        }
        let from = parse_square(&value[0..2])?;
        let to = parse_square(&value[2..4])?;
        let promotion = if value.len() == 5 {
            PieceKind::from_promotion_char(value.chars().nth(4)?)
        } else {
            None
        };
        self.legal_moves()
            .into_iter()
            .find(|chess_move| chess_move.from == from && chess_move.to == to && chess_move.promotion == promotion)
    }

    pub fn is_checkmate(&self) -> bool {
        self.is_in_check(self.side_to_move) && self.legal_moves().is_empty()
    }

    pub fn is_stalemate(&self) -> bool {
        !self.is_in_check(self.side_to_move) && self.legal_moves().is_empty()
    }

    pub fn is_in_check(&self, color: Color) -> bool {
        match self.king_square(color) {
            Some(square) => self.is_square_attacked(square, color.opposite()),
            None => true,
        }
    }

    pub fn is_capture(&self, chess_move: ChessMove) -> bool {
        let Some(piece) = self.piece_at(chess_move.from) else {
            return false;
        };
        if self.piece_at(chess_move.to).is_some() {
            return true;
        }
        piece.kind == PieceKind::Pawn
            && self.en_passant == Some(chess_move.to)
            && file_of(chess_move.from) != file_of(chess_move.to)
    }

    pub fn ascii_board(&self) -> String {
        let mut result = String::new();
        for rank in (0..8).rev() {
            result.push_str(&(rank + 1).to_string());
            result.push_str("  ");
            for file in 0..8 {
                let square = (rank * 8 + file) as usize;
                let symbol = self.board[square].map(Piece::unicode).unwrap_or('.');
                result.push(symbol);
                result.push(' ');
            }
            result.push('\n');
        }
        result.push_str("   a b c d e f g h");
        result
    }

    pub fn perft(&self, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = self.legal_moves();
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut nodes = 0;
        for chess_move in moves {
            let mut next = self.clone();
            next.apply_unchecked(chess_move).expect("legal move must apply");
            nodes += next.perft(depth - 1);
        }
        nodes
    }

    pub(crate) fn apply_unchecked(&mut self, chess_move: ChessMove) -> Result<(), String> {
        let mut piece = self.board[chess_move.from as usize]
            .take()
            .ok_or_else(|| format!("no piece on {}", square_name(chess_move.from)))?;
        let was_pawn = piece.kind == PieceKind::Pawn;
        let captured = self.capture_for_move(piece, chess_move);

        self.update_castling_rights(piece, chess_move, captured);

        if piece.kind == PieceKind::Pawn
            && self.en_passant == Some(chess_move.to)
            && file_of(chess_move.from) != file_of(chess_move.to)
            && self.board[chess_move.to as usize].is_none()
        {
            let captured_square = match piece.color {
                Color::White => chess_move.to.saturating_sub(8),
                Color::Black => chess_move.to + 8,
            };
            self.board[captured_square as usize] = None;
        } else {
            self.board[chess_move.to as usize] = None;
        }

        if piece.kind == PieceKind::King && file_distance(chess_move.from, chess_move.to) == 2 {
            self.move_castling_rook(piece.color, chess_move.to)?;
        }

        if piece.kind == PieceKind::Pawn && is_promotion_rank(chess_move.to, piece.color) {
            piece.kind = chess_move.promotion.unwrap_or(PieceKind::Queen);
        }

        self.board[chess_move.to as usize] = Some(piece);

        if was_pawn && rank_distance(chess_move.from, chess_move.to) == 2 {
            self.en_passant = Some(match piece.color {
                Color::White => chess_move.from + 8,
                Color::Black => chess_move.from - 8,
            });
        } else {
            self.en_passant = None;
        }

        if was_pawn || captured.is_some() {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }
        if self.side_to_move == Color::Black {
            self.fullmove_number += 1;
        }
        self.side_to_move = self.side_to_move.opposite();
        Ok(())
    }

    fn capture_for_move(&self, piece: Piece, chess_move: ChessMove) -> Option<Piece> {
        if piece.kind == PieceKind::Pawn
            && self.en_passant == Some(chess_move.to)
            && file_of(chess_move.from) != file_of(chess_move.to)
            && self.board[chess_move.to as usize].is_none()
        {
            let captured_square = match piece.color {
                Color::White => chess_move.to.saturating_sub(8),
                Color::Black => chess_move.to + 8,
            };
            self.board[captured_square as usize]
        } else {
            self.board[chess_move.to as usize]
        }
    }

    fn update_castling_rights(&mut self, piece: Piece, chess_move: ChessMove, captured: Option<Piece>) {
        match (piece.color, piece.kind) {
            (Color::White, PieceKind::King) => {
                self.castling.white_king_side = false;
                self.castling.white_queen_side = false;
            }
            (Color::Black, PieceKind::King) => {
                self.castling.black_king_side = false;
                self.castling.black_queen_side = false;
            }
            (Color::White, PieceKind::Rook) => match chess_move.from {
                0 => self.castling.white_queen_side = false,
                7 => self.castling.white_king_side = false,
                _ => {}
            },
            (Color::Black, PieceKind::Rook) => match chess_move.from {
                56 => self.castling.black_queen_side = false,
                63 => self.castling.black_king_side = false,
                _ => {}
            },
            _ => {}
        }

        if let Some(captured_piece) = captured {
            if captured_piece.kind == PieceKind::Rook {
                match chess_move.to {
                    0 => self.castling.white_queen_side = false,
                    7 => self.castling.white_king_side = false,
                    56 => self.castling.black_queen_side = false,
                    63 => self.castling.black_king_side = false,
                    _ => {}
                }
            }
        }
    }

    fn move_castling_rook(&mut self, color: Color, king_to: u8) -> Result<(), String> {
        let (rook_from, rook_to) = match (color, king_to) {
            (Color::White, 6) => (7, 5),
            (Color::White, 2) => (0, 3),
            (Color::Black, 62) => (63, 61),
            (Color::Black, 58) => (56, 59),
            _ => return Err("bad castling move".to_string()),
        };
        let rook = self.board[rook_from as usize]
            .take()
            .ok_or_else(|| "castling rook is absent".to_string())?;
        self.board[rook_to as usize] = Some(rook);
        Ok(())
    }

    fn pseudo_legal_moves(&self) -> Vec<ChessMove> {
        let mut moves = Vec::with_capacity(64);
        for square in 0_u8..64 {
            let Some(piece) = self.board[square as usize] else {
                continue;
            };
            if piece.color != self.side_to_move {
                continue;
            }
            match piece.kind {
                PieceKind::Pawn => self.add_pawn_moves(square, piece, &mut moves),
                PieceKind::Knight => self.add_knight_moves(square, piece, &mut moves),
                PieceKind::Bishop => self.add_slider_moves(square, piece, &mut moves, &BISHOP_DIRS),
                PieceKind::Rook => self.add_slider_moves(square, piece, &mut moves, &ROOK_DIRS),
                PieceKind::Queen => self.add_slider_moves(square, piece, &mut moves, &QUEEN_DIRS),
                PieceKind::King => self.add_king_moves(square, piece, &mut moves),
            }
        }
        moves
    }

    fn add_pawn_moves(&self, square: u8, piece: Piece, moves: &mut Vec<ChessMove>) {
        let direction = match piece.color {
            Color::White => 1,
            Color::Black => -1,
        };
        let rank = rank_of(square);
        let file = file_of(square);
        let one_rank = rank + direction;
        if let Some(to) = index(file, one_rank) {
            if self.board[to as usize].is_none() {
                self.push_pawn_move(square, to, piece.color, moves);
                let start_rank = match piece.color {
                    Color::White => 1,
                    Color::Black => 6,
                };
                if rank == start_rank {
                    if let Some(two) = index(file, rank + direction * 2) {
                        if self.board[two as usize].is_none() {
                            moves.push(ChessMove::new(square, two, None));
                        }
                    }
                }
            }
        }

        for df in [-1, 1] {
            if let Some(to) = index(file + df, one_rank) {
                let can_capture = self.board[to as usize]
                    .map(|target| target.color != piece.color && target.kind != PieceKind::King)
                    .unwrap_or(false)
                    || self.en_passant == Some(to);
                if can_capture {
                    self.push_pawn_move(square, to, piece.color, moves);
                }
            }
        }
    }

    fn push_pawn_move(&self, from: u8, to: u8, color: Color, moves: &mut Vec<ChessMove>) {
        if is_promotion_rank(to, color) {
            for promotion in [PieceKind::Queen, PieceKind::Rook, PieceKind::Bishop, PieceKind::Knight] {
                moves.push(ChessMove::new(from, to, Some(promotion)));
            }
        } else {
            moves.push(ChessMove::new(from, to, None));
        }
    }

    fn add_knight_moves(&self, square: u8, piece: Piece, moves: &mut Vec<ChessMove>) {
        for (df, dr) in KNIGHT_DIRS {
            if let Some(to) = index(file_of(square) + df, rank_of(square) + dr) {
                self.push_if_free_or_enemy(square, to, piece.color, moves);
            }
        }
    }

    fn add_slider_moves(&self, square: u8, piece: Piece, moves: &mut Vec<ChessMove>, dirs: &[(i32, i32)]) {
        for &(df, dr) in dirs {
            let mut file = file_of(square) + df;
            let mut rank = rank_of(square) + dr;
            while let Some(to) = index(file, rank) {
                if let Some(target) = self.board[to as usize] {
                    if target.color != piece.color && target.kind != PieceKind::King {
                        moves.push(ChessMove::new(square, to, None));
                    }
                    break;
                }
                moves.push(ChessMove::new(square, to, None));
                file += df;
                rank += dr;
            }
        }
    }

    fn add_king_moves(&self, square: u8, piece: Piece, moves: &mut Vec<ChessMove>) {
        for (df, dr) in KING_DIRS {
            if let Some(to) = index(file_of(square) + df, rank_of(square) + dr) {
                self.push_if_free_or_enemy(square, to, piece.color, moves);
            }
        }
        self.add_castling_moves(piece.color, moves);
    }

    fn add_castling_moves(&self, color: Color, moves: &mut Vec<ChessMove>) {
        if self.is_in_check(color) {
            return;
        }
        match color {
            Color::White => {
                if self.castling.white_king_side
                    && self.can_castle(Color::White, 4, 7, &[5, 6], &[5, 6])
                {
                    moves.push(ChessMove::new(4, 6, None));
                }
                if self.castling.white_queen_side
                    && self.can_castle(Color::White, 4, 0, &[1, 2, 3], &[3, 2])
                {
                    moves.push(ChessMove::new(4, 2, None));
                }
            }
            Color::Black => {
                if self.castling.black_king_side
                    && self.can_castle(Color::Black, 60, 63, &[61, 62], &[61, 62])
                {
                    moves.push(ChessMove::new(60, 62, None));
                }
                if self.castling.black_queen_side
                    && self.can_castle(Color::Black, 60, 56, &[57, 58, 59], &[59, 58])
                {
                    moves.push(ChessMove::new(60, 58, None));
                }
            }
        }
    }

    fn can_castle(
        &self,
        color: Color,
        king_square: u8,
        rook_square: u8,
        empty_squares: &[u8],
        safe_squares: &[u8],
    ) -> bool {
        self.board[king_square as usize]
            == Some(Piece {
                color,
                kind: PieceKind::King,
            })
            && self.board[rook_square as usize]
                == Some(Piece {
                    color,
                    kind: PieceKind::Rook,
                })
            && empty_squares.iter().all(|square| self.board[*square as usize].is_none())
            && safe_squares
                .iter()
                .all(|square| !self.is_square_attacked(*square, color.opposite()))
    }

    fn push_if_free_or_enemy(&self, from: u8, to: u8, color: Color, moves: &mut Vec<ChessMove>) {
        if self.board[to as usize]
            .map(|target| target.color != color && target.kind != PieceKind::King)
            .unwrap_or(true)
        {
            moves.push(ChessMove::new(from, to, None));
        }
    }

    fn king_square(&self, color: Color) -> Option<u8> {
        self.board.iter().enumerate().find_map(|(square, piece)| {
            if *piece == Some(Piece { color, kind: PieceKind::King }) {
                Some(square as u8)
            } else {
                None
            }
        })
    }

    fn is_square_attacked(&self, square: u8, by_color: Color) -> bool {
        let file = file_of(square);
        let rank = rank_of(square);

        let pawn_source_rank = match by_color {
            Color::White => rank - 1,
            Color::Black => rank + 1,
        };
        for df in [-1, 1] {
            if let Some(from) = index(file + df, pawn_source_rank) {
                if self.board[from as usize]
                    == Some(Piece {
                        color: by_color,
                        kind: PieceKind::Pawn,
                    })
                {
                    return true;
                }
            }
        }

        for (df, dr) in KNIGHT_DIRS {
            if let Some(from) = index(file + df, rank + dr) {
                if self.board[from as usize]
                    == Some(Piece {
                        color: by_color,
                        kind: PieceKind::Knight,
                    })
                {
                    return true;
                }
            }
        }

        for (df, dr) in KING_DIRS {
            if let Some(from) = index(file + df, rank + dr) {
                if self.board[from as usize]
                    == Some(Piece {
                        color: by_color,
                        kind: PieceKind::King,
                    })
                {
                    return true;
                }
            }
        }

        self.attacked_by_slider(square, by_color, &BISHOP_DIRS, &[PieceKind::Bishop, PieceKind::Queen])
            || self.attacked_by_slider(square, by_color, &ROOK_DIRS, &[PieceKind::Rook, PieceKind::Queen])
    }

    fn attacked_by_slider(
        &self,
        square: u8,
        by_color: Color,
        dirs: &[(i32, i32)],
        attackers: &[PieceKind],
    ) -> bool {
        for &(df, dr) in dirs {
            let mut file = file_of(square) + df;
            let mut rank = rank_of(square) + dr;
            while let Some(from) = index(file, rank) {
                if let Some(piece) = self.board[from as usize] {
                    if piece.color == by_color && attackers.contains(&piece.kind) {
                        return true;
                    }
                    break;
                }
                file += df;
                rank += dr;
            }
        }
        false
    }
}

pub fn parse_square(value: &str) -> Option<u8> {
    let bytes = value.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let file = bytes[0];
    let rank = bytes[1];
    if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
        return None;
    }
    Some((rank - b'1') * 8 + (file - b'a'))
}

pub fn square_name(square: u8) -> String {
    let file = (b'a' + square % 8) as char;
    let rank = (b'1' + square / 8) as char;
    format!("{file}{rank}")
}

pub fn file_of(square: u8) -> i32 {
    (square % 8) as i32
}

pub fn rank_of(square: u8) -> i32 {
    (square / 8) as i32
}

pub fn index(file: i32, rank: i32) -> Option<u8> {
    if (0..8).contains(&file) && (0..8).contains(&rank) {
        Some((rank * 8 + file) as u8)
    } else {
        None
    }
}

fn is_promotion_rank(square: u8, color: Color) -> bool {
    match color {
        Color::White => rank_of(square) == 7,
        Color::Black => rank_of(square) == 0,
    }
}

fn file_distance(from: u8, to: u8) -> i32 {
    (file_of(from) - file_of(to)).abs()
}

fn rank_distance(from: u8, to: u8) -> i32 {
    (rank_of(from) - rank_of(to)).abs()
}

const KNIGHT_DIRS: [(i32, i32); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];
const KING_DIRS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
const BISHOP_DIRS: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const ROOK_DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const QUEEN_DIRS: [(i32, i32); 8] = [
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
];

impl fmt::Display for ChessMove {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_uci())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_perft_depth_1_and_2() {
        let position = Position::startpos();
        assert_eq!(position.perft(1), 20);
        assert_eq!(position.perft(2), 400);
    }

    #[test]
    fn fen_roundtrip_startpos() {
        let position = Position::from_fen(STARTPOS_FEN).unwrap();
        assert_eq!(position.to_fen(), STARTPOS_FEN);
    }

    #[test]
    fn legal_e2e4_sets_en_passant_square() {
        let mut position = Position::startpos();
        let chess_move = position.parse_uci_move("e2e4").unwrap();
        position.make_legal_move(chess_move).unwrap();
        assert_eq!(position.to_fen(), "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
    }
}
