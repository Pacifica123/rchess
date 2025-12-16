# ♟️ Chess Engine in Rust 🦀

This project is a chess engine implemented in Rust. It provides the core logic for playing chess, including board representation, move generation, move validation, and an AI engine for playing against the computer. The engine is designed to be modular and extensible, allowing for future improvements and enhancements.

##  Key Features

- **Board Representation:** Uses a `HashMap` to efficiently represent the chessboard and piece positions.
- **Move Generation:** Generates all legal moves for a given player using the chess rules.
- **Move Validation:** Validates the legality of moves based on chess rules for each piece type.
- **AI Engine:** Includes an AI engine that can make moves (currently a placeholder with random move selection, but designed for future implementation of Minimax or Alpha-Beta pruning).
- **PGN Parsing:** Supports parsing and serializing chess games in Portable Game Notation (PGN) format.
- **Game State Management:** Manages the overall game state, including move history and game status.
- **Transposition Table:** Caches evaluation results to improve search efficiency.

##  Tech Stack

*   **Language:** Rust 
*   **Data Structures:** `HashMap` (for board representation and transposition table)
*   **AI:** (Placeholder for Minimax/Alpha-Beta pruning)
*   **Random Number Generation:** `rand` crate
*   **Game Logic:** Custom modules for game rules, board representation, and move generation.
*   **PGN Parsing:** Custom implementation (likely using Rust's standard library string manipulation)

##  Getting Started

These instructions will get you a copy of the project up and running on your local machine for development and testing purposes.

### Prerequisites

*   Rust toolchain installed (`rustc`, `cargo`) - [https://www.rust-lang.org/tools/install](https://www.rust-lang.org/tools/install)

### Installation

1.  Clone the repository:

    ```bash
    git clone <repository_url>
    cd chess-engine-rust
    ```

2.  Build the project:

    ```bash
    cargo build
    ```

### Running Locally

1.  Run the main application:

    ```bash
    cargo run
    ```

    This will start the chess game in PC vs PC mode and run it for a fixed number of moves, printing the board state after each move.

##  Usage

The `main.rs` file sets up the game and runs the game loop. You can modify the `main.rs` file to change the game mode, enable/disable modules, or customize the game settings.

```rust
// Example from src/main.rs
fn main() {
    let mut b = Board::init_by_default();
    let mut g = Game::new(b);
    g.gamemode = Gamemode::PCvsPC;
    g.start();

    let mut eng = Engine::new();

    for _i in 0..TOTAL_MOVES {
        eng.make_move(&mut g);
        g.board.display();
    }
}
```

##  Project Structure

```
chess-engine-rust/
├── src/
│   ├── main.rs         # Entry point of the application
│   ├── engine.rs       # AI engine implementation
│   ├── game.rs         # Core game logic and data structures
│   ├── rules.rs        # Chess rules implementation
│   └── parser.rs       # PGN parsing and serialization
├── Cargo.toml      # Project dependencies and metadata
├── Cargo.lock      # Dependency lock file
└── README.md       # This file
```
