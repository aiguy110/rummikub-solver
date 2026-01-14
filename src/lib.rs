use std::collections::{BTreeMap, VecDeque};

pub mod solver;
#[cfg(target_arch = "wasm32")]
pub mod wasm_api;

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

    /// Parse a tile from a string representation
    /// Format: "r13" (red 13), "b1" (blue 1), "y7" (yellow 7), "k9" (black 9), "w" (wild)
    pub fn from_string(s: &str) -> Result<Self, String> {
        if s == "w" {
            return Ok(Tile::wild());
        }
        if s.len() < 2 {
            return Err(format!("Invalid tile string: {}", s));
        }

        let color = match &s[0..1] {
            "r" => 0,
            "b" => 1,
            "y" => 2,
            "k" => 3,
            _ => return Err(format!("Invalid color: {}", &s[0..1])),
        };

        let number: u8 = s[1..].parse()
            .map_err(|_| format!("Invalid number: {}", &s[1..]))?;

        if !(1..=13).contains(&number) {
            return Err(format!("Number must be 1-13, got {}", number));
        }

        Ok(Tile::new(color, number))
    }

    /// Convert tile to string representation
    /// Returns: "r13" (red 13), "b1" (blue 1), etc., or "w" for wild
    pub fn to_string(&self) -> String {
        if self.is_wild() {
            return "w".to_string();
        }
        let color_char = match self.color() {
            Some(0) => 'r',
            Some(1) => 'b',
            Some(2) => 'y',
            Some(3) => 'k',
            None => {
                // This shouldn't happen - we already checked is_wild()
                eprintln!("ERROR: Tile has no color but is not wild. Raw value: 0x{:02x}", self.0);
                return format!("?{}", self.0);
            }
            Some(c) => {
                // This is mathematically impossible since color is 2 bits
                eprintln!("ERROR: Invalid color value: {}. Raw tile value: 0x{:02x}", c, self.0);
                return format!("?{}", self.0);
            }
        };
        match self.number() {
            Some(n) => format!("{}{}", color_char, n),
            None => {
                // Shouldn't happen for non-wild tiles
                eprintln!("ERROR: Non-wild tile has no number. Raw value: 0x{:02x}", self.0);
                format!("?{}", self.0)
            }
        }
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

    /// Parse a meld from a string, auto-detecting type
    /// Formats:
    /// - Group: "5 r b k" (number followed by color letters)
    /// - Run: "y 6 7 8" (color letter followed by numbers)
    pub fn from_string(input: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = input.split_whitespace().collect();
        if tokens.is_empty() {
            return Err("Empty meld string".to_string());
        }

        // Check if first token is a digit (1-13) → Group
        // Otherwise should be a color letter → Run
        if let Ok(num) = tokens[0].parse::<u8>() {
            if (1..=13).contains(&num) {
                return Self::from_group_string(input);
            }
        }

        // Check if first token is a valid color
        if matches!(tokens[0], "r" | "b" | "y" | "k") {
            return Self::from_run_string(input);
        }

        Err(format!(
            "Invalid meld format: '{}'. Use 'N c1 c2 c3' for group or 'C n1 n2 n3' for run",
            input
        ))
    }

    /// Parse a group meld: "5 r b k" (number followed by color letters)
    pub fn from_group_string(input: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = input.split_whitespace().collect();
        if tokens.len() < 4 {
            return Err(format!(
                "Group must have at least 4 tokens (number + 3 colors), got: {}",
                tokens.len()
            ));
        }

        // Parse number
        let number: u8 = tokens[0]
            .parse()
            .map_err(|_| format!("Invalid number: {}", tokens[0]))?;

        if !(1..=13).contains(&number) {
            return Err(format!("Number must be 1-13, got {}", number));
        }

        // Parse colors and create tiles
        let mut tiles = VecDeque::new();
        for color_char in &tokens[1..] {
            let color = match *color_char {
                "r" => 0,
                "b" => 1,
                "y" => 2,
                "k" => 3,
                "w" => {
                    // Allow wildcard in groups
                    tiles.push_back(Tile::wild());
                    continue;
                }
                _ => return Err(format!("Invalid color: {}", color_char)),
            };
            tiles.push_back(Tile::new(color, number));
        }

        if tiles.len() < 3 {
            return Err(format!("Group must have at least 3 tiles, got {}", tiles.len()));
        }

        Ok(Meld::new(MeldType::Group, tiles))
    }

    /// Parse a run meld: "y 6 7 8" (color letter followed by numbers)
    pub fn from_run_string(input: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = input.split_whitespace().collect();
        if tokens.len() < 4 {
            return Err(format!(
                "Run must have at least 4 tokens (color + 3 numbers), got: {}",
                tokens.len()
            ));
        }

        // Parse color
        let color = match tokens[0] {
            "r" => 0,
            "b" => 1,
            "y" => 2,
            "k" => 3,
            "w" => {
                return Err("Wildcard cannot be the starting color of a run".to_string());
            }
            _ => return Err(format!("Invalid color: {}", tokens[0])),
        };

        // Parse numbers and create tiles
        let mut tiles = VecDeque::new();
        for num_str in &tokens[1..] {
            if *num_str == "w" {
                tiles.push_back(Tile::wild());
            } else {
                let number: u8 = num_str
                    .parse()
                    .map_err(|_| format!("Invalid number: {}", num_str))?;

                if !(1..=13).contains(&number) {
                    return Err(format!("Number must be 1-13, got {}", number));
                }

                tiles.push_back(Tile::new(color, number));
            }
        }

        if tiles.len() < 3 {
            return Err(format!("Run must have at least 3 tiles, got {}", tiles.len()));
        }

        Ok(Meld::new(MeldType::Run, tiles))
    }
}

