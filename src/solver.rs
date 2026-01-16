use crate::{Hand, Meld, MeldType, Table, Tile};
use std::collections::{HashMap, HashSet, VecDeque};

/// Cross-platform time tracker for timeout handling
#[derive(Clone, Copy)]
struct TimeTracker {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
    #[cfg(target_arch = "wasm32")]
    start_ms: f64,
    limit_ms: u64,
}

impl TimeTracker {
    fn new(limit_ms: u64) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            start: std::time::Instant::now(),
            #[cfg(target_arch = "wasm32")]
            start_ms: js_sys::Date::now(),
            limit_ms,
        }
    }

    fn is_expired(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start.elapsed() >= std::time::Duration::from_millis(self.limit_ms)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let now = js_sys::Date::now();
            (now - self.start_ms) >= self.limit_ms as f64
        }
    }
}

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

/// What tile a wild represents in a meld
#[derive(Debug, Clone, PartialEq, Eq)]
enum RepresentedTile {
    /// A specific tile (deterministic: runs, groups of 4)
    Concrete(Tile),
    /// Either of two tiles (ambiguous: groups of 3)
    EitherOf(Tile, Tile),
}

/// Tracks wild replacement obligations when picking up melds from the table
#[derive(Debug, Clone, Default)]
struct WildDebt {
    /// Tiles that MUST be played (from runs and groups of 4)
    concrete: HashMap<Tile, u8>,
    /// Play at least one of the pair (from groups of 3)
    either_or: Vec<(Tile, Tile)>,
}

/// Detailed result from the solver including metadata about the search
#[derive(Debug, Clone)]
pub struct SolverResult {
    /// The sequence of moves to execute, or None if no solution found
    pub moves: Option<Vec<SolverMove>>,
    /// Whether the search completed fully (true) or timed out (false)
    pub search_completed: bool,
    /// Maximum depth explored during the search
    pub depth_reached: usize,
    /// Initial hand quality before solving
    pub initial_quality: i32,
    /// Final hand quality after applying the solution
    pub final_quality: i32,
}

// ============================================================================
// Human-Readable Move Types
// ============================================================================

/// Declarative description of how a meld was transformed or created.
/// These moves describe transformations in terms humans can understand,
/// rather than the internal "destroy and rebuild" approach of SolverMove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HumanMove {
    /// Play a meld entirely from hand (no table tiles involved)
    PlayFromHand(Meld),

    /// Add tile(s) from hand to an existing meld
    ExtendMeld {
        original: Meld,
        added_tiles: Vec<Tile>,
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
    JoinMelds {
        sources: Vec<Meld>,
        result: Meld,
    },

    /// Replace wild(s) in a meld with real tiles, taking the wilds
    SwapWild {
        original: Meld,
        /// (replacement_from_hand, wild_taken)
        swaps: Vec<(Tile, Tile)>,
        result: Meld,
    },

    /// Complex rearrangement that doesn't fit other patterns
    Rearrange {
        consumed: Vec<Meld>,
        produced: Vec<Meld>,
        hand_tiles_used: Vec<Tile>,
    },
}

/// Tracks where a tile came from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TileSource {
    Hand,
    TableMeld(usize), // index into original_table
}

/// Assignment of a tile from source to destination
#[derive(Debug, Clone)]
struct TileAssignment {
    tile: Tile,
    source: TileSource,
    dest_meld_idx: usize,
}

/// Tracks what happened to an original meld
#[derive(Debug)]
struct MeldFate {
    original_idx: usize,
    original: Meld,
    /// Where did each tile end up? (index into new_melds, or None if not placed)
    tile_destinations: Vec<Option<usize>>,
}

/// Tracks where a new meld's tiles came from
#[derive(Debug)]
struct MeldOrigin {
    new_idx: usize,
    new_meld: Meld,
    /// Where did each tile come from?
    tile_sources: Vec<TileSource>,
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
/// 1. Explores depth 0 (direct play from hand with no table manipulation)
/// 2. Then explores depth 1 (removing 1 meld from table), depth 2, etc.
/// 3. For each configuration, attempts to find valid melds to play
/// 4. Continues until the search tree is exhausted or the time limit is reached
/// 5. Returns the best solution found across all explored depths
///
/// Uses MinimizeTiles strategy by default.
pub fn find_best_moves(
    table: &mut Table,
    hand: &mut Hand,
    max_ms: u64,
) -> SolverResult {
    find_best_moves_with_strategy(table, hand, max_ms, ScoringStrategy::MinimizeTiles)
}

/// Find the best sequence of moves using a specific scoring strategy.
///
/// This function uses a BFS approach:
/// 1. Explores depth 0 (direct play from hand with no table manipulation)
/// 2. Then explores depth 1 (removing 1 meld from table), depth 2, etc.
/// 3. For each configuration, attempts to find valid melds to play
/// 4. Continues until the search tree is exhausted or the time limit is reached
/// 5. Returns the best solution found across all explored depths
pub fn find_best_moves_with_strategy(
    table: &mut Table,
    hand: &mut Hand,
    max_ms: u64,
    strategy: ScoringStrategy,
) -> SolverResult {
    let quality = |h: &Hand| strategy.evaluate(h);
    find_best_moves_internal(table, hand, max_ms, quality)
}

/// Internal implementation of find_best_moves that accepts a custom quality function.
fn find_best_moves_internal<F>(
    table: &mut Table,
    hand: &mut Hand,
    max_ms: u64,
    quality: F,
) -> SolverResult
where
    F: Fn(&Hand) -> i32 + Copy,
{
    let timer = TimeTracker::new(max_ms);
    let original_hand = hand.clone();
    let original_table = table.clone();

    // Calculate initial quality
    let initial_quality = quality(&original_hand);

    let mut best_solution: Option<(Vec<SolverMove>, i32)> = None;
    let mut depth_reached = 0;

    // BFS: Try depth 0 (direct play), then 1, 2, 3, etc.
    let max_depth = table.len();

    for depth in 0..=max_depth {
        // Check time limit before starting each depth
        if timer.is_expired() {
            break;
        }

        depth_reached = depth;

        // Try all combinations of removing 'depth' melds from the table
        try_all_combinations_at_depth(
            table,
            hand,
            &original_hand,
            depth,
            quality,
            &timer,
            &mut best_solution,
        );
    }

    // Determine if search completed
    let search_completed = !timer.is_expired() && depth_reached == max_depth;

    // Calculate final quality
    let final_quality = if let Some((ref moves, _)) = best_solution {
        // Simulate applying the moves to calculate final hand quality
        let mut temp_hand = original_hand.clone();
        for mov in moves {
            match mov {
                SolverMove::PickUp(_) => {
                    // PickUp doesn't affect hand directly in our calculation
                }
                SolverMove::LayDown(meld) => {
                    for tile in &meld.tiles {
                        temp_hand.remove(tile);
                    }
                }
            }
        }
        quality(&temp_hand)
    } else {
        initial_quality
    };

    // Restore state
    *hand = original_hand;
    *table = original_table;

    // Return the result with metadata
    SolverResult {
        moves: best_solution.map(|(moves, _score)| moves),
        search_completed,
        depth_reached,
        initial_quality,
        final_quality,
    }
}

/// Try all combinations of removing 'count' melds from the table and update best solution
fn try_all_combinations_at_depth<F>(
    table: &mut Table,
    hand: &mut Hand,
    original_hand: &Hand,
    depth: usize,
    quality: F,
    timer: &TimeTracker,
    best_solution: &mut Option<(Vec<SolverMove>, i32)>,
)
where
    F: Fn(&Hand) -> i32 + Copy,
{
    let table_size = table.len();

    // Depth 0 means direct play from hand (no table manipulation)
    // No wild debt since we're not picking up any melds
    if depth == 0 {
        let empty_debt = WildDebt::default();
        if let Some(melds) = find_best_melds(hand, quality, original_hand, timer, &empty_debt) {
            let moves: Vec<SolverMove> = melds
                .iter()
                .map(|meld| SolverMove::LayDown(meld.clone()))
                .collect();

            // Calculate score for this solution
            let mut temp_hand = original_hand.clone();
            for meld in &melds {
                for tile in &meld.tiles {
                    temp_hand.remove(tile);
                }
            }
            let score = quality(&temp_hand);

            // Update best solution if this is better
            if best_solution.as_ref().map_or(true, |(_, best_score)| score > *best_score) {
                *best_solution = Some((moves, score));
            }
        }
        return;
    }

    // For depth > 0, try all combinations of removing 'depth' melds
    if depth > table_size {
        return;
    }

    // Generate all combinations of indices to remove
    let mut indices = vec![0; depth];
    if !generate_combination(&mut indices, 0, 0, table_size, depth) {
        return;
    }

    loop {
        // Check time limit
        if timer.is_expired() {
            return;
        }

        // Try this combination and update best solution if better
        try_meld_combination(table, hand, original_hand, &indices, quality, timer, best_solution);

        // Generate next combination
        if !next_combination(&mut indices, table_size) {
            break;
        }
    }
}

/// Try removing the melds at the given indices and update best solution if better
fn try_meld_combination<F>(
    table: &mut Table,
    hand: &mut Hand,
    original_hand: &Hand,
    indices: &[usize],
    quality: F,
    timer: &TimeTracker,
    best_solution: &mut Option<(Vec<SolverMove>, i32)>,
)
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

