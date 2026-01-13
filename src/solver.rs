use crate::{Hand, Meld, MeldType, Table, Tile};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Represents a solver move in the Rummikub game
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverMove {
    /// Pick up a meld from the table at the given index and add it to the player's hand
    PickUp(usize),
    /// Play a meld from the player's hand onto the table
    LayDown(Meld),
}

/// Scoring strategy for evaluating the quality of a hand
#[derive(Debug, Clone, Copy)]
pub enum ScoringStrategy {
    /// Minimize the count of remaining tiles
    MinimizeTiles,
    /// Minimize the point value of remaining tiles (sum of numbers)
    MinimizePoints,
}

impl ScoringStrategy {
    fn evaluate(&self, hand: &Hand) -> i32 {
        match self {
            Self::MinimizeTiles => {
                let total: i32 = hand.iter().map(|(_, &c)| c as i32).sum();
                -total
            }
            Self::MinimizePoints => {
                let points: i32 = hand.iter()
                    .map(|(tile, &count)| {
                        let value = tile.number().unwrap_or(0) as i32;
                        value * count as i32
                    })
                    .sum();
                -points
            }
        }
    }
}

/// Find the best sequence of moves to play tiles from hand, potentially manipulating the table.
///
/// This function uses a BFS approach:
/// 1. First tries to play directly from the current hand
/// 2. Then explores removing melds from the table (starting with 1, then 2, etc.)
/// 3. For each configuration, attempts to find valid melds to play
///
/// Returns the sequence of moves if a solution is found within the time limit.
/// Uses MinimizeTiles strategy by default.
pub fn find_best_moves(
    table: &mut Table,
    hand: &mut Hand,
    max_ms: u64,
) -> Option<Vec<SolverMove>> {
    find_best_moves_with_strategy(table, hand, max_ms, ScoringStrategy::MinimizeTiles)
}

/// Find the best sequence of moves using a specific scoring strategy.
///
/// This function uses a BFS approach:
/// 1. First tries to play directly from the current hand
/// 2. Then explores removing melds from the table (starting with 1, then 2, etc.)
/// 3. For each configuration, attempts to find valid melds to play
///
/// Returns the sequence of moves if a solution is found within the time limit.
pub fn find_best_moves_with_strategy(
    table: &mut Table,
    hand: &mut Hand,
    max_ms: u64,
    strategy: ScoringStrategy,
) -> Option<Vec<SolverMove>> {
    let quality = |h: &Hand| strategy.evaluate(h);
    find_best_moves_internal(table, hand, max_ms, quality)
}

/// Internal implementation of find_best_moves that accepts a custom quality function.
fn find_best_moves_internal<F>(
    table: &mut Table,
    hand: &mut Hand,
    max_ms: u64,
    quality: F,
) -> Option<Vec<SolverMove>>
where
    F: Fn(&Hand) -> i32 + Copy,
{
    let start_time = Instant::now();
    let time_limit = Duration::from_millis(max_ms);
    let original_hand = hand.clone();
    let original_table = table.clone();

    // Strategy 1: Try to play directly from current hand
    if let Some(melds) = find_best_melds(hand, quality, &original_hand) {
        let moves: Vec<SolverMove> = melds
            .into_iter()
            .map(|meld| SolverMove::LayDown(meld))
            .collect();
        // Restore state
        *hand = original_hand;
        *table = original_table;
        return Some(moves);
    }

    // Strategy 2: BFS exploring table manipulations
    // Try removing 1 meld, then 2, then 3, etc.
    let max_depth = table.len().min(5); // Limit depth to avoid explosion

    for depth in 1..=max_depth {
        // Check time limit
        if start_time.elapsed() >= time_limit {
            *hand = original_hand;
            *table = original_table;
            return None;
        }

        // Try all combinations of removing 'depth' melds from the table
        if let Some(moves) = try_remove_combinations(
            table,
            hand,
            &original_hand,
            depth,
            quality,
            start_time,
            time_limit,
        ) {
            // Restore state
            *hand = original_hand;
            *table = original_table;
            return Some(moves);
        }
    }

    // Restore state if no solution found
    *hand = original_hand;
    *table = original_table;
    None
}