/// A player's hand of tiles
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hand(pub BTreeMap<Tile, u8>);

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

    /// Get an iterator over all tile types and their counts
    pub fn iter(&self) -> impl Iterator<Item = (&Tile, &u8)> {
        self.0.iter()
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

    /// Remove and return a meld at the given index
    pub fn remove_meld(&mut self, index: usize) -> Option<Meld> {
        if index < self.0.len() {
            Some(self.0.remove(index))
        } else {
            None
        }
    }

    /// Insert a meld at the given index
    pub fn insert_meld(&mut self, index: usize, meld: Meld) {
        self.0.insert(index, meld);
    }

    /// Get the number of melds on the table
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_from_string() {
        assert_eq!(Tile::from_string("r13").unwrap(), Tile::new(0, 13));
        assert_eq!(Tile::from_string("b1").unwrap(), Tile::new(1, 1));
        assert_eq!(Tile::from_string("y7").unwrap(), Tile::new(2, 7));
        assert_eq!(Tile::from_string("k9").unwrap(), Tile::new(3, 9));
        assert_eq!(Tile::from_string("w").unwrap(), Tile::wild());

        // Test invalid inputs
        assert!(Tile::from_string("x5").is_err());
        assert!(Tile::from_string("r14").is_err());
        assert!(Tile::from_string("r0").is_err());
        assert!(Tile::from_string("").is_err());
        assert!(Tile::from_string("r").is_err());
    }

    #[test]
    fn test_tile_to_string() {
        assert_eq!(Tile::new(0, 13).to_string(), "r13");
        assert_eq!(Tile::new(1, 1).to_string(), "b1");
        assert_eq!(Tile::new(2, 7).to_string(), "y7");
        assert_eq!(Tile::new(3, 9).to_string(), "k9");
        assert_eq!(Tile::wild().to_string(), "w");
    }

    #[test]
    fn test_tile_roundtrip() {
        let tiles = vec![
            Tile::new(0, 1),
            Tile::new(1, 13),
            Tile::new(2, 7),
            Tile::new(3, 3),
            Tile::wild(),
        ];

        for tile in tiles {
            let s = tile.to_string();
            let parsed = Tile::from_string(&s).unwrap();
            assert_eq!(tile, parsed);
        }
    }
}
