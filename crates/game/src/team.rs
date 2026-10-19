//! The player's team as the overworld's Team button shows it: the creatures they fought with
//! most in their last battles first, and before all of them the one they pinned as their main.
//! There's always a main (while there's a team), and it's the one sent out first in a battle.
//! The board of cards (View all) has them all, the main first, then as the list has them, or
//! as the player sorts and filters them. Each has lifeforce, a gauge (the world says its top,
//! 100): each time it's knocked out in a battle it loses its level's worth (up to the world's
//! cap, 20), and with none left it's lost for good: it fights no more and isn't the main.
//! The story keeps who fought in each of the last battles, and the main (`SaveGame.team`).

use std::collections::HashMap;

use rules::battle::{Battle, Side};
use rules::mon::MonSpec;

/// The rules' own test creatures (crates/core/tests/fixture), for a battle in the tests.
#[cfg(test)]
#[path = "../../core/tests/fixture/mod.rs"]
mod fixture;

/// How many of the last battles count.
pub const RECENT: usize = 10;
/// What a knock-out costs a creature of `level`: its level's worth of lifeforce, up to `cap`
/// (the world's: 20, so from level 20 on it's 20 a knock-out).
pub fn ko_cost(level: u8, cap: u8) -> u8 {
    level.min(cap)
}

/// How many creatures the Team's list shows (the board's first row is them too).
pub const SHOWN: usize = 5;

/// How the board of cards (View all) is put in order, after the main (always first).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    /// as the Team's list has them: who fought in the most of the last battles first
    #[default]
    MostUsed,
    /// the highest first
    Level,
    /// from A
    Name,
    /// the most first
    Hp,
    /// their number in the creature index, from the first
    Number,
}

impl Sort {
    pub const ALL: [Sort; 5] = [Sort::MostUsed, Sort::Level, Sort::Name, Sort::Hp, Sort::Number];

    pub fn label(self) -> &'static str {
        match self {
            Sort::MostUsed => "Most used",
            Sort::Level => "Level",
            Sort::Name => "Name",
            Sort::Hp => "HP",
            Sort::Number => "No.",
        }
    }
}

/// What the story keeps of the team: who fought in each of the last battles (the latest last,
/// `RECENT` at most), the main, and the lifeforce of those that lost some.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TeamLog {
    pub battles: Vec<Vec<String>>,
    /// the creature pinned first, by its id (`ids`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    /// how much lifeforce those that lost some have left, by their ids (the others have all
    /// of theirs): a plain number on the world's gauge, so a rest can give some back; none
    /// left: lost for good
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub lifeforce: std::collections::BTreeMap<String, u8>,
}

/// Each of the team's creatures' id: its species, then its place among those of its species
/// after the first ("sproutle", "sproutle#2").
pub fn ids(party: &[MonSpec]) -> Vec<String> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    party
        .iter()
        .map(|m| {
            let n = seen.entry(m.species.as_str()).or_insert(0);
            *n += 1;
            if *n == 1 {
                m.species.clone()
            } else {
                format!("{}#{n}", m.species)
            }
        })
        .collect()
}

/// Who of the player's team took part in a battle: each creature that came out on the field,
/// once, in the team's order (their ids).
pub fn fought(battle: &Battle, ids: &[String]) -> Vec<String> {
    let mut places: Vec<usize> = battle.battlers.iter().filter(|b| b.side == Side::Player).map(|b| b.mon).collect();
    places.sort_unstable();
    places.dedup();
    places.into_iter().filter_map(|i| ids.get(i).cloned()).collect()
}

impl TeamLog {
    /// A battle fought: who of the team took part (`fought`). The oldest of more than
    /// `RECENT` is forgotten.
    pub fn record(&mut self, fought: Vec<String>) {
        if fought.is_empty() {
            return;
        }
        self.battles.push(fought);
        let over = self.battles.len().saturating_sub(RECENT);
        self.battles.drain(..over);
    }

    /// `id` pinned as the main (the one before it is unpinned). There's always a main: pinned
    /// again, it stays. One lost for good can't be.
    pub fn pin(&mut self, id: &str) {
        if !self.is_lost(id) {
            self.main = Some(id.to_string());
        }
    }

    /// How much lifeforce `id` has left, of `max`.
    pub fn lifeforce(&self, id: &str, max: u8) -> u8 {
        self.lifeforce.get(id).copied().unwrap_or(max).min(max)
    }