/// Try all combinations of removing 'count' melds from the table
fn try_remove_combinations<F>(
    table: &mut Table,
    hand: &mut Hand,
    original_hand: &Hand,
    count: usize,
    quality: F,
    start_time: Instant,
    time_limit: Duration,
) -> Option<Vec<SolverMove>>
where
    F: Fn(&Hand) -> i32 + Copy,
{
    let table_size = table.len();
    if count > table_size {
        return None;
    }

    // Generate all combinations of indices to remove
    let mut indices = vec![0; count];
    if !generate_combination(&mut indices, 0, 0, table_size, count) {
        return None;
    }

    loop {
        // Check time limit
        if start_time.elapsed() >= time_limit {
            return None;
        }

        // Try this combination
        if let Some(moves) = try_meld_combination(table, hand, original_hand, &indices, quality) {
            return Some(moves);
        }

        // Generate next combination
        if !next_combination(&mut indices, table_size) {
            break;
        }
    }

    None
}

/// Try removing the melds at the given indices and finding a solution
fn try_meld_combination<F>(
    table: &mut Table,
    hand: &mut Hand,
    original_hand: &Hand,
    indices: &[usize],
    quality: F,
) -> Option<Vec<SolverMove>>
where
    F: Fn(&Hand) -> i32 + Copy,
{
    let table_snapshot = table.clone();
    let hand_snapshot = hand.clone();

    // Remove melds in reverse order to maintain indices
    let mut removed_melds = Vec::new();
    for &idx in indices.iter().rev() {
        if let Some(meld) = table.remove_meld(idx) {
            // Add tiles to hand
            for tile in &meld.tiles {
                hand.add(*tile);
            }
            removed_melds.push((idx, meld));
        }
    }

    // Try to find melds from the new hand
    if let Some(melds) = find_best_melds(hand, quality, original_hand) {
        // Build the move sequence
        let mut moves = Vec::new();

        // First, pick up the melds (in the order we removed them, which is reversed)
        for (idx, _) in removed_melds.iter().rev() {
            moves.push(SolverMove::PickUp(*idx));
        }

        // Then, lay down the new melds
        for meld in melds {
            moves.push(SolverMove::LayDown(meld));
        }

        // Restore state
        *table = table_snapshot;
        *hand = hand_snapshot;
        return Some(moves);
    }

    // Restore state
    *table = table_snapshot;
    *hand = hand_snapshot;
    None
}

/// Initialize a combination to [0, 1, 2, ..., count-1]
fn generate_combination(
    combo: &mut [usize],
    start: usize,
    _pos: usize,
    _n: usize,
    k: usize,
) -> bool {
    if k == 0 {
        return true;
    }
    for i in 0..k {
        combo[i] = start + i;
    }
    true
}

/// Generate the next combination in lexicographic order
fn next_combination(combo: &mut [usize], n: usize) -> bool {
    let k = combo.len();
    if k == 0 {
        return false;
    }

    // Find the rightmost element that can be incremented
    let mut i = k;
    while i > 0 {
        i -= 1;
        if combo[i] < n - k + i {
            combo[i] += 1;
            // Reset all elements to the right
            for j in (i + 1)..k {
                combo[j] = combo[j - 1] + 1;
            }
            return true;
        }
    }

    false
}

/// Find the best set of melds that can be played from a hand.
///
/// Returns the melds that, when played, leave the hand in the best state
/// according to the quality function. The remaining hand must "beat" the
/// hand_to_beat by having strictly fewer tiles of at least one type, and
/// not having any tile types that hand_to_beat doesn't have.
pub fn find_best_melds<F>(
    hand: &mut Hand,
    quality: F,
    hand_to_beat: &Hand,
) -> Option<Vec<Meld>>
where
    F: Fn(&Hand) -> i32,
{
    // Save the original hand state to ensure we restore it
    let original_hand = hand.clone();

    // Step 1: Generate all possible melds
    let all_possible_melds = generate_all_valid_melds(hand);

    // Step 2: Build tile -> meld indices mapping
    let tile_to_meld_indices = build_tile_index(&all_possible_melds);

    // Step 3: Backtrack to find best combination
    let mut best: Option<(Vec<usize>, i32)> = None;
    let mut active_melds = Vec::new();
    let mut invalid_melds = HashSet::new();

    explore(
        0,
        hand,
        &all_possible_melds,
        &tile_to_meld_indices,
        &mut active_melds,
        &mut invalid_melds,
        &quality,
        hand_to_beat,
        &mut best,
    );

    // Restore the original hand state
    *hand = original_hand;

    // Convert indices back to melds
    best.map(|(indices, _score)| {
        indices.into_iter().map(|i| all_possible_melds[i].clone()).collect()
    })
}

