//! The player's team as the overworld's Team button shows it: the creatures they fought with
//! most in their last battles first, and before all of them the one they pinned as their main.
//! There's always a main (while there's a team), and it's the one sent out first in a battle.
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
/// How many creatures the Team's list shows.
pub const SHOWN: usize = 5;

/// What the story keeps of the team: who fought in each of the last battles (the latest last,
/// `RECENT` at most), and the main.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TeamLog {
    pub battles: Vec<Vec<String>>,
    /// the creature pinned first, by its id (`ids`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
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
    /// again, it stays.
    pub fn pin(&mut self, id: &str) {
        self.main = Some(id.to_string());
    }

    /// The main's place in the team (`ids`): the one pinned; while none is (or it's no longer
    /// in the team), the one that fought most lately: the team's first before any battle.
    /// None only for no team at all.
    pub fn main_of(&self, ids: &[String]) -> Option<usize> {
        self.main.as_ref().and_then(|m| ids.iter().position(|i| i == m)).or_else(|| self.ranked(ids, None).first().copied())
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
    /// sent out first, then the others in the team's order.
    pub fn battle_order(&self, ids: &[String]) -> Vec<usize> {
        let main = self.main_of(ids);
        main.into_iter().chain((0..ids.len()).filter(|&i| Some(i) != main)).collect()
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
