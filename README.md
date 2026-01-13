# Rummikub Solver

A Rust library providing datatypes and algorithms for determining optimal moves in Rummikub.

## Features

- **Efficient tile representation**: Tiles packed into u8 (color + number) with wild/joker support
- **Core datatypes**: `Tile`, `Meld`, `Hand`, `Table`
- **Meld types**: Groups (same number, different colors) and Runs (consecutive numbers, same color)

## Usage

```rust
use rummikub_solver::*;

// Create tiles
let tile = Tile::new(0, 5); // Red 5
let wild = Tile::wild();

// Build a hand
let mut hand = Hand::new();
hand.add(tile);

// Create melds
let meld = Meld::new(MeldType::Run, tiles);
```

## License

See LICENSE file.
