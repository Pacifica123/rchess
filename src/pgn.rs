use std::collections::BTreeMap;

use crate::chess::{file_of, rank_of, square_name, ChessMove, Color, PieceKind, Position, STARTPOS_FEN};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PgnGame {
    pub tags: BTreeMap<String, String>,
    pub start_fen: String,
    pub moves: Vec<ChessMove>,
    pub result: String,
}

pub fn parse_pgn(text: &str) -> Result<PgnGame, String> {
    let tags = parse_tags(text);
    let start_fen = tags
        .get("FEN")
        .cloned()
        .unwrap_or_else(|| STARTPOS_FEN.to_string());
    let mut position = Position::from_fen(&start_fen)?;
    let mut moves = Vec::new();
    let mut result = tags.get("Result").cloned().unwrap_or_else(|| "*".to_string());

    let body = strip_tag_lines(text);
    let clean_body = strip_comments_and_variations(&body);
    for raw_token in clean_body.split_whitespace() {
        let Some(token) = clean_move_token(raw_token) else {
            continue;
        };
        if is_result_token(&token) {
            result = token;
            break;
        }
        if token.starts_with('$') {
            continue;
        }

        let chess_move = parse_move_token(&position, &token)?;
        position.make_legal_move(chess_move)?;
        moves.push(chess_move);
    }

    Ok(PgnGame {
        tags,
        start_fen,
        moves,
        result,
    })
}

pub fn export_pgn(start_fen: &str, moves: &[ChessMove], result: &str) -> Result<String, String> {
    let mut tags = BTreeMap::new();
    tags.insert("Event".to_string(), "rchess game".to_string());
    tags.insert("Site".to_string(), "?".to_string());
    tags.insert("Date".to_string(), "????.??.??".to_string());
    tags.insert("Round".to_string(), "?".to_string());
    tags.insert("White".to_string(), "White".to_string());
    tags.insert("Black".to_string(), "Black".to_string());
    tags.insert("Result".to_string(), result.to_string());
    if start_fen.trim() != STARTPOS_FEN {
        tags.insert("SetUp".to_string(), "1".to_string());
        tags.insert("FEN".to_string(), start_fen.trim().to_string());
    }
    export_pgn_with_tags(start_fen, moves, result, &tags)
}

pub fn export_pgn_with_tags(
    start_fen: &str,
    moves: &[ChessMove],
    result: &str,
    tags: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut output = String::new();
    write_tag(&mut output, tags, "Event");
    write_tag(&mut output, tags, "Site");
    write_tag(&mut output, tags, "Date");
    write_tag(&mut output, tags, "Round");
    write_tag(&mut output, tags, "White");
    write_tag(&mut output, tags, "Black");
    write_tag(&mut output, tags, "Result");
    if start_fen.trim() != STARTPOS_FEN {
        write_tag(&mut output, tags, "SetUp");
        write_tag(&mut output, tags, "FEN");
    }
    for (name, value) in tags {
        if matches!(
            name.as_str(),
            "Event" | "Site" | "Date" | "Round" | "White" | "Black" | "Result" | "SetUp" | "FEN"
        ) {
            continue;
        }
        output.push_str(&format!("[{name} \"{}\"]\n", escape_tag_value(value)));
    }
    output.push('\n');

    let body = export_move_text(start_fen, moves, result)?;
    output.push_str(&wrap_pgn_body(&body, 88));
    output.push('\n');
    Ok(output)
}

pub fn export_move_text(start_fen: &str, moves: &[ChessMove], result: &str) -> Result<String, String> {
    let mut position = Position::from_fen(start_fen.trim())?;
    let mut move_number = start_fullmove_number(start_fen);
    let mut tokens = Vec::new();

    for chess_move in moves {
        let side = position.side_to_move();
        if side == Color::White {
            tokens.push(format!("{move_number}."));
        } else if tokens.is_empty() {
            tokens.push(format!("{move_number}..."));
        }

        let san = move_to_san(&position, *chess_move)?;
        position.make_legal_move(*chess_move)?;
        tokens.push(san);

        if side == Color::Black {
            move_number += 1;
        }
    }

    tokens.push(result.to_string());
    Ok(tokens.join(" "))
}