    /// `id` has no lifeforce left: lost for good.
    pub fn is_lost(&self, id: &str) -> bool {
        self.lifeforce.get(id) == Some(&0)
    }

    /// `id` was knocked out in a battle: `cost` less lifeforce (of `max`, if it had lost none).
    pub fn knocked_out(&mut self, id: &str, max: u8, cost: u8) {
        let left = self.lifeforce(id, max).saturating_sub(cost);
        self.lifeforce.insert(id.to_string(), left);
    }

    /// The main's place in the team (`ids`): the one pinned; while none is (or it's no longer
    /// in the team, or it's lost), the one still alive that fought most lately: the team's
    /// first before any battle. None for no team, or none alive.
    pub fn main_of(&self, ids: &[String]) -> Option<usize> {
        let alive = |i: &usize| !self.is_lost(&ids[*i]);
        self.main.as_ref().and_then(|m| ids.iter().position(|i| i == m)).filter(alive).or_else(|| self.ranked(ids, None).into_iter().find(alive))
    }

    /// The main the team has (`main_of`) pinned for good, so the story keeps it: the one that
    /// fought most lately stays the main once it is, until another is pinned.
    pub fn settle(&mut self, ids: &[String]) {
        if let Some(i) = self.main_of(ids) {
            if self.main.as_deref() != Some(ids[i].as_str()) {
                self.main = Some(ids[i].clone());
            }
        }
    }

    /// The team in the order it goes into a battle (their places in `ids`): the main first,
    /// sent out first, then the others in the team's order; none lost for good.
    pub fn battle_order(&self, ids: &[String]) -> Vec<usize> {
        let main = self.main_of(ids);
        main.into_iter().chain((0..ids.len()).filter(|&i| Some(i) != main && !self.is_lost(&ids[i]))).collect()
    }

    /// In how many of the last battles `id` fought.
    pub fn uses(&self, id: &str) -> usize {
        self.battles.iter().filter(|b| b.iter().any(|f| f == id)).count()
    }

    /// The team's creatures to show (their places in `ids`), `n` at most: the main first, then
    /// the ones that fought in the most of the last battles (the more recently, the sooner;
    /// then the team's order), then the others in the team's order.
    pub fn most_used(&self, ids: &[String], n: usize) -> Vec<usize> {
        let main = self.main_of(ids);
        main.into_iter().chain(self.ranked(ids, main)).take(n).collect()
    }

    /// The board of cards' order (places in `ids`): the main first, always (whatever the
    /// filter says), then the ones `keep` keeps in `order`'s order (ties, and an order that
    /// says nothing, as the Team's list has them); `reverse`: those the other way round.
    pub fn board(&self, ids: &[String], keep: impl Fn(usize) -> bool, order: impl Fn(usize, usize) -> std::cmp::Ordering, reverse: bool) -> Vec<usize> {
        let main = self.main_of(ids);
        let mut rest: Vec<usize> = self.ranked(ids, main).into_iter().filter(|&i| keep(i)).collect();
        rest.sort_by(|&a, &b| order(a, b));
        if reverse {
            rest.reverse();
        }
        main.into_iter().chain(rest).collect()
    }

