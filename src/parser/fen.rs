use crate::game::{Board, Color, CurrentGameStatus, Piece, PieceType, Position};

// fen.rs

#[derive(Debug)]
pub enum FenParseError {
    InvalidFormat,
    UnknownPiece(char),
    InvalidPosition,
    InvalidTurn,
    InvalidCastling,
    InvalidEnPassant,
}

impl From<std::num::ParseIntError> for FenParseError {
    fn from(_: std::num::ParseIntError) -> Self {
        FenParseError::InvalidFormat
    }
}

pub fn parse_fen(fen_str: &str) -> Result<CurrentGameStatus, FenParseError> {
    let parts: Vec<&str> = fen_str.split_whitespace().collect();
    if parts.len() < 4 {
        return Err(FenParseError::InvalidFormat);
    }

    let mut board = Board::new();
    parse_board_placement(&parts[0], &mut board)?;
    
    let current_turn = parse_turn(parts[1])?;
    let castling_rights = parse_castling(parts[2])?;
    let en_passant_target = parse_en_passant(parts[3])?;
    
    let halfmove_clock = parts.get(4).unwrap_or(&"0").parse::<usize>().unwrap_or(0);
    let fullmove_number = parts.get(5).unwrap_or(&"1").parse::<usize>().unwrap_or(1);

    Ok(CurrentGameStatus {
        board,
        current_turn,
        is_gameover: None,
        halfmove_clock,
        fullmove_number,
        castling_rights,
        en_passant_target,
        history_fens: vec![fen_str.to_string()],
    })
}

fn parse_board_placement(placement: &str, board: &mut Board) -> Result<(), FenParseError> {
    let ranks: Vec<&str> = placement.split('/').collect();
    if ranks.len() != 8 {
        return Err(FenParseError::InvalidFormat);
    }

    for (rank_idx, rank_str) in ranks.iter().enumerate() {
        let rank = 8u8 - rank_idx as u8;
        let mut file_idx = 0u8;

        for ch in rank_str.chars() {
            if ch.is_ascii_digit() {
                file_idx += ch.to_digit(10).unwrap() as u8;
            } else {
                if file_idx >= 8 {
                    return Err(FenParseError::InvalidPosition);
                }
                let pos = Position {
                    file: std::char::from_u32(b'a' as u32 + file_idx as u32).unwrap(),
                    rank,
                };
                let piece = parse_piece_char(ch)?;
                board.pieces.insert(pos, piece);
                file_idx += 1;
            }
        }
        if file_idx != 8 {
            return Err(FenParseError::InvalidPosition);
        }
    }
    Ok(())
}

fn parse_piece_char(ch: char) -> Result<Piece, FenParseError> {
    let (piece_type, color) = match ch.to_ascii_uppercase() {
        'K' => (PieceType::King, if ch.is_ascii_uppercase() { Color::White } else { Color::Black }),
        'Q' => (PieceType::Queen, if ch.is_ascii_uppercase() { Color::White } else { Color::Black }),
        'R' => (PieceType::Rook, if ch.is_ascii_uppercase() { Color::White } else { Color::Black }),
        'B' => (PieceType::Bishop, if ch.is_ascii_uppercase() { Color::White } else { Color::Black }),
        'N' => (PieceType::Knight, if ch.is_ascii_uppercase() { Color::White } else { Color::Black }),
        'P' => (PieceType::Pawn, if ch.is_ascii_uppercase() { Color::White } else { Color::Black }),
        _ => return Err(FenParseError::UnknownPiece(ch)),
    };
    Ok(Piece::new(piece_type, color, Some(Position { file: 'a', rank: 1 }))) // pos обновится при insert
}

fn parse_turn(turn: &str) -> Result<Color, FenParseError> {
    match turn {
        "w" => Ok(Color::White),
        "b" => Ok(Color::Black),
        _ => Err(FenParseError::InvalidTurn),
    }
}

fn parse_castling(castling: &str) -> Result<(bool, bool, bool, bool), FenParseError> {
    let mut wk = false; let mut wq = false; let mut bk = false; let mut bq = false;
    for ch in castling.chars() {
        match ch {
            'K' => wk = true,
            'Q' => wq = true,
            'k' => bk = true,
            'q' => bq = true,
            '-' => break,
            _ => return Err(FenParseError::InvalidCastling),
        }
    }
    Ok((wk, wq, bk, bq))
}

fn parse_en_passant(ep: &str) -> Result<Option<Position>, FenParseError> {
    match ep {
        "-" => Ok(None),
        ep if ep.len() == 2 => {
            let file = ep.chars().next().unwrap();
            let rank = ep.chars().nth(1).unwrap().to_digit(10).unwrap();
            Ok(Some(Position { file, rank: rank as u8 }))
        },
        _ => Err(FenParseError::InvalidEnPassant),
    }
}