pub fn position_after_moves(start_fen: &str, moves: &[ChessMove]) -> Result<Position, String> {
    let mut position = Position::from_fen(start_fen.trim())?;
    for chess_move in moves {
        position.make_legal_move(*chess_move)?;
    }
    Ok(position)
}

pub fn moves_to_san(start_fen: &str, moves: &[ChessMove]) -> Result<Vec<String>, String> {
    let mut position = Position::from_fen(start_fen.trim())?;
    let mut result = Vec::with_capacity(moves.len());
    for chess_move in moves {
        let san = move_to_san(&position, *chess_move)?;
        position.make_legal_move(*chess_move)?;
        result.push(san);
    }
    Ok(result)
}

pub fn move_to_san(position: &Position, chess_move: ChessMove) -> Result<String, String> {
    let piece = position
        .piece_at(chess_move.from)
        .ok_or_else(|| format!("no piece on {}", square_name(chess_move.from)))?;

    let is_castling = piece.kind == PieceKind::King && (file_of(chess_move.from) - file_of(chess_move.to)).abs() == 2;
    let mut san = if is_castling {
        if file_of(chess_move.to) == 6 {
            "O-O".to_string()
        } else {
            "O-O-O".to_string()
        }
    } else {
        let mut value = String::new();
        if let Some(letter) = san_piece_letter(piece.kind) {
            value.push(letter);
            value.push_str(&disambiguation(position, chess_move, piece.kind));
        } else if position.is_capture(chess_move) {
            value.push(file_char(chess_move.from));
        }

        if position.is_capture(chess_move) {
            value.push('x');
        }
        value.push_str(&square_name(chess_move.to));
        if let Some(promotion) = chess_move.promotion {
            value.push('=');
            value.push(san_piece_letter(promotion).ok_or_else(|| "bad promotion piece".to_string())?);
        }
        value
    };

    let mut next = position.clone();
    next.make_legal_move(chess_move)?;
    if next.is_checkmate() {
        san.push('#');
    } else if next.is_in_check(next.side_to_move()) {
        san.push('+');
    }
    Ok(san)
}

fn parse_move_token(position: &Position, token: &str) -> Result<ChessMove, String> {
    if let Some(chess_move) = position.parse_uci_move(token) {
        return Ok(chess_move);
    }

    let normalized = normalize_san(token);
    let mut matches = Vec::new();
    for chess_move in position.legal_moves() {
        let san = move_to_san(position, chess_move)?;
        if normalize_san(&san) == normalized {
            matches.push(chess_move);
        }
    }

    match matches.len() {
        1 => Ok(matches[0]),
        0 => Err(format!("cannot parse PGN move `{token}` in position {}", position.to_fen())),
        _ => Err(format!("ambiguous PGN move `{token}` in position {}", position.to_fen())),
    }
}

fn disambiguation(position: &Position, chess_move: ChessMove, kind: PieceKind) -> String {
    let competitors: Vec<ChessMove> = position
        .legal_moves()
        .into_iter()
        .filter(|candidate| {
            candidate.from != chess_move.from
                && candidate.to == chess_move.to
                && position
                    .piece_at(candidate.from)
                    .map(|piece| piece.kind == kind && piece.color == position.side_to_move())
                    .unwrap_or(false)
        })
        .collect();

    if competitors.is_empty() {
        return String::new();
    }

    let same_file = competitors
        .iter()
        .any(|candidate| file_of(candidate.from) == file_of(chess_move.from));
    let same_rank = competitors
        .iter()
        .any(|candidate| rank_of(candidate.from) == rank_of(chess_move.from));

    if !same_file {
        file_char(chess_move.from).to_string()
    } else if !same_rank {
        rank_char(chess_move.from).to_string()
    } else {
        format!("{}{}", file_char(chess_move.from), rank_char(chess_move.from))
    }
}

