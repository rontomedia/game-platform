use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::rng::Rng;
use crate::tile::{full_set, Tile};
use crate::turn::{check_turn, TurnError};

/// The rule variants a game is played with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Rules {
    /// Tiles dealt to each player: 14, or 13 in some editions.
    pub rack_size: usize,
    /// Points a player's first sets must be worth together.
    pub initial_meld: u32,
    /// What a joker left on a rack costs at the end.
    pub joker_penalty: u32,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules { rack_size: 14, initial_meld: 30, joker_penalty: 30 }
    }
}

/// How a game ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// The player who emptied their rack. If nobody could and the pool ran
    /// out, those with the lowest rack total (several on a tie).
    pub winners: Vec<usize>,
    /// Points per player: each loser loses their rack's total, each
    /// winner gains all the losers' totals.
    pub scores: Vec<i32>,
}

/// What one player may know about a game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PlayerView {
    /// This player's seat.
    pub you: usize,
    pub rules: Rules,
    pub rack: Vec<Tile>,
    pub table: Vec<Vec<Tile>>,
    /// Tiles on each player's rack.
    pub rack_sizes: Vec<usize>,
    /// Whether each player has made their initial meld.
    pub melded: Vec<bool>,
    /// Tiles left to draw.
    pub pool: usize,
    /// Whose turn it is (after the game, whose it was).
    pub current: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub outcome: Option<Outcome>,
}

/// A game from deal to scores. Players are seats `0..players`, taking turns
/// in that order; each turn is a play or a draw.
#[derive(Clone, Debug)]
pub struct Game {
    rules: Rules,
    pool: Vec<Tile>,
    racks: Vec<Vec<Tile>>,
    table: Vec<Vec<Tile>>,
    melded: Vec<bool>,
    current: usize,
    /// Turns in a row in which the pool was empty and nobody played. A
    /// whole round of them ends the game.
    passes: usize,
    outcome: Option<Outcome>,
}

impl Game {
    /// A shuffled deal; the same seed deals the same game. Player 0 starts.
    ///
    /// Panics unless there are 2 to 4 players and tiles enough for them.
    pub fn new(players: usize, rules: Rules, seed: u64) -> Game {
        let mut pool = full_set();
        assert!((2..=4).contains(&players), "2 to 4 players, not {players}");
        assert!(players * rules.rack_size < pool.len(), "not enough tiles for {players} racks of {}", rules.rack_size);
        Rng::new(seed).shuffle(&mut pool);
        let racks = (0..players).map(|_| pool.split_off(pool.len() - rules.rack_size)).collect();
        Game::from_deal(racks, pool, rules)
    }

