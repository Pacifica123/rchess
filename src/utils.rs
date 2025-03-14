use crate::game::*;

pub fn print_move(move_: &Move) {
    let piece_str = match move_.piece.piece_type {
        PieceType::King => "K",
        PieceType::Queen => "Q",
        PieceType::Rook => "R",
        PieceType::Bishop => "B",
        PieceType::Knight => "N",
        PieceType::Pawn => "",
    };

    let capture_str = if move_.move_type == MoveType::Capture {
        "x"
    } else {
        ""
    };

    let move_type_str = match move_.move_type {
        MoveType::EnPassant => "e.p.".to_string(),
        MoveType::Castling => {
            if move_.new_position.file == 'g' {
                "O-O".to_string()
            } else {
                "O-O-O".to_string()
            }
        }
        MoveType::Promotion => {
            let promotion_piece = match move_.piece.piece_type {
                PieceType::Pawn => "Q", // По умолчанию превращаем в ферзя
                _ => unreachable!(),
            };
            format!("{}=", promotion_piece)
        }
        _ => "".to_string(),
    };

    let position_str = format!("{}{}", move_.new_position.file, move_.new_position.rank);

    if move_.move_type == MoveType::Castling {
        println!("{}", move_type_str);
    } else if move_.move_type == MoveType::Promotion {
        println!("{}{}{}", piece_str, capture_str, position_str);
    } else {
        println!("{}{}{}", piece_str, capture_str, position_str);
    }

    if !move_type_str.is_empty() && move_.move_type != MoveType::Castling {
        println!("{}", move_type_str);
    }
}