fn san_piece_letter(kind: PieceKind) -> Option<char> {
    match kind {
        PieceKind::King => Some('K'),
        PieceKind::Queen => Some('Q'),
        PieceKind::Rook => Some('R'),
        PieceKind::Bishop => Some('B'),
        PieceKind::Knight => Some('N'),
        PieceKind::Pawn => None,
    }
}

fn file_char(square: u8) -> char {
    (b'a' + square % 8) as char
}

fn rank_char(square: u8) -> char {
    (b'1' + square / 8) as char
}

fn parse_tags(text: &str) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('[') || !line.ends_with(']') {
            continue;
        }
        let inner = &line[1..line.len() - 1];
        let mut parts = inner.splitn(2, char::is_whitespace);
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(rest) = parts.next() else {
            continue;
        };
        let rest = rest.trim();
        if !rest.starts_with('"') || !rest.ends_with('"') || rest.len() < 2 {
            continue;
        }
        let value = unescape_tag_value(&rest[1..rest.len() - 1]);
        tags.insert(name.to_string(), value);
    }
    tags
}

fn strip_tag_lines(text: &str) -> String {
    let mut body = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

fn strip_comments_and_variations(text: &str) -> String {
    let mut output = String::new();
    let mut in_brace_comment = false;
    let mut in_semicolon_comment = false;
    let mut variation_depth = 0_u32;

    for ch in text.chars() {
        if in_semicolon_comment {
            if ch == '\n' {
                in_semicolon_comment = false;
                output.push(' ');
            }
            continue;
        }
        if in_brace_comment {
            if ch == '}' {
                in_brace_comment = false;
                output.push(' ');
            }
            continue;
        }
        if variation_depth > 0 {
            match ch {
                '(' => variation_depth += 1,
                ')' => variation_depth -= 1,
                _ => {}
            }
            continue;
        }

        match ch {
            '{' => in_brace_comment = true,
            ';' => in_semicolon_comment = true,
            '(' => variation_depth = 1,
            _ => output.push(ch),
        }
    }
    output
}

fn clean_move_token(raw: &str) -> Option<String> {
    let mut token = raw.trim().trim_matches(|ch: char| ch == '\u{feff}').to_string();
    if token.is_empty() || token.starts_with('$') {
        return None;
    }
    if is_result_token(&token) {
        return Some(token);
    }

    loop {
        let digit_count = token.chars().take_while(|ch| ch.is_ascii_digit()).count();
        if digit_count == 0 {
            break;
        }
        let rest = &token[digit_count..];
        let dot_count = rest.chars().take_while(|ch| *ch == '.').count();
        if dot_count == 0 {
            break;
        }
        token = rest[dot_count..].to_string();
    }

    while token.starts_with('.') {
        token.remove(0);
    }
    if token.is_empty() || token == "*" {
        return if token == "*" { Some(token) } else { None };
    }
    Some(token)
}

fn normalize_san(token: &str) -> String {
    let mut value = token.trim().replace('0', "O");
    if let Some(stripped) = value.strip_suffix("e.p.") {
        value = stripped.trim_end().to_string();
    }
    while value.ends_with('+')
        || value.ends_with('#')
        || value.ends_with('!')
        || value.ends_with('?')
    {
        value.pop();
    }
    value
}

fn is_result_token(token: &str) -> bool {
    matches!(token, "1-0" | "0-1" | "1/2-1/2" | "*")
}

fn start_fullmove_number(fen: &str) -> u32 {
    fen.split_whitespace()
        .nth(5)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
}

fn write_tag(output: &mut String, tags: &BTreeMap<String, String>, name: &str) {
    if let Some(value) = tags.get(name) {
        output.push_str(&format!("[{name} \"{}\"]\n", escape_tag_value(value)));
    }
}

fn escape_tag_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unescape_tag_value(value: &str) -> String {
    let mut result = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            result.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            result.push(ch);
        }
    }
    if escaped {
        result.push('\\');
    }
    result
}

