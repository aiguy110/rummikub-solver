use crate::{Hand, Meld, MeldType, Table, Tile, solver};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

/// Initialize panic hook for better error messages in the browser console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// JSON-serializable representation of a meld
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MeldJson {
    #[serde(rename = "group")]
    Group { tiles: Vec<String> },
    #[serde(rename = "run")]
    Run { tiles: Vec<String> },
}

/// JSON-serializable representation of a solver move
#[derive(Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum MoveJson {
    #[serde(rename = "pickup")]
    PickUp { index: usize },
    #[serde(rename = "laydown")]
    LayDown { meld: MeldJson },
}

/// Result of the solver operation
#[derive(Serialize, Deserialize)]
pub struct SolverResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moves: Option<Vec<MoveJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Whether the search completed fully (true) or timed out (false)
    pub search_completed: bool,
    /// Maximum depth explored during the search
    pub depth_reached: usize,
    /// Initial hand quality before solving
    pub initial_quality: i32,
    /// Final hand quality after applying the solution
    pub final_quality: i32,
}

/// Main WASM API: Solve a Rummikub game state
///
/// # Arguments
/// * `hand_tiles` - JSON array of tile strings (e.g., ["r1", "b5", "w"])
/// * `table_melds` - JSON array of meld objects (e.g., [{"type": "run", "tiles": ["r1", "r2", "r3"]}])
/// * `strategy` - Scoring strategy: "minimize_tiles" or "minimize_points"
/// * `time_limit_ms` - Maximum time to search in milliseconds
///
/// # Returns
/// JSON string with SolverResult containing success, moves, or error
#[wasm_bindgen]
pub fn solve_rummikub(
    hand_tiles: &str,
    table_melds: &str,
    strategy: &str,
    time_limit_ms: u64,
) -> String {
    match solve_internal(hand_tiles, table_melds, strategy, time_limit_ms) {
        Ok(result) => serde_json::to_string(&result)
            .unwrap_or_else(|e| format!(r#"{{"success":false,"error":"Serialization error: {}"}}"#, e)),
        Err(e) => serde_json::to_string(&SolverResult {
            success: false,
            moves: None,
            error: Some(e),
            search_completed: false,
            depth_reached: 0,
            initial_quality: 0,
            final_quality: 0,
        })
        .unwrap_or_else(|e| format!(r#"{{"success":false,"error":"Serialization error: {}"}}"#, e)),
    }
}

/// Internal implementation of solve_rummikub
fn solve_internal(
    hand_tiles: &str,
    table_melds: &str,
    strategy_str: &str,
    time_limit_ms: u64,
) -> Result<SolverResult, String> {
    // 1. Parse hand_tiles JSON into Vec<String>
    let hand_strs: Vec<String> =
        serde_json::from_str(hand_tiles).map_err(|e| format!("Invalid hand JSON: {}", e))?;

    // 2. Parse each tile string into Tile
    let mut hand = Hand::new();
    for tile_str in hand_strs {
        let tile = Tile::from_string(&tile_str)?;
        hand.add(tile);
    }

    // 3. Parse table_melds JSON
    let table_json: Vec<MeldJson> =
        serde_json::from_str(table_melds).map_err(|e| format!("Invalid table JSON: {}", e))?;

    let mut table = Table::new();
    for meld_json in table_json {
        let meld = meld_from_json(meld_json)?;
        table.add_meld(meld);
    }

    // 4. Parse strategy
    let strategy = match strategy_str {
        "minimize_tiles" => solver::ScoringStrategy::MinimizeTiles,
        "minimize_points" => solver::ScoringStrategy::MinimizePoints,
        _ => return Err(format!("Unknown strategy: {}", strategy_str)),
    };

    // 5. Call solver with strategy
    let solver_result =
        solver::find_best_moves_with_strategy(&mut table, &mut hand, time_limit_ms, strategy);

    // 6. Convert result to JSON
    let moves_json = solver_result.moves.as_ref().map(|moves| {
        moves.iter().map(|m| move_to_json(m.clone())).collect()
    });

    Ok(SolverResult {
        success: solver_result.moves.is_some(),
        moves: moves_json,
        error: if solver_result.moves.is_none() {
            Some("No solution found within time limit".to_string())
        } else {
            None
        },
        search_completed: solver_result.search_completed,
        depth_reached: solver_result.depth_reached,
        initial_quality: solver_result.initial_quality,
        final_quality: solver_result.final_quality,
    })
}

/// Convert JSON meld to internal Meld type
fn meld_from_json(meld_json: MeldJson) -> Result<Meld, String> {
    let (meld_type, tile_strs) = match meld_json {
        MeldJson::Group { tiles } => (MeldType::Group, tiles),
        MeldJson::Run { tiles } => (MeldType::Run, tiles),
    };

    let mut tiles = VecDeque::new();
    for tile_str in tile_strs {
        tiles.push_back(Tile::from_string(&tile_str)?);
    }

    Ok(Meld::new(meld_type, tiles))
}

/// Convert internal Meld to JSON representation
fn meld_to_json(meld: &Meld) -> MeldJson {
    let tiles: Vec<String> = meld.tiles.iter().map(|t| t.to_string()).collect();

    match meld.meld_type {
        MeldType::Group => MeldJson::Group { tiles },
        MeldType::Run => MeldJson::Run { tiles },
    }
}

/// Convert internal SolverMove to JSON representation
fn move_to_json(solver_move: solver::SolverMove) -> MoveJson {
    match solver_move {
        solver::SolverMove::PickUp(index) => MoveJson::PickUp { index },
        solver::SolverMove::LayDown(meld) => MoveJson::LayDown {
            meld: meld_to_json(&meld),
        },
    }
}