/// Generate all valid melds that could potentially be formed from the hand
fn generate_all_valid_melds(hand: &Hand) -> Vec<Meld> {
    let mut melds = Vec::new();

    // Generate runs for each color
    for color in 0..4 {
        generate_runs_for_color(hand, color, &mut melds);
    }

    // Generate groups for each number
    for number in 1..=13 {
        generate_groups_for_number(hand, number, &mut melds);
    }

    melds
}

/// Generate all possible runs for a specific color
fn generate_runs_for_color(hand: &Hand, color: u8, melds: &mut Vec<Meld>) {
    let num_wildcards = hand.count(&Tile::wild());

    // Try all possible starting positions and lengths
    for start in 1..=11 {
        // Maximum run length from this starting position
        let max_len = 14 - start;

        // Try all lengths >= 3
        for length in 3..=max_len {
            // Generate all possible wildcard placement patterns
            let wildcard_patterns = generate_wildcard_patterns(length, num_wildcards);

            for pattern in wildcard_patterns {
                if can_form_run(hand, color, start, length, &pattern) {
                    let meld = build_run(color, start, length, pattern);
                    melds.push(meld);
                }
            }
        }
    }
}

/// Generate all possible positions where wildcards could be placed
fn generate_wildcard_patterns(length: u8, available_wilds: u8) -> Vec<Vec<u8>> {
    let mut patterns = vec![Vec::new()]; // Start with empty pattern (no wildcards)

    if available_wilds == 0 {
        return patterns;
    }

    // Generate all subsets of positions [0, 1, ..., length-1]
    // Limited to using at most available_wilds wildcards
    for mask in 1..(1 << length) {
        let mut positions = Vec::new();
        for i in 0..length {
            if (mask & (1 << i)) != 0 {
                positions.push(i);
            }
        }

        if positions.len() <= available_wilds as usize {
            patterns.push(positions);
        }
    }

    patterns
}

/// Check if a run can be formed with the given parameters
fn can_form_run(
    hand: &Hand,
    color: u8,
    start: u8,
    length: u8,
    wild_positions: &[u8],
) -> bool {
    let wilds_needed = wild_positions.len();
    if hand.count(&Tile::wild()) < wilds_needed as u8 {
        return false;
    }

    // Check each position in the run
    for i in 0..length {
        if !wild_positions.contains(&i) {
            // Need actual tile
            let tile = Tile::new(color, start + i);
            if hand.count(&tile) == 0 {
                return false;
            }
        }
    }

    true
}

/// Build a run meld
fn build_run(color: u8, start: u8, length: u8, wild_positions: Vec<u8>) -> Meld {
    let mut tiles = VecDeque::new();
    for i in 0..length {
        if wild_positions.contains(&i) {
            tiles.push_back(Tile::wild());
        } else {
            tiles.push_back(Tile::new(color, start + i));
        }
    }
    Meld::new(MeldType::Run, tiles)
}

/// Generate all possible groups for a specific number
fn generate_groups_for_number(hand: &Hand, number: u8, melds: &mut Vec<Meld>) {
    let num_wildcards = hand.count(&Tile::wild());

    // Count available tiles of this number for each color
    let mut available_colors = Vec::new();
    for color in 0..4 {
        let tile = Tile::new(color, number);
        if hand.count(&tile) > 0 {
            available_colors.push(color);
        }
    }

    // Need at least 3 tiles total (colors + wildcards)
    if available_colors.len() + (num_wildcards as usize) < 3 {
        return;
    }

    // Generate all valid combinations of colors + wildcards
    // Groups can be size 3 or 4
    for group_size in 3..=4 {
        let wilds_needed = if group_size > available_colors.len() {
            group_size - available_colors.len()
        } else {
            0
        };

        if wilds_needed > num_wildcards as usize {
            continue;
        }

        // Generate all subsets of available colors of the right size
        generate_color_combinations(&available_colors, group_size - wilds_needed, wilds_needed, number, melds);
    }
}

