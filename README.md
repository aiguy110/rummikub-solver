# Rummikub Solver

A Rust library and web interface for determining optimal moves in Rummikub.

**[Try it live on GitHub Pages!](https://aiguy110.github.io/rummikub-solver/)**

## Web Interface

The project includes a mobile-friendly web interface built with vanilla JavaScript and WebAssembly. Features include:

- Visual tile picker for building your hand
- Table state management for existing melds
- Real-time solver with configurable strategies (minimize tiles or minimize points)
- Save/load game states using localStorage
- Fully client-side - no backend required

### Local Development

1. Build the WASM module:
   ```bash
   cargo install wasm-pack
   wasm-pack build --target web
   ```

2. Serve the files locally:
   ```bash
   python3 -m http.server
   ```

3. Open `http://localhost:8000` in your browser

### Deployment

The project automatically deploys to GitHub Pages via GitHub Actions on push to `main`. To set up:

1. Go to repository Settings → Pages
2. Set Source to "GitHub Actions"
3. Push changes to `main` branch
4. The site will be available at `https://YOUR_USERNAME.github.io/rummikub-solver/`

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