    // Compute wild debts from the removed melds
    // Any wilds in these melds require replacement tiles to be played
    let wild_debt = compute_wild_debts(&removed_melds);

    // Try to find melds from the new hand
    if let Some(melds) = find_best_melds(hand, quality, original_hand, timer, &wild_debt) {
        // Build the move sequence
        let mut moves = Vec::new();

        // First, pick up the melds (in the order we removed them, which is reversed)
        for (idx, _) in removed_melds.iter().rev() {
            moves.push(SolverMove::PickUp(*idx));
        }

        // Then, lay down the new melds
        for meld in &melds {
            moves.push(SolverMove::LayDown(meld.clone()));
        }

        // Calculate score for this solution
        let mut temp_hand = original_hand.clone();
        for meld in &melds {
            for tile in &meld.tiles {
                temp_hand.remove(tile);
            }
        }
        let score = quality(&temp_hand);

        // Update best solution if this is better
        if best_solution.as_ref().map_or(true, |(_, best_score)| score > *best_score) {
            *best_solution = Some((moves, score));
        }
    }

    // Restore state
    *table = table_snapshot;
    *hand = hand_snapshot;
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
///
/// The wild_debt parameter specifies tiles that MUST be played in the melds
/// to satisfy wild replacement constraints from picked-up table melds.
fn find_best_melds<F>(
    hand: &mut Hand,
    quality: F,
    hand_to_beat: &Hand,
    timer: &TimeTracker,
    wild_debt: &WildDebt,
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
        timer,
        wild_debt,
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
    timer: &TimeTracker,
    wild_debt: &WildDebt,
    best: &mut Option<(Vec<usize>, i32)>,
) where
    F: Fn(&Hand) -> i32,
{
    // Check timer for early exit
    if timer.is_expired() {
        return;
    }

    // Terminal check or early termination
    if current_index >= all_possible_melds.len() {
        evaluate_terminal_state(
            remaining_tiles,
            active_melds,
            all_possible_melds,
            quality,
            hand_to_beat,
            wild_debt,
            best,
        );
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
        timer,
        wild_debt,
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
            timer,
            wild_debt,
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
#[allow(clippy::too_many_arguments)]
fn evaluate_terminal_state<F>(
    remaining_hand: &Hand,
    active_melds: &[usize],
    all_possible_melds: &[Meld],
    quality: &F,
    hand_to_beat: &Hand,
    wild_debt: &WildDebt,
    best: &mut Option<(Vec<usize>, i32)>,
) where
    F: Fn(&Hand) -> i32,
{
    // First check if this beats the hand to beat
    if !beats(remaining_hand, hand_to_beat) {
        return;
    }

    // Check if wild debt is satisfied by the played melds
    let played_melds: Vec<Meld> = active_melds
        .iter()
        .map(|&i| all_possible_melds[i].clone())
        .collect();

    if !is_wild_debt_satisfied(wild_debt, &played_melds) {
        return;
    }

    // This is a valid solution - check if it's the best
    let score = quality(remaining_hand);
    if best.as_ref().map_or(true, |(_, best_score)| score > *best_score) {
        *best = Some((active_melds.to_vec(), score));
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

// ============================================================================
// Wild Debt Computation
// ============================================================================

/// Compute what tile a wild represents at a given position in a meld.
///
/// For runs: the wild's position determines its number.
/// For groups of 4: the wild represents the one missing color.
/// For groups of 3: the wild could be either of two missing colors (EitherOf).
fn compute_represented_tile(meld: &Meld, wild_position: usize) -> Option<RepresentedTile> {
    match meld.meld_type {
        MeldType::Run => {
            // Find the color from any non-wild tile
            let color = meld.tiles.iter().find_map(|t| t.color())?;

            // Find the starting number by looking at non-wild tiles
            // For each non-wild tile, we can compute: start = tile.number - position
            let start = meld.tiles.iter().enumerate().find_map(|(i, t)| {
                t.number().map(|n| n as i32 - i as i32)
            })?;

            // The wild at position `wild_position` represents start + wild_position
            let represented_number = (start + wild_position as i32) as u8;
            if represented_number >= 1 && represented_number <= 13 {
                Some(RepresentedTile::Concrete(Tile::new(color, represented_number)))
            } else {
                None
            }
        }
        MeldType::Group => {
            // Find the number from any non-wild tile
            let number = meld.tiles.iter().find_map(|t| t.number())?;

            // Find which colors are present (non-wild tiles)
            let colors_present: Vec<u8> = meld.tiles.iter()
                .filter_map(|t| t.color())
                .collect();

            // Find missing colors (0-3)
            let missing_colors: Vec<u8> = (0..4)
                .filter(|c| !colors_present.contains(c))
                .collect();

            match missing_colors.len() {
                1 => {
                    // Group of 4 with one wild: wild represents the one missing color
                    Some(RepresentedTile::Concrete(Tile::new(missing_colors[0], number)))
                }
                2 => {
                    // Group of 3 with one wild: wild could be either missing color
                    Some(RepresentedTile::EitherOf(
                        Tile::new(missing_colors[0], number),
                        Tile::new(missing_colors[1], number),
                    ))
                }
                _ => {
                    // More than 2 missing colors means multiple wilds -
                    // each wild could be any missing color, but we need to be consistent
                    // For simplicity, return the first missing color
                    if !missing_colors.is_empty() {
                        Some(RepresentedTile::Concrete(Tile::new(missing_colors[0], number)))
                    } else {
                        None
                    }
                }
            }
        }
    }
}

/// Compute wild debts from a list of picked-up melds.
///
/// For each wild in each picked meld, we determine what tile it represents
/// and add it to the debt structure.
fn compute_wild_debts(picked_melds: &[(usize, Meld)]) -> WildDebt {
    let mut debt = WildDebt::default();

    for (_, meld) in picked_melds {
        for (pos, tile) in meld.tiles.iter().enumerate() {
            if tile.is_wild() {
                if let Some(represented) = compute_represented_tile(meld, pos) {
                    match represented {
                        RepresentedTile::Concrete(t) => {
                            *debt.concrete.entry(t).or_insert(0) += 1;
                        }
                        RepresentedTile::EitherOf(t1, t2) => {
                            debt.either_or.push((t1, t2));
                        }
                    }
                }
            }
        }
    }

    debt
}

/// Check if the wild debt is satisfied by the tiles played in the given melds.
///
/// Returns true if all debts are paid:
/// - For concrete debts: the tile must appear in played melds at least debt_count times
/// - For either-or debts: at least one of the two options must appear in played melds
fn is_wild_debt_satisfied(debt: &WildDebt, played_melds: &[Meld]) -> bool {
    // Count tiles played in all melds
    let mut played_counts: HashMap<Tile, u8> = HashMap::new();
    for meld in played_melds {
        for tile in &meld.tiles {
            if !tile.is_wild() {
                *played_counts.entry(*tile).or_insert(0) += 1;
            }
        }
    }

    // Check concrete debts
    for (tile, &required_count) in &debt.concrete {
        let played = played_counts.get(tile).copied().unwrap_or(0);
        if played < required_count {
            return false;
        }
    }

    // Check either-or debts
    for (t1, t2) in &debt.either_or {
        let played_t1 = played_counts.get(t1).copied().unwrap_or(0);
        let played_t2 = played_counts.get(t2).copied().unwrap_or(0);
        if played_t1 == 0 && played_t2 == 0 {
            return false;
        }
    }

    true
}

// ============================================================================
// Human Move Translation
// ============================================================================

/// Translate a sequence of SolverMoves into human-readable HumanMoves.
///
/// This function analyzes the before/after state of the table and hand to produce
/// moves that describe transformations in terms humans can understand.
pub fn translate_to_human_moves(
    original_table: &Table,
    original_hand: &Hand,
    solver_moves: &[SolverMove],
) -> Vec<HumanMove> {
    // Extract picked up melds and laid down melds from solver moves
    let mut picked_melds: Vec<(usize, Meld)> = Vec::new();
    let mut laid_down_melds: Vec<Meld> = Vec::new();

    for mov in solver_moves {
        match mov {
            SolverMove::PickUp(idx) => {
                if let Some(meld) = original_table.melds().get(*idx) {
                    picked_melds.push((*idx, meld.clone()));
                }
            }
            SolverMove::LayDown(meld) => {
                laid_down_melds.push(meld.clone());
            }
        }
    }

    // If no moves, return empty
    if laid_down_melds.is_empty() {
        return Vec::new();
    }

    // Assign tile provenance
    let assignments = assign_tile_provenance(&picked_melds, original_hand, &laid_down_melds);

    // Build MeldOrigin for each new meld
    let meld_origins = build_meld_origins(&laid_down_melds, &assignments);

    // Build MeldFate for each picked-up meld
    let meld_fates = build_meld_fates(&picked_melds, &assignments);

    // Now analyze patterns and generate human moves
    generate_human_moves(&picked_melds, &laid_down_melds, &meld_origins, &meld_fates, original_hand)
}

/// Assign tile provenance - determine which source tile maps to which destination tile
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
    let mut assignments = Vec::new();
    let mut used = vec![false; source_pool.len()];

    for (meld_idx, meld) in new_melds.iter().enumerate() {
        for tile in meld.tiles.iter() {
            // First try to find matching table source
            let source_idx = source_pool
                .iter()
                .enumerate()
                .position(|(i, (t, src))| {
                    !used[i] && *t == *tile && matches!(src, TileSource::TableMeld(_))
                })
                .or_else(|| {
                    // Fall back to hand source
                    source_pool
                        .iter()
                        .enumerate()
                        .position(|(i, (t, _))| !used[i] && *t == *tile)
                });

            if let Some(i) = source_idx {
                used[i] = true;
                assignments.push(TileAssignment {
                    tile: *tile,
                    source: source_pool[i].1,
                    dest_meld_idx: meld_idx,
                });
            }
        }
    }

    assignments
}

/// Build MeldOrigin for each new meld
fn build_meld_origins(new_melds: &[Meld], assignments: &[TileAssignment]) -> Vec<MeldOrigin> {
    new_melds
        .iter()
        .enumerate()
        .map(|(idx, meld)| {
            let tile_sources: Vec<TileSource> = meld
                .tiles
                .iter()
                .map(|tile| {
                    // Find the assignment for this tile in this meld
                    assignments
                        .iter()
                        .find(|a| a.dest_meld_idx == idx && a.tile == *tile)
                        .map(|a| a.source)
                        .unwrap_or(TileSource::Hand)
                })
                .collect();

            MeldOrigin {
                new_idx: idx,
                new_meld: meld.clone(),
                tile_sources,
            }
        })
        .collect()
}

/// Build MeldFate for each picked-up meld
fn build_meld_fates(
    picked_melds: &[(usize, Meld)],
    assignments: &[TileAssignment],
) -> Vec<MeldFate> {
    picked_melds
        .iter()
        .map(|(orig_idx, meld)| {
            let tile_destinations: Vec<Option<usize>> = meld
                .tiles
                .iter()
                .map(|tile| {
                    // Find where this tile ended up
                    assignments
                        .iter()
                        .find(|a| {
                            a.tile == *tile && matches!(a.source, TileSource::TableMeld(i) if i == *orig_idx)
                        })
                        .map(|a| a.dest_meld_idx)
                })
                .collect();

            MeldFate {
                original_idx: *orig_idx,
                original: meld.clone(),
                tile_destinations,
            }
        })
        .collect()
}

/// Generate human-readable moves from the analyzed data
fn generate_human_moves(
    picked_melds: &[(usize, Meld)],
    _laid_down_melds: &[Meld],
    meld_origins: &[MeldOrigin],
    meld_fates: &[MeldFate],
    _original_hand: &Hand,
) -> Vec<HumanMove> {
    let mut human_moves = Vec::new();
    let mut processed_new_melds = HashSet::new();
    let mut processed_old_melds = HashSet::new();

    // First pass: detect PlayFromHand (melds entirely from hand)
    for origin in meld_origins {
        if origin.tile_sources.iter().all(|s| matches!(s, TileSource::Hand)) {
            human_moves.push(HumanMove::PlayFromHand(origin.new_meld.clone()));
            processed_new_melds.insert(origin.new_idx);
        }
    }

    // Second pass: detect ExtendMeld (original meld + hand tiles = new meld)
    for fate in meld_fates {
        if processed_old_melds.contains(&fate.original_idx) {
            continue;
        }

        // Check if all tiles went to the same new meld
        let destinations: HashSet<usize> = fate
            .tile_destinations
            .iter()
            .filter_map(|d| *d)
            .collect();

        if destinations.len() == 1 {
            let dest_idx = *destinations.iter().next().unwrap();
            if processed_new_melds.contains(&dest_idx) {
                continue;
            }

            let origin = &meld_origins[dest_idx];

            // Check if the new meld has additional tiles from hand
            let hand_tiles: Vec<Tile> = origin
                .new_meld
                .tiles
                .iter()
                .zip(origin.tile_sources.iter())
                .filter_map(|(tile, src)| {
                    if matches!(src, TileSource::Hand) {
                        Some(*tile)
                    } else {
                        None
                    }
                })
                .collect();

            if !hand_tiles.is_empty() && origin.new_meld.tiles.len() > fate.original.tiles.len() {
                // This is an ExtendMeld
                human_moves.push(HumanMove::ExtendMeld {
                    original: fate.original.clone(),
                    added_tiles: hand_tiles,
                    result: origin.new_meld.clone(),
                });
                processed_new_melds.insert(dest_idx);
                processed_old_melds.insert(fate.original_idx);
            } else if hand_tiles.is_empty() && meld_tiles_equal(&fate.original, &origin.new_meld) {
                // Unchanged meld - skip it
                processed_new_melds.insert(dest_idx);
                processed_old_melds.insert(fate.original_idx);
            }
        }
    }

    // Third pass: detect SplitMeld (one original becomes multiple new melds)
    for fate in meld_fates {
        if processed_old_melds.contains(&fate.original_idx) {
            continue;
        }

        let destinations: HashSet<usize> = fate
            .tile_destinations
            .iter()
            .filter_map(|d| *d)
            .collect();

        // If tiles went to multiple destinations and all destinations only have tiles from this source
        if destinations.len() >= 2 {
            let mut is_pure_split = true;
            for &dest_idx in &destinations {
                if processed_new_melds.contains(&dest_idx) {
                    is_pure_split = false;
                    break;
                }
                let origin = &meld_origins[dest_idx];
                // Check if this new meld only has tiles from this original meld
                for src in &origin.tile_sources {
                    if let TileSource::TableMeld(idx) = src {
                        if *idx != fate.original_idx {
                            is_pure_split = false;
                            break;
                        }
                    } else {
                        is_pure_split = false;
                        break;
                    }
                }
            }

            if is_pure_split {
                let parts: Vec<Meld> = destinations
                    .iter()
                    .map(|&idx| meld_origins[idx].new_meld.clone())
                    .collect();

                human_moves.push(HumanMove::SplitMeld {
                    original: fate.original.clone(),
                    parts,
                });

                for dest_idx in destinations {
                    processed_new_melds.insert(dest_idx);
                }
                processed_old_melds.insert(fate.original_idx);
            }
        }
    }

    // Fourth pass: detect JoinMelds (multiple originals become one new meld)
    for origin in meld_origins {
        if processed_new_melds.contains(&origin.new_idx) {
            continue;
        }

        // Find all unique table sources for this meld
        let table_sources: HashSet<usize> = origin
            .tile_sources
            .iter()
            .filter_map(|s| {
                if let TileSource::TableMeld(idx) = s {
                    Some(*idx)
                } else {
                    None
                }
            })
            .collect();

        // If multiple table sources and no hand tiles, this is a JoinMelds
        if table_sources.len() >= 2 {
            let has_hand_tiles = origin.tile_sources.iter().any(|s| matches!(s, TileSource::Hand));

            if !has_hand_tiles {
                // Check all sources are unprocessed
                if table_sources.iter().all(|idx| !processed_old_melds.contains(idx)) {
                    let sources: Vec<Meld> = table_sources
                        .iter()
                        .filter_map(|idx| {
                            picked_melds.iter().find(|(i, _)| i == idx).map(|(_, m)| m.clone())
                        })
                        .collect();

                    human_moves.push(HumanMove::JoinMelds {
                        sources,
                        result: origin.new_meld.clone(),
                    });

                    processed_new_melds.insert(origin.new_idx);
                    for idx in table_sources {
                        processed_old_melds.insert(idx);
                    }
                }
            }
        }
    }

    // Fifth pass: detect SwapWild
    for fate in meld_fates {
        if processed_old_melds.contains(&fate.original_idx) {
            continue;
        }

        // Check if original has wilds and a new meld has the same tiles but with wilds replaced
        let wild_positions: Vec<usize> = fate
            .original
            .tiles
            .iter()
            .enumerate()
            .filter(|(_, t)| t.is_wild())
            .map(|(i, _)| i)
            .collect();

        if wild_positions.is_empty() {
            continue;
        }

        // Find the destination meld
        let destinations: HashSet<usize> = fate
            .tile_destinations
            .iter()
            .filter_map(|d| *d)
            .collect();

        if destinations.len() == 1 {
            let dest_idx = *destinations.iter().next().unwrap();
            if processed_new_melds.contains(&dest_idx) {
                continue;
            }

            let origin = &meld_origins[dest_idx];

            // Check if the new meld has the same structure but with wilds replaced
            if origin.new_meld.tiles.len() == fate.original.tiles.len() {
                let mut swaps = Vec::new();
                let mut is_swap = true;

                for pos in &wild_positions {
                    // Check if the tile at this position is now from hand
                    if *pos < origin.tile_sources.len() {
                        if matches!(origin.tile_sources[*pos], TileSource::Hand) {
                            let replacement = origin.new_meld.tiles[*pos];
                            let wild = Tile::wild();
                            swaps.push((replacement, wild));
                        } else {
                            is_swap = false;
                            break;
                        }
                    }
                }

                if is_swap && !swaps.is_empty() {
                    human_moves.push(HumanMove::SwapWild {
                        original: fate.original.clone(),
                        swaps,
                        result: origin.new_meld.clone(),
                    });
                    processed_new_melds.insert(dest_idx);
                    processed_old_melds.insert(fate.original_idx);
                }
            }
        }
    }

    // Final pass: anything remaining becomes a Rearrange
    let unprocessed_old: Vec<Meld> = meld_fates
        .iter()
        .filter(|f| !processed_old_melds.contains(&f.original_idx))
        .map(|f| f.original.clone())
        .collect();

    let unprocessed_new: Vec<Meld> = meld_origins
        .iter()
        .filter(|o| !processed_new_melds.contains(&o.new_idx))
        .map(|o| o.new_meld.clone())
        .collect();

    if !unprocessed_new.is_empty() {
        // Collect hand tiles used in unprocessed new melds
        let hand_tiles_used: Vec<Tile> = meld_origins
            .iter()
            .filter(|o| !processed_new_melds.contains(&o.new_idx))
            .flat_map(|o| {
                o.new_meld
                    .tiles
                    .iter()
                    .zip(o.tile_sources.iter())
                    .filter_map(|(tile, src)| {
                        if matches!(src, TileSource::Hand) {
                            Some(*tile)
                        } else {
                            None
                        }
                    })
            })
            .collect();

        human_moves.push(HumanMove::Rearrange {
            consumed: unprocessed_old,
            produced: unprocessed_new,
            hand_tiles_used,
        });
    }

    human_moves
}

/// Check if two melds have the same tiles (in the same order)
fn meld_tiles_equal(a: &Meld, b: &Meld) -> bool {
    if a.tiles.len() != b.tiles.len() {
        return false;
    }
    a.tiles.iter().zip(b.tiles.iter()).all(|(t1, t2)| t1 == t2)
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

        let timer = TimeTracker::new(1000);
        let empty_debt = WildDebt::default();
        let result = find_best_melds(&mut hand, quality, &hand_to_beat, &timer, &empty_debt);

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

        let timer = TimeTracker::new(1000);
        let empty_debt = WildDebt::default();
        let _result = find_best_melds(&mut hand, quality, &hand_to_beat, &timer, &empty_debt);

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

        let timer = TimeTracker::new(1000);
        let empty_debt = WildDebt::default();
        let result = find_best_melds(&mut hand, quality, &hand_to_beat, &timer, &empty_debt);

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
        assert!(result.moves.is_some());
        let moves = result.moves.unwrap();
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

        assert!(result.moves.is_some());
        let moves = result.moves.unwrap();
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
        assert!(result.moves.is_none());
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
        assert!(result.moves.is_some());
        let moves = result.moves.unwrap();

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
        assert!(result.moves.is_some());
        let moves = result.moves.unwrap();

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
        assert!(result.moves.is_some());
        let moves = result.moves.unwrap();
        assert!(!moves.is_empty());
    }

    #[test]
    fn test_find_best_moves_returns_best_not_first() {
        // This test verifies that the solver continues searching and returns
        // the BEST solution found, not just the first solution.
        //
        // Setup:
        // - Hand: Red 1,2,3,4,11 (can play 1,2,3 at depth 0, leaving 4,11)
        // - Table: Red 5,6,7 (one meld)
        //
        // Depth 0 (direct play): Can only play Red 1,2,3, leaving Red 4,11 (2 tiles)
        // Depth 1 (pick up table): Can play Red 1,2,3,4,5,6,7, leaving Red 11 (1 tile)
        //
        // The solver should return the depth 1 solution (1 remaining tile is better than 2)

        let mut table = Table::new();
        // Add Red 5,6,7 to the table
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 5));
        tiles.push_back(Tile::new(0, 6));
        tiles.push_back(Tile::new(0, 7));
        table.add_meld(Meld::new(MeldType::Run, tiles));

        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1));  // Red 1
        hand.add(Tile::new(0, 2));  // Red 2
        hand.add(Tile::new(0, 3));  // Red 3
        hand.add(Tile::new(0, 4));  // Red 4
        hand.add(Tile::new(0, 11)); // Red 11 (isolated tile)