/// Generate all combinations of colors for a group
fn generate_color_combinations(
    available_colors: &[u8],
    colors_needed: usize,
    wilds_needed: usize,
    number: u8,
    melds: &mut Vec<Meld>,
) {
    if colors_needed == 0 {
        // Just wildcards
        let mut tiles = VecDeque::new();
        for _ in 0..wilds_needed {
            tiles.push_back(Tile::wild());
        }
        if tiles.len() >= 3 {
            melds.push(Meld::new(MeldType::Group, tiles));
        }
        return;
    }

    // Generate all combinations of colors_needed from available_colors
    let mut combination = vec![0; colors_needed];
    generate_combinations_helper(
        available_colors,
        colors_needed,
        0,
        0,
        &mut combination,
        wilds_needed,
        number,
        melds,
    );
}

/// Helper for generating combinations
fn generate_combinations_helper(
    available: &[u8],
    needed: usize,
    start: usize,
    index: usize,
    combination: &mut [u8],
    wilds_needed: usize,
    number: u8,
    melds: &mut Vec<Meld>,
) {
    if index == needed {
        // Build the group
        let mut tiles = VecDeque::new();
        for &color in &combination[..needed] {
            tiles.push_back(Tile::new(color, number));
        }
        for _ in 0..wilds_needed {
            tiles.push_back(Tile::wild());
        }
        melds.push(Meld::new(MeldType::Group, tiles));
        return;
    }

    for i in start..available.len() {
        combination[index] = available[i];
        generate_combinations_helper(
            available,
            needed,
            i + 1,
            index + 1,
            combination,
            wilds_needed,
            number,
            melds,
        );
    }
}

/// Build a map from tiles to the indices of melds that use them
fn build_tile_index(melds: &[Meld]) -> HashMap<Tile, Vec<usize>> {
    let mut index = HashMap::new();

    for (meld_idx, meld) in melds.iter().enumerate() {
        for tile in &meld.tiles {
            index.entry(*tile).or_insert_with(Vec::new).push(meld_idx);
        }
    }

    index
}

/// Recursive backtracking to find the best combination of melds
#[allow(clippy::too_many_arguments)]
fn explore<F>(
    current_index: usize,
    remaining_tiles: &mut Hand,
    all_possible_melds: &[Meld],
    tile_to_meld_indices: &HashMap<Tile, Vec<usize>>,
    active_melds: &mut Vec<usize>,
    invalid_melds: &mut HashSet<usize>,
    quality: &F,
    hand_to_beat: &Hand,
    best: &mut Option<(Vec<usize>, i32)>,
) where
    F: Fn(&Hand) -> i32,
{
    // Terminal check or early termination
    if current_index >= all_possible_melds.len() {
        evaluate_terminal_state(remaining_tiles, active_melds, quality, hand_to_beat, best);
        return;
    }

    // Option 1: Don't take this meld, move to next
    explore(
        current_index + 1,
        remaining_tiles,
        all_possible_melds,
        tile_to_meld_indices,
        active_melds,
        invalid_melds,
        quality,
        hand_to_beat,
        best,
    );

    // Option 2: Take this meld if valid
    let meld = &all_possible_melds[current_index];
    if !invalid_melds.contains(&current_index) && can_play_meld(remaining_tiles, meld) {
        // Play the meld
        remove_tiles_from_meld(remaining_tiles, meld);
        active_melds.push(current_index);

        // Mark conflicting melds as invalid
        let newly_invalid = mark_conflicting_melds(
            meld,
            remaining_tiles,
            tile_to_meld_indices,
            all_possible_melds,
            invalid_melds,
        );

        // Recurse
        explore(
            current_index + 1,
            remaining_tiles,
            all_possible_melds,
            tile_to_meld_indices,
            active_melds,
            invalid_melds,
            quality,
            hand_to_beat,
            best,
        );

        // Backtrack
        unmark_invalid_melds(&newly_invalid, invalid_melds);
        active_melds.pop();
        restore_tiles_from_meld(remaining_tiles, meld);
    }
}

