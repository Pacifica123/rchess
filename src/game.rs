//! Основная логика игры


use std::collections::HashMap;
use std::hash::{Hash, Hasher};

//TODO: Game, Board, Move, Rules

    /*          --- ФИГУРА ---         */
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Color {
    White,
    Black
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceType {
    King,
    Queen,
    Rook,
    Bishop,
    Knight,
    Pawn
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub file: char, // a - h
    pub rank: u8
}

impl Hash for Position {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file.hash(state);
        self.rank.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub piece_type: PieceType,
    pub color: Color,
    pub pos: Option<Position>,  // None - фигура отсутствует на доске (срублена, фора, другое)
    pub first_move: bool        // Для пешек и рокировки 
}

impl Piece {
    pub fn new(ptype: PieceType, color: Color, pos: Option<Position>) -> Self{
        Self{
            piece_type: ptype, 
            color,
            pos,
            first_move:true,
        }
    }
}
    /*          ---  ДОСКА  ---          */
#[derive(Debug, Clone)]
pub struct Board {
    pieces: HashMap<Position, Piece>
}

impl Board {
    pub fn new() -> Self{
        Self{
            pieces: HashMap::new()
        }
    }
    /// получить фигуру по позиции (или ее отсутсвие)
    pub fn get_piece_at(&self, pos: &Position) -> Option<&Piece>{
        self.pieces.get(pos)
    }

    /// возвращает Some чтобы обработать рубку если на to стояла фигура
    pub fn move_piece(mut self, from: Position, to: Position) -> Option<Piece> {
        if let Some(mut piece) = self.pieces.remove(&from){
            piece.pos = Some(to); // <- Обновляем позицию фигуры

            //если на to была фигура, она сьедается автоматически
            return self.pieces.insert(to, piece);
        }
        None
    }

    /// Получить фигуру по типу и цвету
    pub fn find_piece(&self, piece_type: PieceType, color: Color) -> Option<Position> {
        for (pos, piece) in &self.pieces {
            if piece.piece_type == piece_type && piece.color == color {
                return Some(*pos);
            }
        }
        None
    }

    pub fn find_by_color(&self, color: Color) ->Vec<Piece> {
        let mut res = Vec::new();
        for (_, p) in &self.pieces {
            if p.color == color{
                res.push(*p);
            }
        }
        res
    }
}
