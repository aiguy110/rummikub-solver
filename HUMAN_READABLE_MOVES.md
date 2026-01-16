# HumanMove Translation System

## Problem Statement

The solver produces `Vec<SolverMove>` with:
- `PickUp(usize)` - pick up entire meld at index, add tiles to hand
- `LayDown(Meld)` - play a meld from combined hand

This "destroy and rebuild" approach doesn't match how humans think about Rummikub moves.

## Design Decisions

- **Meld references**: Include meld content directly (self-describing)
- **Move style**: Declarative (describe transformations, not execution order)
- **Fallback**: Try human moves, include raw diff as backup context

## HumanMove Variants

```rust
/// Declarative description of how a meld was transformed or created
pub enum HumanMove {
    /// Play a meld entirely from hand (no table tiles involved)
    PlayFromHand(Meld),

    /// Add tile(s) from hand to an existing meld
    /// Shows the original meld and the extended result
    ExtendMeld {
        original: Meld,
        added_tiles: Vec<Tile>,  // tiles from hand
        result: Meld,
    },

    /// Take tile(s) from a meld, leaving a valid meld behind
    TakeFromMeld {
        original: Meld,
        taken_tiles: Vec<Tile>,
        remaining: Meld,
    },

    /// Split one meld into multiple melds
    SplitMeld {
        original: Meld,
        parts: Vec<Meld>,
    },

    /// Combine multiple melds (or meld fragments) into one
    /// sources may include parts from splits
    JoinMelds {
        sources: Vec<Meld>,
        result: Meld,
    },

    /// Replace wild(s) in a meld with real tiles, taking the wilds
    SwapWild {
        original: Meld,
        swaps: Vec<(Tile, Tile)>,  // (replacement_from_hand, wild_taken)
        result: Meld,
    },

    /// Complex rearrangement that doesn't fit other patterns
    /// Fallback showing before/after for specific melds
    Rearrange {
        consumed: Vec<Meld>,      // original melds that were used
        produced: Vec<Meld>,      // new melds created
        hand_tiles_used: Vec<Tile>,
    },
}
```

### Variant Justifications

| Variant | Purpose |
|---------|---------|
| `PlayFromHand` | Player uses only their own tiles |
| `ExtendMeld` | Add tiles to run (front/back) or group (4th color) |
| `TakeFromMeld` | Remove tiles from meld (run ends or group with 4+ tiles) |
| `SplitMeld` | Split long run to reuse parts elsewhere |
| `JoinMelds` | Combine runs or run segments |
| `SwapWild` | Replace wild with real tile, take wild for other use |
| `Rearrange` | Fallback for complex multi-meld transformations |

## Translation Algorithm

### High-Level Approach

```
Input: original_table, original_hand, Vec<SolverMove>
Output: Vec<HumanMove>

1. Parse SolverMove sequence:
   - Extract picked_up_melds (original melds that were removed)
   - Extract laid_down_melds (new melds that were placed)

2. Build tile provenance:
   - For each tile in laid_down_melds, determine if it came from:
     - Hand (player's original tiles)
     - A specific table meld

3. Analyze each original meld's fate:
   - UNCHANGED: appears identically in output → no HumanMove needed
   - EXTENDED: appears with additional hand tiles → ExtendMeld
   - SPLIT: tiles appear in multiple output melds → SplitMeld
   - MERGED: tiles combined with other melds → JoinMelds
   - CONSUMED: tiles spread across multiple outputs → Rearrange

4. Analyze each new meld's origin:
   - PURE_HAND: all tiles from hand → PlayFromHand
   - FROM_ONE: all tiles from single original meld → part of Split
   - FROM_MANY: tiles from multiple sources → JoinMelds or Rearrange

5. Detect wild card movements:
   - If wild leaves meld A and real tile enters → SwapWild
```

### Core Data Structures

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TileSource {
    Hand,
    TableMeld(usize),  // index into original_table
}

struct TileAssignment {
    tile: Tile,
    source: TileSource,
    dest_meld_idx: usize,
    dest_position: usize,
}

