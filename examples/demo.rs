use rummikub_solver::{Hand, Tile};

fn main() {
    println!("Rummikub Solver\n");

    // Example: Find best melds from a hand
    let mut hand = Hand::new();
    hand.add(Tile::new(0, 1)); // Red 1
    hand.add(Tile::new(0, 2)); // Red 2
    hand.add(Tile::new(0, 3)); // Red 3
    hand.add(Tile::new(1, 5)); // Blue 5
    hand.add(Tile::new(2, 5)); // Yellow 5
    hand.add(Tile::new(3, 5)); // Black 5
    hand.add(Tile::wild());     // Joker

    println!("Initial hand:");
    for (tile, count) in hand.iter() {
        if tile.is_wild() {
            println!("  Wild x{}", count);
        } else {
            println!(
                "  Color {} Number {} x{}",
                tile.color().unwrap(),
                tile.number().unwrap(),
                count
            );
        }
    }

    let hand_to_beat = hand.clone();

    // Quality function: prefer hands with fewer total tiles
    let quality = |h: &Hand| {
        let total: i32 = h.iter().map(|(_, &c)| c as i32).sum();
        -total
    };

    println!("\nSearching for best meld combinations...");
    if let Some(melds) = rummikub_solver::solver::find_best_melds(&mut hand, quality, &hand_to_beat) {
        println!("Found {} melds:", melds.len());
        for (i, meld) in melds.iter().enumerate() {
            print!("  Meld {}: {:?} [", i + 1, meld.meld_type);
            for (j, tile) in meld.tiles.iter().enumerate() {
                if j > 0 {
                    print!(", ");
                }
                if tile.is_wild() {
                    print!("Wild");
                } else {
                    print!("C{}N{}", tile.color().unwrap(), tile.number().unwrap());
                }
            }
            println!("]");
        }

        // Apply the melds to see the remaining hand
        for meld in &melds {
            for tile in &meld.tiles {
                hand.remove(tile);
            }
        }

        println!("\nRemaining hand:");
        if hand.iter().count() == 0 {
            println!("  (empty - played all tiles!)");
        } else {
            for (tile, count) in hand.iter() {
                if tile.is_wild() {
                    println!("  Wild x{}", count);
                } else {
                    println!(
                        "  Color {} Number {} x{}",
                        tile.color().unwrap(),
                        tile.number().unwrap(),
                        count
                    );
                }
            }
        }
    } else {
        println!("No valid meld combinations found.");
    }
}