    fn from_deal(racks: Vec<Vec<Tile>>, pool: Vec<Tile>, rules: Rules) -> Game {
        let players = racks.len();
        Game {
            rules,
            pool,
            racks,
            table: Vec::new(),
            melded: vec![false; players],
            current: 0,
            passes: 0,
            outcome: None,
        }
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    pub fn players(&self) -> usize {
        self.racks.len()
    }

    /// Whose turn it is; after the game, whose it was.
    pub fn current(&self) -> usize {
        self.current
    }

    pub fn rack(&self, player: usize) -> &[Tile] {
        &self.racks[player]
    }

    pub fn table(&self) -> &[Vec<Tile>] {
        &self.table
    }

    pub fn melded(&self, player: usize) -> bool {
        self.melded[player]
    }

    pub fn pool_size(&self) -> usize {
        self.pool.len()
    }

    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    /// The player's turn: the table becomes `table` (see `check_turn`). A
    /// rejected turn changes nothing; the player may try again or draw.
    pub fn play(&mut self, player: usize, table: Vec<Vec<Tile>>) -> Result<(), TurnError> {
        self.check_turn_of(player)?;
        let played = check_turn(&self.table, &table, &self.racks[player], self.melded[player], &self.rules)?;
        let rack = &mut self.racks[player];
        for tile in played {
            let i = rack.iter().position(|&t| t == tile).expect("check_turn found it on the rack");
            rack.remove(i);
        }
        self.table = table;
        self.melded[player] = true;
        self.passes = 0;
        if self.racks[player].is_empty() {
            self.finish(Some(player));
        } else {
            self.next_turn();
        }
        Ok(())
    }

    /// The player's turn: they take a tile from the pool instead of playing,
    /// which ends their turn. With the pool empty this is a pass. Returns
    /// the tile drawn.
    pub fn draw(&mut self, player: usize) -> Result<Option<Tile>, TurnError> {
        self.check_turn_of(player)?;
        let tile = self.pool.pop();
        match tile {
            Some(tile) => {
                self.racks[player].push(tile);
                self.passes = 0;
            }
            None => self.passes += 1,
        }
        if self.passes >= self.players() {
            self.finish(None);
        } else {
            self.next_turn();
        }
        Ok(tile)
    }

    /// The game as `player` sees it: other racks only by size, the pool
    /// only by count.
    pub fn view(&self, player: usize) -> PlayerView {
        PlayerView {
            you: player,
            rules: self.rules.clone(),
            rack: self.racks[player].clone(),
            table: self.table.clone(),
            rack_sizes: self.racks.iter().map(Vec::len).collect(),
            melded: self.melded.clone(),
            pool: self.pool.len(),
            current: self.current,
            outcome: self.outcome.clone(),
        }
    }

    fn check_turn_of(&self, player: usize) -> Result<(), TurnError> {
        if self.outcome.is_some() {
            Err(TurnError::GameOver)
        } else if player != self.current {
            Err(TurnError::NotYourTurn)
        } else {
            Ok(())
        }
    }

    fn next_turn(&mut self) {
        self.current = (self.current + 1) % self.players();
    }

    /// Ends the game; `went_out` is the player who emptied their rack.
    fn finish(&mut self, went_out: Option<usize>) {
        let totals: Vec<i32> = self
            .racks
            .iter()
            .map(|rack| rack.iter().map(|t| t.penalty(self.rules.joker_penalty) as i32).sum())
            .collect();
        let winners: Vec<usize> = match went_out {
            Some(player) => vec![player],
            None => {
                let lowest = *totals.iter().min().expect("there are players");
                (0..totals.len()).filter(|&p| totals[p] == lowest).collect()
            }
        };
        let lost: i32 = (0..totals.len()).filter(|p| !winners.contains(p)).map(|p| totals[p]).sum();
        let scores = (0..totals.len()).map(|p| if winners.contains(&p) { lost } else { -totals[p] }).collect();
        self.outcome = Some(Outcome { winners, scores });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set::{classify, points};
    use crate::tile::Color::{self, *};

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }

    /// Every tile is in exactly one place.
    fn assert_all_tiles_present(game: &Game) {
        let mut tiles: Vec<Tile> =
            game.pool.iter().chain(game.racks.iter().flatten()).chain(game.table.iter().flatten()).copied().collect();
        let mut all = full_set();
        let key = |t: &Tile| format!("{t:?}");
        tiles.sort_by_key(key);
        all.sort_by_key(key);
        assert_eq!(tiles, all);
    }

    #[test]
    fn deal() {
        let game = Game::new(4, Rules::default(), 7);
        assert!((0..4).all(|p| game.rack(p).len() == 14));
        assert_eq!(game.pool_size(), 106 - 4 * 14);
        assert_eq!(game.current(), 0);
        assert_all_tiles_present(&game);

        let short = Game::new(2, Rules { rack_size: 13, ..Rules::default() }, 7);
        assert!((0..2).all(|p| short.rack(p).len() == 13));
    }

    #[test]
    fn same_seed_same_deal() {
        let a = Game::new(3, Rules::default(), 42);
        let b = Game::new(3, Rules::default(), 42);
        let c = Game::new(3, Rules::default(), 43);
        assert_eq!(a.rack(0), b.rack(0));
        assert_eq!(a.pool, b.pool);
        assert_ne!(a.rack(0), c.rack(0));
    }

    #[test]
    #[should_panic]
    fn five_players_is_too_many() {
        Game::new(5, Rules::default(), 1);
    }

    #[test]
    fn drawing_takes_a_tile_and_passes_the_turn() {
        let mut game = Game::new(2, Rules::default(), 1);
        assert_eq!(game.draw(1), Err(TurnError::NotYourTurn));
        let tile = game.draw(0).unwrap().unwrap();
        assert_eq!(game.rack(0).len(), 15);
        assert_eq!(*game.rack(0).last().unwrap(), tile);
        assert_eq!(game.pool_size(), 106 - 2 * 14 - 1);
        assert_eq!(game.current(), 1);
        assert_all_tiles_present(&game);
    }

    #[test]
    fn a_rejected_play_changes_nothing() {
        let mut game = Game::from_deal(
            vec![vec![n(Red, 1), n(Red, 2), n(Red, 3), n(Blue, 9)], vec![n(Black, 5)]],
            vec![n(Orange, 1)],
            Rules::default(),
        );
        let low = vec![vec![n(Red, 1), n(Red, 2), n(Red, 3)]];
        assert_eq!(game.play(0, low), Err(TurnError::InitialMeldTooLow { points: 6, needed: 30 }));
        assert_eq!(game.rack(0).len(), 4);
        assert!(game.table().is_empty());
        assert_eq!(game.current(), 0, "still their turn");
    }

    #[test]
    fn a_blocked_game_ends_with_the_lowest_rack() {
        let mut game = Game::from_deal(
            vec![
                vec![n(Red, 10), n(Red, 11), n(Red, 12), n(Black, 13)],
                vec![n(Blue, 5), Tile::Joker],
                vec![n(Orange, 2), n(Black, 1)],
            ],
            vec![n(Orange, 9)],
            Rules::default(),
        );
        game.play(0, vec![vec![n(Red, 10), n(Red, 11), n(Red, 12)]]).unwrap();
        assert!(game.melded(0));
        game.draw(1).unwrap(); // the last tile in the pool
        game.draw(2).unwrap(); // a pass: the pool is empty
        let table = vec![vec![n(Red, 10), n(Red, 11), n(Red, 12), n(Black, 13)]];
        assert_eq!(game.play(0, table), Err(TurnError::InvalidSet { index: 0 }));
        // The black 13 can't go anywhere: player 0 passes too, then 1.
        game.draw(0).unwrap();
        assert!(game.outcome().is_none());
        game.draw(1).unwrap();
        // A whole round of passes with the pool empty: the lowest rack wins.
        // Totals: 13; 5 + 30 (joker) + 9; 2 + 1.
        let outcome = game.outcome().unwrap();
        assert_eq!(outcome.winners, vec![2]);
        assert_eq!(outcome.scores, vec![-13, -44, 57]);
        assert_eq!(game.draw(2), Err(TurnError::GameOver));
    }

    #[test]
    fn going_out() {
        let mut game = Game::from_deal(
            vec![vec![n(Red, 10), n(Red, 11), n(Red, 12)], vec![n(Blue, 5), Tile::Joker]],
            vec![n(Orange, 9)],
            Rules::default(),
        );
        game.play(0, vec![vec![n(Red, 10), n(Red, 11), n(Red, 12)]]).unwrap();
        let outcome = game.outcome().unwrap();
        assert_eq!(outcome.winners, vec![0]);
        assert_eq!(outcome.scores, vec![35, -35]);
        assert_eq!(game.play(1, vec![]), Err(TurnError::GameOver));
    }

    #[test]
    fn a_tie_has_several_winners() {
        let mut game =
            Game::from_deal(vec![vec![n(Red, 4)], vec![n(Blue, 4)], vec![n(Black, 9)]], vec![], Rules::default());
        for p in 0..3 {
            game.draw(p).unwrap();
        }
        let outcome = game.outcome().unwrap();
        assert_eq!(outcome.winners, vec![0, 1]);
        assert_eq!(outcome.scores, vec![9, 9, -9]);
    }

    #[test]
    fn a_view_hides_other_racks_and_the_pool() {
        let mut game = Game::new(3, Rules::default(), 5);
        game.draw(0).unwrap();
        let view = game.view(1);
        assert_eq!(view.you, 1);
        assert_eq!(view.rack, game.rack(1));
        assert_eq!(view.rack_sizes, vec![15, 14, 14]);
        assert_eq!(view.pool, 106 - 3 * 14 - 1);
        assert_eq!(view.current, 1);
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains("\"rackSizes\":[15,14,14]"), "{json}");
        assert!(!json.contains("outcome"), "no outcome while playing: {json}");
    }