        // First, verify depth 0 works
        let original_hand = hand.clone();
        let quality = |h: &Hand| {
            let total: i32 = h.iter().map(|(_, &c)| c as i32).sum();
            -total
        };
        let timer = TimeTracker::new(5000);
        let empty_debt = WildDebt::default();
        let depth0_result = find_best_melds(&mut hand, quality, &original_hand, &timer, &empty_debt);
        assert!(depth0_result.is_some(), "Depth 0 should find a solution");
        let depth0_melds = depth0_result.unwrap();

        // Calculate remaining tiles at depth 0
        let mut remaining_depth0 = original_hand.clone();
        for meld in &depth0_melds {
            for tile in &meld.tiles {
                remaining_depth0.remove(tile);
            }
        }
        let depth0_remaining: i32 = remaining_depth0.iter().map(|(_, &c)| c as i32).sum();

        // Now test find_best_moves
        let mut hand = original_hand.clone();
        let result = find_best_moves(&mut table, &mut hand, 5000);

        assert!(result.moves.is_some(), "find_best_moves should find a solution");
        let moves = result.moves.unwrap();

        // Calculate remaining tiles from the moves
        let mut test_hand = original_hand.clone();
        let mut test_table = table.clone();
        for mov in &moves {
            match mov {
                SolverMove::PickUp(idx) => {
                    let meld = test_table.remove_meld(*idx).unwrap();
                    for tile in &meld.tiles {
                        test_hand.add(*tile);
                    }
                }
                SolverMove::LayDown(meld) => {
                    for tile in &meld.tiles {
                        test_hand.remove(tile);
                    }
                }
            }
        }
        let final_remaining: i32 = test_hand.iter().map(|(_, &c)| c as i32).sum();