struct MeldFate {
    original_idx: usize,
    original: Meld,
    // Where did each tile go?
    tile_destinations: Vec<Option<usize>>,  // index into new_melds
}

struct MeldOrigin {
    new_idx: usize,
    new_meld: Meld,
    // Where did each tile come from?
    tile_sources: Vec<TileSource>,
}
```

### Tile Provenance Assignment

The key challenge is determining which source tile maps to which destination tile, especially when there are duplicates.

```rust
fn assign_tile_provenance(
    picked_melds: &[(usize, Meld)],
    hand: &Hand,
    new_melds: &[Meld],
) -> Vec<TileAssignment> {
    // Build source pool: list of (Tile, TileSource)
    let mut source_pool: Vec<(Tile, TileSource)> = Vec::new();

    for (idx, meld) in picked_melds {
        for tile in &meld.tiles {
            source_pool.push((*tile, TileSource::TableMeld(*idx)));
        }
    }
    for (tile, &count) in hand.iter() {
        for _ in 0..count {
            source_pool.push((*tile, TileSource::Hand));
        }
    }

    // Greedy assignment: prefer table sources over hand sources
    // (preserves provenance for human-readable output)
    let mut assignments = Vec::new();
    let mut used = vec![false; source_pool.len()];

    for (meld_idx, meld) in new_melds.iter().enumerate() {
        for (pos, tile) in meld.tiles.iter().enumerate() {
            // First try to find matching table source
            let source_idx = source_pool.iter().enumerate()
                .position(|(i, (t, src))| {
                    !used[i] && *t == *tile && matches!(src, TileSource::TableMeld(_))
                })
                .or_else(|| {
                    // Fall back to hand source
                    source_pool.iter().enumerate()
                        .position(|(i, (t, _))| !used[i] && *t == *tile)
                });

            if let Some(i) = source_idx {
                used[i] = true;
                assignments.push(TileAssignment {
                    tile: *tile,
                    source: source_pool[i].1,
                    dest_meld_idx: meld_idx,
                    dest_position: pos,
                });
            }
        }
    }

    assignments
}
```

### Edge Cases

1. **Duplicate tiles**: Greedy assignment prefers table sources; ties broken arbitrarily
2. **Unchanged melds**: Detected when fate shows all tiles went to same new meld with same structure
3. **Complex rearrangements**: When one original's tiles spread to 3+ new melds, use Rearrange
4. **Wild movement**: Track separately; wild leaving + real tile entering same position = SwapWild

## Files to Modify

1. **`src/solver.rs`**
   - Add `HumanMove` enum
   - Add helper structs (`TileSource`, `TileAssignment`, `MeldFate`, `MeldOrigin`)
   - Add `translate_to_human_moves()` function
   - Add pattern detection helpers

2. **`src/lib.rs`**
   - Export `HumanMove` from `solver` module

3. **`src/wasm_api.rs`** (optional, for web UI)
   - Add `translate_moves_to_human()` WASM export
   - Return JSON representation of HumanMove list

## Testing Strategy

1. **Unit tests** (in `src/solver.rs`):
   - Pure hand play → `PlayFromHand`
   - Extend run at end → `ExtendMeld`
   - Extend group with 4th color → `ExtendMeld`
   - Split long run → `SplitMeld`
   - Join two runs → `JoinMelds`
   - Swap wild for real tile → `SwapWild`

2. **Integration tests**:
   - Complex scenario: pick up 2 melds, reorganize into 3 melds + play from hand
   - Verify all tiles accounted for

3. **Round-trip verification**:
   - Apply SolverMoves to get final state
   - Verify HumanMoves describe a valid transformation to same state

## Implementation Order

1. Define `HumanMove` enum and helper types
2. Implement `assign_tile_provenance()` (tile matching)
3. Implement pattern detectors one at a time:
   - `PlayFromHand` (simplest)
   - `ExtendMeld`
   - `SplitMeld`
   - `JoinMelds`
   - `SwapWild`
   - `Rearrange` (fallback)
4. Add tests for each pattern
5. Wire up WASM export
