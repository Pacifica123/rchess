//! parsers.rs | Парсинг и сериализация FEN, PGN и возможно в будущем других форматов
use crate::game::{
    Board, Color, PieceType, Position, CurrentGameStatus, Move, MoveType, MoveHistory
};
use std::collections::HashMap;

use crate::parser::fen;
use crate::parser::pgn;