    /// The team (their places in `ids`) but `except`, the ones that fought in the most of the
    /// last battles first (the more recently, the sooner; then the team's order).
    fn ranked(&self, ids: &[String], except: Option<usize>) -> Vec<usize> {
        let mut rest: Vec<(usize, usize, usize)> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != except)
            .map(|(i, id)| (i, self.uses(id), self.battles.iter().rposition(|b| b.iter().any(|f| f == id)).map_or(0, |p| p + 1)))
            .collect();
        rest.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
        rest.into_iter().map(|r| r.0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(species: &[&str]) -> Vec<MonSpec> {
        species.iter().map(|s| MonSpec::new(s, 5)).collect()
    }

    #[test]
    fn the_same_species_twice_is_told_apart() {
        assert_eq!(ids(&team(&["sproutle", "emberkit", "sproutle", "sproutle"])), ["sproutle", "emberkit", "sproutle#2", "sproutle#3"]);
    }

    #[test]
    fn the_most_used_come_first_and_the_main_before_them() {
        let ids: Vec<String> = ["a", "b", "c", "d", "e", "f"].iter().map(|s| s.to_string()).collect();
        let mut log = TeamLog::default();
        // no battles yet: the team's order
        assert_eq!(log.most_used(&ids, SHOWN), [0, 1, 2, 3, 4]);
        // f fought three times, c twice (c the latest), e twice (earlier)
        for fought in [vec!["f", "e"], vec!["f", "e"], vec!["f"], vec!["c"], vec!["c"]] {
            log.record(fought.into_iter().map(String::from).collect());
        }
        assert_eq!(log.most_used(&ids, SHOWN), [5, 2, 4, 0, 1]);
        assert_eq!((log.uses("f"), log.uses("a")), (3, 0));
        // b pinned: first, though it never fought
        log.pin("b");
        assert_eq!(log.most_used(&ids, SHOWN), [1, 5, 2, 4, 0]);
        log.pin("d");
        assert_eq!(log.main.as_deref(), Some("d"), "pinning another moves the pin");
        assert_eq!(log.most_used(&ids, SHOWN)[0], 3);
        log.pin("d");
        assert_eq!(log.main.as_deref(), Some("d"), "pinned again: still the main");
        // a main no longer in the team: the one that fought most is
        log.pin("gone");
        assert_eq!(log.most_used(&ids, SHOWN), [5, 2, 4, 0, 1]);
        // fewer creatures than places: all of them
        assert_eq!(log.most_used(&ids[..2], SHOWN), [0, 1]);
    }

    /// There's always a main while there's a team: the team's first before any battle, else
    /// the one that fought most lately, pinned for good once settled; it goes into a battle
    /// first, the others after it in the team's order.
    #[test]
    fn there_is_always_a_main_and_it_goes_first() {
        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let mut log = TeamLog::default();
        assert_eq!((log.main_of(&ids), log.main_of(&[])), (Some(0), None), "the team's first; no team, no main");
        assert_eq!(log.battle_order(&ids), [0, 1, 2, 3]);
        log.settle(&ids);
        assert_eq!(log.main.as_deref(), Some("a"), "settled: the story keeps it");
        log.record(vec!["c".into()]);
        log.settle(&ids);
        assert_eq!(log.main.as_deref(), Some("a"), "settled, it stays the main though c fought since");
        log.pin("c");
        assert_eq!(log.battle_order(&ids), [2, 0, 1, 3], "the main first, then the team's order");
        // the main gone from the team: the one that fought most lately, pinned once settled
        log.main = Some("gone".into());
        log.record(vec!["d".into()]);
        assert_eq!(log.main_of(&ids), Some(3));
        log.settle(&ids);
        assert_eq!(log.main.as_deref(), Some("d"));
        let mut none = TeamLog::default();
        none.settle(&[]);
        assert!(none.main.is_none() && none.battle_order(&[]).is_empty());
    }

    /// The board: the main first, then as the list has them (its first row the list's five);
    /// sorted, the order asked for, ties as the list has them; reversed; filtered, the main
    /// kept first all the same.
    #[test]
    fn the_board_has_the_main_first_whatever_the_order() {
        let ids: Vec<String> = ["a", "b", "c", "d", "e", "f", "g"].iter().map(|s| s.to_string()).collect();
        let levels = [5, 30, 12, 30, 8, 50, 1];
        let mut log = TeamLog::default();
        for fought in [vec!["f"], vec!["f", "c"], vec!["c"], vec!["g"]] {
            log.record(fought.into_iter().map(String::from).collect());
        }
        log.pin("b");
        let same = |_: usize, _: usize| std::cmp::Ordering::Equal;
        let board = log.board(&ids, |_| true, same, false);
        assert_eq!(board[..SHOWN], log.most_used(&ids, SHOWN)[..], "the first row: the list's five");
        assert_eq!(board, [1, 2, 5, 6, 0, 3, 4]);
        // by level, the highest first (d and b tie at 30: b is the main anyway)
        let by_level = |a: usize, b: usize| levels[b].cmp(&levels[a]);
        assert_eq!(log.board(&ids, |_| true, by_level, false), [1, 5, 3, 2, 4, 0, 6]);
        assert_eq!(log.board(&ids, |_| true, by_level, true), [1, 6, 0, 4, 2, 3, 5], "the main first, the rest the other way round");
        // only the levels over 10: the main all the same
        assert_eq!(log.board(&ids, |i| levels[i] > 10 && i != 1, by_level, false), [1, 5, 3, 2]);
        assert!(TeamLog::default().board(&[], |_| true, same, false).is_empty());
    }

    /// Lifeforce, a gauge: all of it at first; a knock-out's cost less for each; none left,
    /// lost for good: not sent into battle, not the main (another alive is), not pinned; all
    /// lost, none fights.
    #[test]
    fn lifeforce_runs_out_and_the_lost_fight_no_more() {
        let ids: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let mut log = TeamLog::default();
        assert_eq!((log.lifeforce("a", 100), log.is_lost("a")), (100, false));
        log.knocked_out("a", 100, 25);
        assert_eq!(log.lifeforce("a", 100), 75);
        log.knocked_out("a", 100, 70);
        assert_eq!(log.lifeforce("a", 100), 5, "a knock-out costs what the world says");
        log.knocked_out("a", 100, 25);
        assert!(log.is_lost("a") && log.lifeforce("a", 100) == 0, "never under nothing");
        assert_eq!(log.battle_order(&ids), [1, 2], "the main lost: the next alive goes first");
        log.pin("a");
        assert_ne!(log.main.as_deref(), Some("a"), "no pinning the lost");
        log.pin("c");
        assert_eq!(log.battle_order(&ids), [2, 1]);
        log.main = Some("a".into());
        assert_eq!(log.main_of(&ids), Some(1), "pinned before it was lost: another is the main");
        for id in ["b", "c"] {
            for _ in 0..4 {
                log.knocked_out(id, 100, 25);
            }
        }
        assert!(log.battle_order(&ids).is_empty() && log.main_of(&ids).is_none(), "four knock-outs at 25: none alive, none fights");
        assert_eq!(log.lifeforce("b", 50), 0);
        let text = serde_json::to_string(&log).unwrap();
        assert_eq!(serde_json::from_str::<TeamLog>(&text).unwrap(), log, "kept with the story");
    }

    /// A knock-out costs a creature its level, up to the cap: level 15, 15; from 20 on, 20.
    #[test]
    fn a_knock_out_costs_the_level_up_to_the_cap() {
        assert_eq!([1, 15, 20, 21, 35, 100].map(|lv| ko_cost(lv, 20)), [1, 15, 20, 20, 20, 20]);
        assert_eq!(ko_cost(30, 25), 25, "the world's cap");
        let mut log = TeamLog::default();
        log.knocked_out("a", 100, ko_cost(15, 20));
        log.knocked_out("a", 100, ko_cost(35, 20));
        assert_eq!(log.lifeforce("a", 100), 65);
    }

    #[test]
    fn only_the_last_battles_count() {
        let mut log = TeamLog::default();
        log.record(vec!["old".into()]);
        for _ in 0..RECENT {
            log.record(vec!["new".into()]);
        }
        log.record(vec![]);
        assert_eq!(log.battles.len(), RECENT, "a battle no one fought in isn't one");
        assert_eq!((log.uses("old"), log.uses("new")), (0, RECENT));
    }

    /// Who fought: the creatures that came out (two at once in a double battle), once each;
    /// none before the battle starts.
    #[test]
    fn who_came_out_fought() {
        use rules::battle::{BattleKind, BattleSetup};
        use rules::config::{Config, Period};
        use rules::dex::Dex;
        use rules::mon::create_mon;
        let dex = std::sync::Arc::new(Dex::from_content(&super::fixture::content()).unwrap());
        let mons = |p: &[MonSpec]| {
            p.iter()
                .map(|s| {
                    let mut s = s.clone();
                    s.ivs = Some([20; 6]);
                    create_mon(&dex, &s, None, &Config::default())
                })
                .collect::<Vec<_>>()
        };
        let party = team(&["voltkit", "nibbler", "drowsbear", "voltkit"]);
        let setup = BattleSetup {
            kind: BattleKind::Trainer,
            size: 2,
            seed: 7,
            config: Config::default(),
            period: Period::Day,
            player_name: "You".into(),
            player_party: mons(&party),
            bag: Default::default(),
            foe_name: "Foe".into(),
            foe_party: mons(&team(&["drowsbear", "nibbler"])),
            pool: vec![],
        };
        let mut b = Battle::new(dex.clone(), setup);
        let ids = ids(&party);
        assert!(fought(&b, &ids).is_empty(), "no one out before it starts");
        b.start();
        assert_eq!(fought(&b, &ids), ["voltkit", "nibbler"]);
    }

    #[test]
    fn the_log_reads_back_as_it_was_written() {
        let mut log = TeamLog::default();
        log.record(vec!["a".into(), "b".into()]);
        log.pin("b");
        let text = serde_json::to_string(&log).unwrap();
        assert_eq!(serde_json::from_str::<TeamLog>(&text).unwrap(), log);
        // a story saved before the team was kept
        assert_eq!(serde_json::from_str::<TeamLog>("{}").unwrap(), TeamLog::default());
    }
}