        // The final solution should be better than or equal to depth 0
        assert!(final_remaining <= depth0_remaining,
                "Final solution ({} tiles) should be at least as good as depth 0 ({} tiles)",
                final_remaining, depth0_remaining);
    }

    #[test]
    fn test_find_best_moves_explores_multiple_depths() {
        // Verify that the solver can explore multiple depths and finds
        // solutions even when direct play doesn't work

        let mut table = Table::new();
        // Add a meld to table: Red 1,2,3
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::new(0, 2));
        tiles.push_back(Tile::new(0, 3));
        table.add_meld(Meld::new(MeldType::Run, tiles));

        let mut hand = Hand::new();
        // Add only Red 4 - cannot form a valid meld by itself
        hand.add(Tile::new(0, 4));

        let original_table = table.clone();
        let original_hand = hand.clone();

        let result = find_best_moves(&mut table, &mut hand, 5000);

        // Should find a solution by picking up the table meld
        assert!(result.moves.is_some(), "Should find a solution at depth > 0");

        // Verify state is preserved
        assert_eq!(table, original_table);
        assert_eq!(hand, original_hand);
    }

    // ========================================================================
    // Human Move Translation Tests
    // ========================================================================

    #[test]
    fn test_translate_play_from_hand() {
        // Playing tiles entirely from hand should produce PlayFromHand
        let table = Table::new();
        let mut hand = Hand::new();
        hand.add(Tile::new(0, 1)); // Red 1
        hand.add(Tile::new(0, 2)); // Red 2
        hand.add(Tile::new(0, 3)); // Red 3

        let mut laid_tiles = VecDeque::new();
        laid_tiles.push_back(Tile::new(0, 1));
        laid_tiles.push_back(Tile::new(0, 2));
        laid_tiles.push_back(Tile::new(0, 3));
        let meld = Meld::new(MeldType::Run, laid_tiles);

        let solver_moves = vec![SolverMove::LayDown(meld.clone())];

        let human_moves = translate_to_human_moves(&table, &hand, &solver_moves);

        assert_eq!(human_moves.len(), 1);
        match &human_moves[0] {
            HumanMove::PlayFromHand(m) => {
                assert_eq!(m.tiles.len(), 3);
            }
            _ => panic!("Expected PlayFromHand, got {:?}", human_moves[0]),
        }
    }

    #[test]
    fn test_translate_extend_meld() {
        // Picking up a meld and laying down a longer version should produce ExtendMeld
        let mut table = Table::new();
        let mut original_tiles = VecDeque::new();
        original_tiles.push_back(Tile::new(0, 1));
        original_tiles.push_back(Tile::new(0, 2));
        original_tiles.push_back(Tile::new(0, 3));
        let original_meld = Meld::new(MeldType::Run, original_tiles);
        table.add_meld(original_meld.clone());

        let mut hand = Hand::new();
        hand.add(Tile::new(0, 4)); // Red 4

        // Extended meld: [1, 2, 3, 4]
        let mut extended_tiles = VecDeque::new();
        extended_tiles.push_back(Tile::new(0, 1));
        extended_tiles.push_back(Tile::new(0, 2));
        extended_tiles.push_back(Tile::new(0, 3));
        extended_tiles.push_back(Tile::new(0, 4));
        let extended_meld = Meld::new(MeldType::Run, extended_tiles);

        let solver_moves = vec![
            SolverMove::PickUp(0),
            SolverMove::LayDown(extended_meld.clone()),
        ];

        let human_moves = translate_to_human_moves(&table, &hand, &solver_moves);

        assert_eq!(human_moves.len(), 1);
        match &human_moves[0] {
            HumanMove::ExtendMeld {
                original,
                added_tiles,
                result,
            } => {
                assert_eq!(original.tiles.len(), 3);
                assert_eq!(added_tiles.len(), 1);
                assert_eq!(added_tiles[0], Tile::new(0, 4));
                assert_eq!(result.tiles.len(), 4);
            }
            _ => panic!("Expected ExtendMeld, got {:?}", human_moves[0]),
        }
    }

    #[test]
    fn test_translate_split_meld() {
        // Picking up a long meld and splitting it should produce SplitMeld
        let mut table = Table::new();
        let mut original_tiles = VecDeque::new();
        for i in 1..=6 {
            original_tiles.push_back(Tile::new(0, i));
        }
        let original_meld = Meld::new(MeldType::Run, original_tiles);
        table.add_meld(original_meld.clone());

        let hand = Hand::new(); // No tiles in hand

        // Split into [1,2,3] and [4,5,6]
        let mut part1_tiles = VecDeque::new();
        for i in 1..=3 {
            part1_tiles.push_back(Tile::new(0, i));
        }
        let part1 = Meld::new(MeldType::Run, part1_tiles);

        let mut part2_tiles = VecDeque::new();
        for i in 4..=6 {
            part2_tiles.push_back(Tile::new(0, i));
        }
        let part2 = Meld::new(MeldType::Run, part2_tiles);

        let solver_moves = vec![
            SolverMove::PickUp(0),
            SolverMove::LayDown(part1.clone()),
            SolverMove::LayDown(part2.clone()),
        ];

        let human_moves = translate_to_human_moves(&table, &hand, &solver_moves);

        assert_eq!(human_moves.len(), 1);
        match &human_moves[0] {
            HumanMove::SplitMeld { original, parts } => {
                assert_eq!(original.tiles.len(), 6);
                assert_eq!(parts.len(), 2);
            }
            _ => panic!("Expected SplitMeld, got {:?}", human_moves[0]),
        }
    }

    #[test]
    fn test_translate_join_melds() {
        // Picking up two melds and combining them should produce JoinMelds
        let mut table = Table::new();

        // First meld: Red 1,2,3
        let mut meld1_tiles = VecDeque::new();
        for i in 1..=3 {
            meld1_tiles.push_back(Tile::new(0, i));
        }
        table.add_meld(Meld::new(MeldType::Run, meld1_tiles));

        // Second meld: Red 4,5,6
        let mut meld2_tiles = VecDeque::new();
        for i in 4..=6 {
            meld2_tiles.push_back(Tile::new(0, i));
        }
        table.add_meld(Meld::new(MeldType::Run, meld2_tiles));

        let hand = Hand::new(); // No tiles in hand

        // Combine into [1,2,3,4,5,6]
        let mut combined_tiles = VecDeque::new();
        for i in 1..=6 {
            combined_tiles.push_back(Tile::new(0, i));
        }
        let combined = Meld::new(MeldType::Run, combined_tiles);

        let solver_moves = vec![
            SolverMove::PickUp(0),
            SolverMove::PickUp(1),
            SolverMove::LayDown(combined.clone()),
        ];

        let human_moves = translate_to_human_moves(&table, &hand, &solver_moves);

        assert_eq!(human_moves.len(), 1);
        match &human_moves[0] {
            HumanMove::JoinMelds { sources, result } => {
                assert_eq!(sources.len(), 2);
                assert_eq!(result.tiles.len(), 6);
            }
            _ => panic!("Expected JoinMelds, got {:?}", human_moves[0]),
        }
    }

    #[test]
    fn test_translate_rearrange_fallback() {
        // Complex rearrangement that doesn't fit other patterns
        let mut table = Table::new();

        // Original meld: Red 1,2,3,4,5
        let mut original_tiles = VecDeque::new();
        for i in 1..=5 {
            original_tiles.push_back(Tile::new(0, i));
        }
        table.add_meld(Meld::new(MeldType::Run, original_tiles));

        let mut hand = Hand::new();
        hand.add(Tile::new(1, 3)); // Blue 3
        hand.add(Tile::new(2, 3)); // Yellow 3

        // Rearrange into:
        // - Run: Red 1, 2
        // - Group: Red 3, Blue 3, Yellow 3
        // - Run: Red 4, 5

        // But runs need 3 tiles, so this isn't valid
        // Let's make a valid scenario

        // Clear and redo: Original meld: Red 1,2,3,4,5,6
        let mut table = Table::new();
        let mut original_tiles = VecDeque::new();
        for i in 1..=6 {
            original_tiles.push_back(Tile::new(0, i));
        }
        table.add_meld(Meld::new(MeldType::Run, original_tiles));

        // Rearrange into:
        // - Run: Red 1, 2, 3
        // - Group: Red 4, Blue 4, Yellow 4
        // - Run: Red 5, 6 (invalid, needs 3 tiles)

        // Let's use a simpler scenario that requires rearrange
        let mut hand = Hand::new();
        hand.add(Tile::new(1, 4)); // Blue 4
        hand.add(Tile::new(2, 4)); // Yellow 4

        let mut run1_tiles = VecDeque::new();
        for i in 1..=3 {
            run1_tiles.push_back(Tile::new(0, i));
        }
        let run1 = Meld::new(MeldType::Run, run1_tiles);

        let mut group_tiles = VecDeque::new();
        group_tiles.push_back(Tile::new(0, 4));
        group_tiles.push_back(Tile::new(1, 4));
        group_tiles.push_back(Tile::new(2, 4));
        let group = Meld::new(MeldType::Group, group_tiles);

        let mut run2_tiles = VecDeque::new();
        run2_tiles.push_back(Tile::new(0, 5));
        run2_tiles.push_back(Tile::new(0, 6));
        // Need one more - let's add a tile
        hand.add(Tile::new(0, 7));
        run2_tiles.push_back(Tile::new(0, 7));
        let run2 = Meld::new(MeldType::Run, run2_tiles);

        let solver_moves = vec![
            SolverMove::PickUp(0),
            SolverMove::LayDown(run1),
            SolverMove::LayDown(group),
            SolverMove::LayDown(run2),
        ];

        let human_moves = translate_to_human_moves(&table, &hand, &solver_moves);

        // Should produce some combination of moves
        // The exact pattern depends on analysis, but we should get something
        assert!(!human_moves.is_empty(), "Should produce at least one human move");
    }

    #[test]
    fn test_translate_empty_moves() {
        let table = Table::new();
        let hand = Hand::new();
        let solver_moves: Vec<SolverMove> = vec![];

        let human_moves = translate_to_human_moves(&table, &hand, &solver_moves);

        assert!(human_moves.is_empty());
    }

    // ========================================================================
    // Wild Debt Tests
    // ========================================================================

    #[test]
    fn test_compute_represented_tile_run() {
        // Run: [R1, Wild, R3] - wild at position 1 represents R2
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1)); // Red 1
        tiles.push_back(Tile::wild());
        tiles.push_back(Tile::new(0, 3)); // Red 3
        let meld = Meld::new(MeldType::Run, tiles);

        let represented = compute_represented_tile(&meld, 1);
        assert_eq!(represented, Some(RepresentedTile::Concrete(Tile::new(0, 2))));
    }

    #[test]
    fn test_compute_represented_tile_run_wild_at_start() {
        // Run: [Wild, R2, R3] - wild at position 0 represents R1
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::wild());
        tiles.push_back(Tile::new(0, 2)); // Red 2
        tiles.push_back(Tile::new(0, 3)); // Red 3
        let meld = Meld::new(MeldType::Run, tiles);

        let represented = compute_represented_tile(&meld, 0);
        assert_eq!(represented, Some(RepresentedTile::Concrete(Tile::new(0, 1))));
    }

    #[test]
    fn test_compute_represented_tile_run_wild_at_end() {
        // Run: [R1, R2, Wild] - wild at position 2 represents R3
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1)); // Red 1
        tiles.push_back(Tile::new(0, 2)); // Red 2
        tiles.push_back(Tile::wild());
        let meld = Meld::new(MeldType::Run, tiles);

        let represented = compute_represented_tile(&meld, 2);
        assert_eq!(represented, Some(RepresentedTile::Concrete(Tile::new(0, 3))));
    }

    #[test]
    fn test_compute_represented_tile_group_of_4() {
        // Group of 4: [R5, B5, Y5, Wild] - wild represents K5 (the missing color)
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 5)); // Red 5
        tiles.push_back(Tile::new(1, 5)); // Blue 5
        tiles.push_back(Tile::new(2, 5)); // Yellow 5
        tiles.push_back(Tile::wild());
        let meld = Meld::new(MeldType::Group, tiles);

        let represented = compute_represented_tile(&meld, 3);
        assert_eq!(represented, Some(RepresentedTile::Concrete(Tile::new(3, 5)))); // Black 5
    }

    #[test]
    fn test_compute_represented_tile_group_of_3() {
        // Group of 3: [R5, B5, Wild] - wild could be Y5 or K5
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 5)); // Red 5
        tiles.push_back(Tile::new(1, 5)); // Blue 5
        tiles.push_back(Tile::wild());
        let meld = Meld::new(MeldType::Group, tiles);

        let represented = compute_represented_tile(&meld, 2);
        match represented {
            Some(RepresentedTile::EitherOf(t1, t2)) => {
                // Should be Y5 and K5 (colors 2 and 3)
                assert!(
                    (t1 == Tile::new(2, 5) && t2 == Tile::new(3, 5)) ||
                    (t1 == Tile::new(3, 5) && t2 == Tile::new(2, 5))
                );
            }
            _ => panic!("Expected EitherOf for group of 3 with wild"),
        }
    }

    #[test]
    fn test_compute_wild_debts_single_run() {
        // Single run with wild: [R1, Wild, R3]
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::wild());
        tiles.push_back(Tile::new(0, 3));
        let meld = Meld::new(MeldType::Run, tiles);

        let picked_melds = vec![(0, meld)];
        let debt = compute_wild_debts(&picked_melds);

        assert_eq!(debt.concrete.get(&Tile::new(0, 2)), Some(&1)); // R2 is owed
        assert!(debt.either_or.is_empty());
    }

    #[test]
    fn test_compute_wild_debts_group_of_3() {
        // Group of 3 with wild: [R5, B5, Wild]
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 5));
        tiles.push_back(Tile::new(1, 5));
        tiles.push_back(Tile::wild());
        let meld = Meld::new(MeldType::Group, tiles);

        let picked_melds = vec![(0, meld)];
        let debt = compute_wild_debts(&picked_melds);

        assert!(debt.concrete.is_empty());
        assert_eq!(debt.either_or.len(), 1);
        let (t1, t2) = &debt.either_or[0];
        // Either Y5 or K5
        assert!(
            (*t1 == Tile::new(2, 5) && *t2 == Tile::new(3, 5)) ||
            (*t1 == Tile::new(3, 5) && *t2 == Tile::new(2, 5))
        );
    }

    #[test]
    fn test_is_wild_debt_satisfied_concrete() {
        // Debt: need R2
        let mut debt = WildDebt::default();
        debt.concrete.insert(Tile::new(0, 2), 1);

        // Melds that include R2
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 1));
        tiles.push_back(Tile::new(0, 2));
        tiles.push_back(Tile::new(0, 3));
        let meld = Meld::new(MeldType::Run, tiles);

        assert!(is_wild_debt_satisfied(&debt, &[meld]));
    }

    #[test]
    fn test_is_wild_debt_not_satisfied_concrete() {
        // Debt: need R2
        let mut debt = WildDebt::default();
        debt.concrete.insert(Tile::new(0, 2), 1);

        // Melds that don't include R2
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 3));
        tiles.push_back(Tile::new(0, 4));
        tiles.push_back(Tile::new(0, 5));
        let meld = Meld::new(MeldType::Run, tiles);

        assert!(!is_wild_debt_satisfied(&debt, &[meld]));
    }

    #[test]
    fn test_is_wild_debt_satisfied_either_or() {
        // Debt: need Y5 OR K5
        let mut debt = WildDebt::default();
        debt.either_or.push((Tile::new(2, 5), Tile::new(3, 5)));

        // Meld includes Y5
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 5)); // R5
        tiles.push_back(Tile::new(1, 5)); // B5
        tiles.push_back(Tile::new(2, 5)); // Y5
        let meld = Meld::new(MeldType::Group, tiles);

        assert!(is_wild_debt_satisfied(&debt, &[meld]));
    }

    #[test]
    fn test_is_wild_debt_not_satisfied_either_or() {
        // Debt: need Y5 OR K5
        let mut debt = WildDebt::default();
        debt.either_or.push((Tile::new(2, 5), Tile::new(3, 5)));

        // Meld doesn't include Y5 or K5
        let mut tiles = VecDeque::new();
        tiles.push_back(Tile::new(0, 5)); // R5
        tiles.push_back(Tile::new(1, 5)); // B5
        tiles.push_back(Tile::wild());    // Wild (doesn't count as Y5 or K5)
        let meld = Meld::new(MeldType::Group, tiles);

        assert!(!is_wild_debt_satisfied(&debt, &[meld]));
    }

    #[test]
    fn test_wild_debt_integration_with_replacement() {
        // Scenario: Table has [R1, Wild, R3], Player has [R2, B1, B2, B3]
        // Player should be able to pick up the meld and use the wild because they have R2
        let mut table = Table::new();
        let mut wild_meld_tiles = VecDeque::new();
        wild_meld_tiles.push_back(Tile::new(0, 1)); // R1
        wild_meld_tiles.push_back(Tile::wild());
        wild_meld_tiles.push_back(Tile::new(0, 3)); // R3
        table.add_meld(Meld::new(MeldType::Run, wild_meld_tiles));

        let mut hand = Hand::new();
        hand.add(Tile::new(0, 2)); // R2 - needed to replace wild
        hand.add(Tile::new(1, 1)); // B1
        hand.add(Tile::new(1, 2)); // B2
        hand.add(Tile::new(1, 3)); // B3

        let result = find_best_moves(&mut table, &mut hand, 5000);

        // Should find a solution because player has R2 to pay the wild debt
        assert!(result.moves.is_some(), "Should find a solution when replacement tile is available");
    }

    #[test]
    fn test_wild_debt_integration_no_replacement() {
        // Scenario: Table has [R1, Wild, R3], Player has [B1, B2, B3] (no R2!)
        // Player should NOT be able to use the wild from the table
        let mut table = Table::new();
        let mut wild_meld_tiles = VecDeque::new();
        wild_meld_tiles.push_back(Tile::new(0, 1)); // R1
        wild_meld_tiles.push_back(Tile::wild());
        wild_meld_tiles.push_back(Tile::new(0, 3)); // R3
        table.add_meld(Meld::new(MeldType::Run, wild_meld_tiles));

        let mut hand = Hand::new();
        hand.add(Tile::new(1, 1)); // B1
        hand.add(Tile::new(1, 2)); // B2
        hand.add(Tile::new(1, 3)); // B3

        let result = find_best_moves(&mut table, &mut hand, 5000);

        // Should still find a solution (direct play of B1,B2,B3)
        // but it should NOT involve picking up the table meld
        assert!(result.moves.is_some());
        let moves = result.moves.unwrap();

        // Verify no PickUp moves (can't use the wild without R2)
        let has_pickup = moves.iter().any(|m| matches!(m, SolverMove::PickUp(_)));

        // Note: The solver might still find depth-0 solution (direct play)
        // The key is that if there IS a pickup, the wild debt must be satisfied
        if has_pickup {
            panic!("Should not pick up meld without replacement tile");
        }
    }
}