/// Check if a meld can be played from the current hand
fn can_play_meld(hand: &Hand, meld: &Meld) -> bool {
    // Count tiles in meld
    for tile in &meld.tiles {
        if hand.count(tile) == 0 {
            return false;
        }
    }

    // Need to verify we have enough of each tile type
    let mut needed = HashMap::new();
    for tile in &meld.tiles {
        *needed.entry(*tile).or_insert(0u8) += 1;
    }

    for (tile, count) in needed {
        if hand.count(&tile) < count {
            return false;
        }
    }

    true
}

/// Remove tiles from hand based on a meld
fn remove_tiles_from_meld(hand: &mut Hand, meld: &Meld) {
    for tile in &meld.tiles {
        hand.remove(tile);
    }
}

/// Restore tiles to hand (backtracking)
fn restore_tiles_from_meld(hand: &mut Hand, meld: &Meld) {
    for tile in &meld.tiles {
        hand.add(*tile);
    }
}

/// Mark melds that can no longer be played due to insufficient tiles
fn mark_conflicting_melds(
    played_meld: &Meld,
    remaining_tiles: &Hand,
    tile_to_meld_indices: &HashMap<Tile, Vec<usize>>,
    all_possible_melds: &[Meld],
    invalid_melds: &mut HashSet<usize>,
) -> Vec<usize> {
    let mut newly_invalid = Vec::new();

    // Check all melds that share tiles with the played meld
    for tile in &played_meld.tiles {
        if let Some(meld_indices) = tile_to_meld_indices.get(tile) {
            for &meld_idx in meld_indices {
                if !invalid_melds.contains(&meld_idx)
                    && !can_play_meld(remaining_tiles, &all_possible_melds[meld_idx])
                {
                    invalid_melds.insert(meld_idx);
                    newly_invalid.push(meld_idx);
                }
            }
        }
    }

    newly_invalid
}

/// Unmark melds during backtracking
fn unmark_invalid_melds(newly_invalid: &[usize], invalid_melds: &mut HashSet<usize>) {
    for &meld_idx in newly_invalid {
        invalid_melds.remove(&meld_idx);
    }
}

/// Evaluate a terminal state and potentially update the best solution
fn evaluate_terminal_state<F>(
    remaining_hand: &Hand,
    active_melds: &[usize],
    quality: &F,
    hand_to_beat: &Hand,
    best: &mut Option<(Vec<usize>, i32)>,
) where
    F: Fn(&Hand) -> i32,
{
    if beats(remaining_hand, hand_to_beat) {
        let score = quality(remaining_hand);
        if best.as_ref().map_or(true, |(_, best_score)| score > *best_score) {
            *best = Some((active_melds.to_vec(), score));
        }
    }
}