fn wrap_pgn_body(body: &str, width: usize) -> String {
    let mut output = String::new();
    let mut line_len = 0_usize;
    for token in body.split_whitespace() {
        let token_len = token.len();
        if line_len > 0 && line_len + 1 + token_len > width {
            output.push('\n');
            line_len = 0;
        }
        if line_len > 0 {
            output.push(' ');
            line_len += 1;
        }
        output.push_str(token);
        line_len += token_len;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scholars_mate_san() {
        let pgn = "1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0";
        let game = parse_pgn(pgn).unwrap();
        let position = position_after_moves(&game.start_fen, &game.moves).unwrap();
        assert_eq!(game.result, "1-0");
        assert!(position.is_checkmate());
    }

    #[test]
    fn exports_basic_san() {
        let mut position = Position::startpos();
        let mut moves = Vec::new();
        for token in ["e2e4", "e7e5", "g1f3"] {
            let chess_move = position.parse_uci_move(token).unwrap();
            position.make_legal_move(chess_move).unwrap();
            moves.push(chess_move);
        }
        let text = export_move_text(STARTPOS_FEN, &moves, "*").unwrap();
        assert_eq!(text, "1. e4 e5 2. Nf3 *");
    }

    #[test]
    fn parses_fen_tag_and_black_to_move() {
        let pgn = "[SetUp \"1\"]\n[FEN \"4k3/8/8/8/8/8/8/R3K2R b KQ - 0 1\"]\n\n1... Kf7 *";
        let game = parse_pgn(pgn).unwrap();
        assert_eq!(game.moves.len(), 1);
        let position = position_after_moves(&game.start_fen, &game.moves).unwrap();
        assert_eq!(position.side_to_move(), Color::White);
    }


    #[test]
    fn parses_and_exports_knight_file_disambiguation() {
        let start_fen = "4k3/8/8/8/8/8/8/1N2KN2 w - - 0 1";
        let mut position = Position::from_fen(start_fen).unwrap();
        let chess_move = position.parse_uci_move("b1d2").unwrap();
        assert_eq!(move_to_san(&position, chess_move).unwrap(), "Nbd2");
        position.make_legal_move(chess_move).unwrap();

        let pgn = format!("[SetUp \"1\"]\n[FEN \"{start_fen}\"]\n\n1. Nbd2 *");
        let game = parse_pgn(&pgn).unwrap();
        assert_eq!(game.moves, vec![chess_move]);
    }

    #[test]
    fn parses_and_exports_rook_rank_disambiguation() {
        let start_fen = "4k3/8/8/8/8/R7/8/R3K3 w - - 0 1";
        let position = Position::from_fen(start_fen).unwrap();
        let chess_move = position.parse_uci_move("a1a2").unwrap();
        assert_eq!(move_to_san(&position, chess_move).unwrap(), "R1a2");

        let pgn = format!("[SetUp \"1\"]\n[FEN \"{start_fen}\"]\n\n1. R1a2 *");
        let game = parse_pgn(&pgn).unwrap();
        assert_eq!(game.moves, vec![chess_move]);
    }

    #[test]
    fn rejects_ambiguous_san_without_required_disambiguation() {
        let start_fen = "4k3/8/8/8/8/8/8/1N2KN2 w - - 0 1";
        let pgn = format!("[SetUp \"1\"]\n[FEN \"{start_fen}\"]\n\n1. Nd2 *");
        let error = parse_pgn(&pgn).unwrap_err();
        assert!(error.contains("cannot parse PGN move") || error.contains("ambiguous PGN move"));
    }

    #[test]
    fn moves_to_san_returns_plain_san_list() {
        let game = parse_pgn("1. e4 e5 2. Nf3 Nc6 *").unwrap();
        let san = moves_to_san(&game.start_fen, &game.moves).unwrap();
        assert_eq!(san, vec!["e4", "e5", "Nf3", "Nc6"]);
    }
}
