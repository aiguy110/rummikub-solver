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

## Solver Algorithm

### `find_best_melds` Function

Core algorithm for finding optimal meld combinations from a hand. Located in `src/solver.rs`.

**Signature:**
```rust
fn find_best_melds<F>(
    hand: &mut Hand,
    quality: F,
    hand_to_beat: &Hand,
) -> Option<Vec<Meld>>
where F: Fn(&Hand) -> i32
```

**Parameters:**
- `hand`: The player's current tiles
- `quality`: Closure scoring terminal hands (higher = better)
- `hand_to_beat`: Baseline hand to beat (must reduce at least one tile type count)

**Algorithm:**
1. **Meld Generation**: Pre-generate all possible melds (runs/groups) including wildcard substitutions
2. **Indexing**: Build tile → meld indices for fast conflict detection
3. **Backtracking**: Explore meld combinations in canonical order
4. **Pruning**: Track invalid melds as tiles are consumed
5. **Evaluation**: Score terminal hands that "beat" the baseline

**Wildcard Handling:**
- Wildcards (0xFF) treated as another tile type for counting
- Substitution logic in meld generation: enumerate all valid wildcard placements
- E.g., run [R1,R2,R3] generates variants with wildcards at each position if available

**"Beats" Predicate:**
- Terminal hand must not contain tile types absent from baseline
- Must have strictly fewer tiles than baseline for at least one type

**Invariant:**
- The `hand` parameter is **always restored** to its original state after the function returns, regardless of whether a solution is found or not
- This is achieved by cloning the hand at the start and restoring it before returning

### `SolverMove` Enum

Represents a single move in the game. Located in `src/solver.rs`.

**Variants:**
- `PickUp(usize)`: Pick up a meld from the table at the given index
- `LayDown(Meld)`: Play a meld from the player's hand onto the table

### `find_best_moves` Function

High-level solver that finds a sequence of moves to play tiles, potentially manipulating the table. Located in `src/solver.rs`.

**Signature:**
```rust
pub fn find_best_moves(
    table: &mut Table,
    hand: &mut Hand,
    max_ms: u64,
) -> Option<Vec<SolverMove>>
```

**Parameters:**
- `table`: The current table state (melds on the table)
- `hand`: The player's current tiles
- `max_ms`: Time limit in milliseconds

**Algorithm:**
1. **Direct Play**: Try to play from current hand using `find_best_melds`
2. **BFS Search**: Explore removing melds from table (depth 1, then 2, etc., max 5)
3. **For Each Combination**: Remove melds → add tiles to hand → call `find_best_melds`
4. **Time Limit**: Stop if `max_ms` exceeded

**Quality Metric:** Negative of total tile count (fewer tiles remaining = better)

**Invariants:**
- Both `table` and `hand` are **always restored** to original state after the function returns
- Move sequences are valid: PickUp moves occur before LayDown moves

---

## Termux Environment Notes

This project is developed in a Termux environment. Cargo commands generally work but may encounter permission issues.

**Observed Issue:**
The first invocation of cargo commands may fail with `EACCES: permission denied, mkdir '/tmp/claude/.../tasks'`. This is a Claude Code internal error (related to task tracking), not a cargo issue.

**Workarounds:**
1. Retry the command - subsequent invocations usually work
2. Use `cargo build 2>&1` first, then run tests
3. Pipe output: `cargo test 2>&1 | head -100`

The `2>&1` redirect doesn't fix the permission issue itself, but piping output (e.g., `| head`) may change execution context and avoid the error.

---

## Instructions for AI Agents

**Keep this file updated as you work on the project.** When you add new features, refactor code, or make significant changes, update the relevant sections in this file to reflect the current state of the codebase.

**Important**: This file must never exceed 250 lines. If adding new content would exceed this limit, consolidate or remove outdated information first.
