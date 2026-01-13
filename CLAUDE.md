# Rummikub Solver - AI Context

## Project Overview

Rust library for Rummikub game solving. Currently implements core datatypes; algorithms are planned.

## Key Structures

- **Tile**: u8-packed tile (bits 0-1: color, bits 2-5: number, 0xFF: wild/joker)
- **MeldType**: Group (same number, different colors) or Run (consecutive numbers, same color)
- **Meld**: Collection of tiles forming a valid set (VecDeque)
- **Hand**: Player's tiles (BTreeMap for counting)
- **Table**: All melds currently played (Vec)

## Implementation Notes

- Tiles are efficiently bit-packed in a single byte
- Colors: 0=Red, 1=Blue, 2=Yellow, 3=Black
- Numbers: 1-13
- Wild tiles represented as 0xFF

## Future Work

Algorithms for move generation and optimization are planned but not yet implemented.

---

## Instructions for AI Agents

**Keep this file updated as you work on the project.** When you add new features, refactor code, or make significant changes, update the relevant sections in this file to reflect the current state of the codebase.

**Important**: This file must never exceed 250 lines. If adding new content would exceed this limit, consolidate or remove outdated information first.