/// Check if one hand "beats" another according to the rules:
/// - Terminal must not contain tile types that baseline doesn't have
/// - Terminal must have strictly fewer tiles than baseline for at least one tile type
fn beats(terminal: &Hand, baseline: &Hand) -> bool {
    let mut has_strict_improvement = false;

    // Check all tile types in terminal
    for (tile, &terminal_count) in terminal.iter() {
        let baseline_count = baseline.count(tile);

        // Terminal has a tile type that baseline doesn't have
        if baseline_count == 0 {
            return false;
        }

        // Track if we have strictly fewer of at least one type
        if terminal_count < baseline_count {
            has_strict_improvement = true;
        }
    }

    // Also check tile types in baseline that terminal doesn't have
    // Having 0 of a tile when baseline has >0 counts as strictly fewer
    for (tile, &baseline_count) in baseline.iter() {
        let terminal_count = terminal.count(tile);
        if terminal_count < baseline_count {
            has_strict_improvement = true;
        }
    }

    has_strict_improvement
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_play_meld_simple() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2
        hand.add(Tile::new(0, 3)); // Red 3

        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::new(0, 2));
        tiles.push_back(Tile::new(0, 3));
        let meld = Meld::new(MeldType::Run, tiles);

        assert!(can_play_meld(&hand, &meld));
    }

    #[test]
    fn test_can_play_meld_insufficient_tiles() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2

        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::new(0, 2));
        tiles.push_back(Tile::new(0, 3));
        let meld = Meld::new(MeldType::Run, tiles);

        assert!(!can_play_meld(&hand, &meld));
    }

    #[test]
    fn test_remove_and_restore_tiles() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1));
        hand.add(Tile::new(0, 2));
        hand.add(Tile::new(0, 3));

        let original = hand.clone();

        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::new(0, 2));
        let meld = Meld::new(MeldType::Run, tiles);

        remove_tiles_from_meld(&mut hand, &meld);
        assert_eq!(hand.count(&Tile::new(0, 1)), 0);
        assert_eq!(hand.count(&Tile::new(0, 2)), 0);
        assert_eq!(hand.count(&Tile::new(0, 3)), 1);

        restore_tiles_from_meld(&mut hand, &meld);
        assert_eq!(hand, original);
    }

    #[test]
    fn test_beats_empty_beats_empty() {
        let empty1 = Hand::new();
        let empty2 = Hand::new();
        // Empty doesn't beat empty (no strict improvement)
        assert!(!beats(&empty1, &empty2));
    }

    #[test]
    fn test_beats_fewer_tiles() {
        let mut baseline = Hand::new();
        baseline.add(Tile::new(0, 1));
        baseline.add(Tile::new(0, 2));

        let mut better = Hand::new();
        better.add(Tile::new(0, 1)); // Same tile type, fewer count

        assert!(beats(&better, &baseline));
    }

    #[test]
    fn test_beats_extra_tile_type() {
        let mut baseline = Hand::new();
        baseline.add(Tile::new(0, 1));

        let mut worse = Hand::new();
        worse.add(Tile::new(0, 1));
        worse.add(Tile::new(0, 2)); // Extra tile type

        assert!(!beats(&worse, &baseline));
    }

    #[test]
    fn test_build_tile_index() {
        let mut tiles1 = VecDeque::new();
        tiles1.push_back(Tile::new(0, 1));
        tiles1.push_back(Tile::new(0, 2));
        let meld1 = Meld::new(MeldType::Run, tiles1);

        let mut tiles2 = VecDeque::new();
        tiles2.push_back(Tile::new(0, 2));
        tiles2.push_back(Tile::new(0, 3));
        let meld2 = Meld::new(MeldType::Run, tiles2);

        let melds = vec![meld1, meld2];
        let index = build_tile_index(&melds);

        assert_eq!(index.get(&Tile::new(0, 1)), Some(&vec![0]));
        assert_eq!(index.get(&Tile::new(0, 2)), Some(&vec![0, 1]));
        assert_eq!(index.get(&Tile::new(0, 3)), Some(&vec![1]));
    }

    #[test]
    fn test_generate_runs_simple() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2
        hand.add(Tile::new(0, 3)); // Red 3
        hand.add(Tile::new(0, 4)); // Red 4

        let melds = generate_all_valid_melds(&hand);

        // Should generate: [1,2,3], [2,3,4], [1,2,3,4]
        assert!(melds.len() >= 3);
        assert!(melds.iter().any(|m| m.tiles.len() == 4));
    }

    #[test]
    fn test_generate_groups_simple() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 5)); // Red 5
        hand.add(Tile::new(1, 5)); // Blue 5
        hand.add(Tile::new(2, 5)); // Yellow 5

        let melds = generate_all_valid_melds(&hand);

        // Should generate at least the group [R5, B5, Y5]
        assert!(melds.iter().any(|m| m.meld_type == MeldType::Group && m.tiles.len() == 3));
    }

    #[test]
    fn test_find_best_melds_simple() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2
        hand.add(Tile::new(0, 3)); // Red 3
        hand.add(Tile::new(0, 4)); // Red 4

        let mut hand_to_beat = Hand::new();
        hand_to_beat.add(Tile::new(0, 1));
        hand_to_beat.add(Tile::new(0, 2));
        hand_to_beat.add(Tile::new(0, 3));
        hand_to_beat.add(Tile::new(0, 4));

        // Quality function: fewer tiles is better
        let quality = |h: &Hand| {
            let total: i32 = h.0.values().map(|&c| c as i32).sum();
            -total // Negative because we want to minimize
        };

        let result = find_best_melds(&mut hand, quality, &hand_to_beat);

        // Should find a solution (play the run of 4)
        assert!(result.is_some());
        let melds = result.unwrap();
        assert!(!melds.is_empty());
    }

    #[test]
    fn test_generate_runs_with_wildcard() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 3)); // Red 3
        hand.add(Tile::wild());     // Wildcard

        let melds = generate_all_valid_melds(&hand);

        // Should generate run [R1, Wild(as R2), R3]
        assert!(melds.iter().any(|m| {
            m.meld_type == MeldType::Run &&
            m.tiles.len() == 3 &&
            m.tiles.contains(&Tile::wild())
        }));
    }

    #[test]
    fn test_generate_groups_with_wildcard() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 5)); // Red 5
        hand.add(Tile::new(1, 5)); // Blue 5
        hand.add(Tile::wild());     // Wildcard

        let melds = generate_all_valid_melds(&hand);

        // Should generate group with wildcard
        assert!(melds.iter().any(|m| {
            m.meld_type == MeldType::Group &&
            m.tiles.len() == 3 &&
            m.tiles.contains(&Tile::wild())
        }));
    }

    #[test]
    fn test_wildcard_in_tile_index() {
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::wild());
        tiles.push_back(Tile::new(0, 3));
        let meld = Meld::new(MeldType::Run, tiles);

        let melds = vec![meld];
        let index = build_tile_index(&melds);

        // Wildcard should be in the index
        assert!(index.contains_key(&Tile::wild()));
        assert_eq!(index.get(&Tile::wild()), Some(&vec![0]));
    }

    #[test]
    fn test_find_best_melds_preserves_hand() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2
        hand.add(Tile::new(0, 3)); // Red 3
        hand.add(Tile::new(1, 5)); // Blue 5

        let original = hand.clone();

        let mut hand_to_beat = Hand::new();
        hand_to_beat.add(Tile::new(0, 1));
        hand_to_beat.add(Tile::new(0, 2));
        hand_to_beat.add(Tile::new(0, 3));
        hand_to_beat.add(Tile::new(1, 5));

        let quality = |h: &Hand| {
            let total: i32 = h.0.values().map(|&c| c as i32).sum();
            -total
        };

        let _result = find_best_melds(&mut hand, quality, &hand_to_beat);

        // Hand should be unchanged regardless of result
        assert_eq!(hand, original);
    }

    #[test]
    fn test_find_best_melds_preserves_hand_no_solution() {
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2

        let original = hand.clone();

        let mut hand_to_beat = Hand::new();
        hand_to_beat.add(Tile::new(0, 1));
        hand_to_beat.add(Tile::new(0, 2));

        let quality = |h: &Hand| {
            let total: i32 = h.0.values().map(|&c| c as i32).sum();
            -total
        };

        let result = find_best_melds(&mut hand, quality, &hand_to_beat);

        // Hand should be unchanged even when no solution is found
        assert_eq!(hand, original);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_best_moves_direct_play() {
        let mut table = Table::new();
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2
        hand.add(Tile::new(0, 3)); // Red 3

        let result = find_best_moves(&mut table, &mut hand, 1000);

        // Should find a solution (direct play)
        assert!(result.is_some());
        let moves = result.unwrap();
        assert!(!moves.is_empty());

        // All moves should be LayDown
        for mov in &moves {
            match mov {
                SolverMove::LayDown(_) => {}
                SolverMove::PickUp(_) => panic!("Should not pick up from empty table"),
            }
        }
    }

    #[test]
    fn test_find_best_moves_with_table_manipulation() {
        let mut table = Table::new();
        // Add a meld to the table: Red 1, 2, 3
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::new(0, 2));
        tiles.push_back(Tile::new(0, 3));
        table.add_meld(Meld::new(MeldType::Run, tiles));

        let mut hand = Hand::new();
        hand.add(Tile::new(0, 4)); // Red 4

        // Cannot play directly, but can pick up the meld and play [1,2,3,4]
        let result = find_best_moves(&mut table, &mut hand, 1000);

        assert!(result.is_some());
        let moves = result.unwrap();
        assert!(!moves.is_empty());

        // Should have at least one PickUp move
        assert!(moves.iter().any(|m| matches!(m, SolverMove::PickUp(_))));
    }

    #[test]
    fn test_find_best_moves_preserves_state() {
        let mut table = Table::new();
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::new(0, 2));
        tiles.push_back(Tile::new(0, 3));
        table.add_meld(Meld::new(MeldType::Run, tiles));

        let original_table = table.clone();

        let mut hand = Hand::new();
        hand.add(Tile::new(1, 5)); // Blue 5

        let original_hand = hand.clone();

        let _result = find_best_moves(&mut table, &mut hand, 1000);

        // State should be preserved
        assert_eq!(table, original_table);
        assert_eq!(hand, original_hand);
    }

    #[test]
    fn test_find_best_moves_no_solution() {
        let mut table = Table::new();
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(1, 5)); // Blue 5

        let result = find_best_moves(&mut table, &mut hand, 1000);

        // Cannot form a valid meld with just 2 unrelated tiles
        assert!(result.is_none());
    }

    #[test]
    fn test_find_best_moves_timeout() {
        let mut table = Table::new();
        // Add several melds to create a complex search space
        for i in 0..3 {
            let mut tiles = VecDeque::new();
            tiles.push_back(Tile::new(i, 1));
            tiles.push_back(Tile::new(i, 2));
            tiles.push_back(Tile::new(i, 3));
            table.add_meld(Meld::new(MeldType::Run, tiles));
        }

        let mut hand = Hand::new();
        hand.add(Tile::new(0, 4));

        let original_table = table.clone();
        let original_hand = hand.clone();

        // Very short timeout
        let _result = find_best_moves(&mut table, &mut hand, 1);

        // State should still be preserved even on timeout
        assert_eq!(table, original_table);
        assert_eq!(hand, original_hand);
    }

    #[test]
    fn test_find_best_moves_empty_table() {
        let mut table = Table::new();
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1));
        hand.add(Tile::new(0, 2));
        hand.add(Tile::new(0, 3));

        let result = find_best_moves(&mut table, &mut hand, 1000);

        // Should succeed with direct play
        assert!(result.is_some());
        let moves = result.unwrap();

        // Should only have LayDown moves
        assert!(moves.iter().all(|m| matches!(m, SolverMove::LayDown(_))));
    }

    #[test]
    fn test_find_best_moves_multiple_melds_from_table() {
        let mut table = Table::new();

        // Add a meld: Red 1,2,3
        let mut tiles1 = VecDeque::new();
        tiles1.push_back(Tile::new(0, 1));
        tiles1.push_back(Tile::new(0, 2));
        tiles1.push_back(Tile::new(0, 3));
        table.add_meld(Meld::new(MeldType::Run, tiles1));

        let mut hand = Hand::new();
        // Add tiles that need the table meld: just Red 4 and 5
        // Can't play these alone, but can pick up Red 1,2,3 and play 1,2,3,4,5
        hand.add(Tile::new(0, 4));
        hand.add(Tile::new(0, 5));

        let result = find_best_moves(&mut table, &mut hand, 2000);

        // Should pick up the meld and form a longer run
        assert!(result.is_some());
        let moves = result.unwrap();

        // Should have at least one PickUp move
        let pickup_count = moves.iter().filter(|m| matches!(m, SolverMove::PickUp(_))).count();
        assert!(pickup_count >= 1, "Should pick up at least 1 meld");

        // Should have at least one LayDown move
        let laydown_count = moves.iter().filter(|m| matches!(m, SolverMove::LayDown(_))).count();
        assert!(laydown_count >= 1, "Should lay down at least 1 meld");
    }

    #[test]
    fn test_find_best_moves_with_wildcard() {
        let mut table = Table::new();
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 3)); // Red 3
        hand.add(Tile::wild());     // Wildcard

        let result = find_best_moves(&mut table, &mut hand, 1000);

        // Should form run with wildcard as Red 2
        assert!(result.is_some());
        let moves = result.unwrap();
        assert!(!moves.is_empty());
    }
}
