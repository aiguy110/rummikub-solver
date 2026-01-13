use std::collections::{BTreeMap, VecDeque};

/// A tile in Rummikub represented as a u8.
/// - Bits 0-1: Color (00 = Red, 01 = Blue, 10 = Yellow, 11 = Black)
/// - Bits 2-5: Number (1-13)
/// - All 1s (0xFF): Wild/Joker
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile(u8);

impl Tile {
    const COLOR_MASK: u8 = 0b0000_0011;
    const NUMBER_MASK: u8 = 0b0011_1100;
    const NUMBER_SHIFT: u8 = 2;
    const WILD: u8 = 0xFF;

    /// Create a new tile from color (0-3) and number (1-13)
    pub fn new(color: u8, number: u8) -> Self {
        assert!(color < 4, "Color must be 0-3");
        assert!(number >= 1 && number <= 13, "Number must be 1-13");
        Tile((number << Self::NUMBER_SHIFT) | color)
    }

    /// Create a wild/joker tile
    pub fn wild() -> Self {
        Tile(Self::WILD)
    }

    /// Get the color (0-3), or None for wild
    pub fn color(&self) -> Option<u8> {
        if self.is_wild() {
            None
        } else {
            Some(self.0 & Self::COLOR_MASK)
        }
    }

    /// Get the number (1-13), or None for wild
    pub fn number(&self) -> Option<u8> {
        if self.is_wild() {
            None
        } else {
            Some((self.0 & Self::NUMBER_MASK) >> Self::NUMBER_SHIFT)
        }
    }

    /// Check if this is a wild/joker tile
    pub fn is_wild(&self) -> bool {
        self.0 == Self::WILD
    }
}

/// Type of meld in Rummikub
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeldType {
    /// A group: same number, different colors
    Group,
    /// A run: consecutive numbers, same color
    Run,
}

/// A meld (set of tiles) on the table
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meld {
    pub meld_type: MeldType,
    pub tiles: VecDeque<Tile>,
}

impl Meld {
    /// Create a new meld
    pub fn new(meld_type: MeldType, tiles: VecDeque<Tile>) -> Self {
        Meld { meld_type, tiles }
    }
}

/// A player's hand of tiles
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hand(BTreeMap<Tile, u8>);

impl Hand {
    /// Create a new empty hand
    pub fn new() -> Self {
        Hand(BTreeMap::new())
    }

    /// Add a tile to the hand
    pub fn add(&mut self, tile: Tile) {
        *self.0.entry(tile).or_insert(0) += 1;
    }

    /// Remove a tile from the hand
    pub fn remove(&mut self, tile: &Tile) -> bool {
        if let Some(count) = self.0.get_mut(tile) {
            if *count > 0 {
                *count -= 1;
                if *count == 0 {
                    self.0.remove(tile);
                }
                return true;
            }
        }
        false
    }

    /// Get the count of a specific tile
    pub fn count(&self, tile: &Tile) -> u8 {
        self.0.get(tile).copied().unwrap_or(0)
    }
}

impl Default for Hand {
    fn default() -> Self {
        Self::new()
    }
}

/// The table state (all melds currently on the table)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table(Vec<Meld>);

impl Table {
    /// Create a new empty table
    pub fn new() -> Self {
        Table(Vec::new())
    }

    /// Add a meld to the table
    pub fn add_meld(&mut self, meld: Meld) {
        self.0.push(meld);
    }

    /// Get all melds on the table
    pub fn melds(&self) -> &[Meld] {
        &self.0
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {
    println!("Rummikub Solver");
}