    /// A naive player: puts down any valid sets of three it can find on its
    /// rack (enough for the initial meld, or any once melded) and, once
    /// melded, adds single tiles to either end of sets on the table. Else
    /// draws.
    fn naive_turn(game: &mut Game) {
        let player = game.current();
        let mut rack = game.rack(player).to_vec();
        let mut sets = Vec::new();
        'search: loop {
            for i in 0..rack.len() {
                for j in 0..rack.len() {
                    for k in 0..rack.len() {
                        if i == j || j == k || i == k {
                            continue;
                        }
                        let set = vec![rack[i], rack[j], rack[k]];
                        if classify(&set).is_some() {
                            let mut picked = [i, j, k];
                            picked.sort_unstable();
                            for p in picked.into_iter().rev() {
                                rack.remove(p);
                            }
                            sets.push(set);
                            continue 'search;
                        }
                    }
                }
            }
            break;
        }
        let worth: u32 = sets.iter().map(|s| points(s).unwrap()).sum();
        let melded = game.melded(player);
        if !melded && worth < game.rules().initial_meld {
            sets.clear();
        }
        let mut table = game.table().to_vec();
        let mut changed = !sets.is_empty();
        table.extend(sets);
        if melded {
            rack.retain(|&tile| {
                for set in table.iter_mut() {
                    for at in [0, set.len()] {
                        set.insert(at, tile);
                        if classify(set).is_some() {
                            changed = true;
                            return false;
                        }
                        set.remove(at);
                    }
                }
                true
            });
        }
        if changed {
            game.play(player, table).expect("the naive player only makes valid plays");
        } else {
            game.draw(player).unwrap();
        }
    }

    #[test]
    fn whole_games_end_and_keep_every_tile() {
        let mut endings = [0; 2];
        for seed in 0..40 {
            let mut game = Game::new(2 + seed as usize % 3, Rules::default(), seed);
            let mut turns = 0;
            while game.outcome().is_none() {
                naive_turn(&mut game);
                assert_all_tiles_present(&game);
                assert!(game.table().iter().all(|s| classify(s).is_some()));
                turns += 1;
                assert!(turns < 1000, "seed {seed} doesn't end");
            }
            let outcome = game.outcome().unwrap();
            let went_out = (0..game.players()).any(|p| game.rack(p).is_empty());
            endings[went_out as usize] += 1;
            if went_out {
                assert_eq!(outcome.scores.iter().sum::<i32>(), 0, "zero-sum when someone goes out");
            }
        }
        // Both endings happen, so both are exercised.
        assert!(endings[0] > 0 && endings[1] > 0, "{endings:?}");
    }
}
