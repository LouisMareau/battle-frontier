//! The player's Bag: pockets of so many cells each (`Settings::bag`), the items in them
//! taking cells (an item's `size`; turned a quarter, across and down swap), moved and turned
//! to make room, as in packing; one kind stacks in a slot up to its `stack`. Kept in the
//! story's save (`SaveGame::bag`); what of it a battle can use goes in with the party
//! (`battle_bag`) and what the battle used comes out of it (`after_battle`).

use std::collections::BTreeMap;

use content::Content;
use rules::battle::Item as BattleItem;
use serde::{Deserialize, Serialize};

/// The Bag: each pocket's slots.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bag {
    pub pockets: Vec<PocketItems>,
}

/// A pocket's slots: each an item, where it lies and how many of it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketItems {
    pub id: String,
    pub slots: Vec<Slot>,
}

/// An item in a pocket: which, its top-left cell (across, down), whether it's turned a
/// quarter, and how many of it stack here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slot {
    pub item: String,
    pub at: [u8; 2],
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub turned: bool,
    pub count: u16,
}

/// What the Bag knows of an item, from its record: its pocket, its cells and its stack.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemFacts {
    pub pocket: String,
    pub cells: [u8; 2],
    pub stack: u16,
}

/// What the Bag knows of a pocket: its kind ("grid", "dispenser" or "rack") and its room as
/// a grid (a dispenser's tubes in a row, a rack's hooks).
#[derive(Clone, Debug, PartialEq)]
pub struct PocketFacts {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub room: [u8; 2],
}

/// What the Bag knows of items and pockets, from the content: an item's cells and stack as
/// its pocket has them (one tube, one hook, or its own cells in a grid).
#[derive(Clone, Debug, Default)]
pub struct Catalogue {
    items: BTreeMap<String, ItemFacts>,
    pockets: Vec<PocketFacts>,
}

impl Catalogue {
    pub fn of(content: &Content) -> Catalogue {
        let settings = content.world().bag;
        let pockets = settings.pockets.iter().map(|p| PocketFacts { id: p.id.clone(), name: p.name.clone(), kind: p.kind.clone(), room: p.room() }).collect();
        let items = content
            .items
            .iter()
            .map(|i| {
                let p = settings.pocket(&i.pocket);
                (i.id.clone(), ItemFacts { pocket: i.pocket.clone(), cells: p.map_or(i.cells(), |p| p.cells_of(i)), stack: p.map_or(i.stack_size(), |p| p.stack_of(i)) })
            })
            .collect();
        Catalogue { items, pockets }
    }

    /// The pockets in order.
    pub fn pockets(&self) -> &[PocketFacts] {
        &self.pockets
    }

    pub fn pocket(&self, id: &str) -> Option<&PocketFacts> {
        self.pockets.iter().find(|p| p.id == id)
    }

    pub fn pocket_cells(&self, id: &str) -> Option<[u8; 2]> {
        self.pocket(id).map(|p| p.room)
    }

    pub fn item(&self, id: &str) -> Option<&ItemFacts> {
        self.items.get(id)
    }
}

/// The cells an item takes, across and down, turned a quarter or not (turned, they swap).
pub fn span(cells: [u8; 2], turned: bool) -> [u8; 2] {
    if turned {
        [cells[1], cells[0]]
    } else {
        cells
    }
}

/// Whether `span` cells at `at` lie within a pocket `room` big.
fn within(room: [u8; 2], at: [u8; 2], span: [u8; 2]) -> bool {
    at[0] as u16 + span[0] as u16 <= room[0] as u16 && at[1] as u16 + span[1] as u16 <= room[1] as u16
}

/// Whether two placements share a cell.
fn overlap((a, sa): ([u8; 2], [u8; 2]), (b, sb): ([u8; 2], [u8; 2])) -> bool {
    let (a, sa, b, sb) = (a.map(u16::from), sa.map(u16::from), b.map(u16::from), sb.map(u16::from));
    a[0] < b[0] + sb[0] && b[0] < a[0] + sa[0] && a[1] < b[1] + sb[1] && b[1] < a[1] + sa[1]
}

impl PocketItems {
    /// The cells a slot takes.
    pub fn span_of(&self, cat: &Catalogue, i: usize) -> [u8; 2] {
        let s = &self.slots[i];
        span(cat.item(&s.item).map_or([1, 1], |f| f.cells), s.turned)
    }

    /// Whether `span` cells could lie at `at` (the slot `except` moving there): within the
    /// pocket, over no other slot.
    pub fn fits(&self, cat: &Catalogue, room: [u8; 2], except: Option<usize>, at: [u8; 2], span: [u8; 2]) -> bool {
        within(room, at, span) && self.slots.iter().enumerate().filter(|(i, _)| Some(*i) != except).all(|(i, s)| !overlap((at, span), (s.at, self.span_of(cat, i))))
    }

    /// The first place (the rows from the top, the cells from the left) where `cells` fit,
    /// as they are or turned a quarter.
    fn room_for(&self, cat: &Catalogue, room: [u8; 2], cells: [u8; 2]) -> Option<([u8; 2], bool)> {
        for y in 0..room[1] {
            for x in 0..room[0] {
                for turned in [false, true] {
                    if turned && cells[0] == cells[1] {
                        continue;
                    }
                    if self.fits(cat, room, None, [x, y], span(cells, turned)) {
                        return Some(([x, y], turned));
                    }
                }
            }
        }
        None
    }

    /// The cells its slots take, in all.
    pub fn taken(&self, cat: &Catalogue) -> usize {
        (0..self.slots.len()).map(|i| self.span_of(cat, i)).map(|s| s[0] as usize * s[1] as usize).sum()
    }
}

impl Bag {
    pub fn pocket(&self, id: &str) -> Option<&PocketItems> {
        self.pockets.iter().find(|p| p.id == id)
    }

    fn pocket_mut(&mut self, id: &str) -> &mut PocketItems {
        if let Some(i) = self.pockets.iter().position(|p| p.id == id) {
            return &mut self.pockets[i];
        }
        self.pockets.push(PocketItems { id: id.to_string(), slots: vec![] });
        self.pockets.last_mut().unwrap()
    }

    /// How many of `item` the Bag holds.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn count(&self, item: &str) -> u32 {
        self.pockets.iter().flat_map(|p| &p.slots).filter(|s| s.item == item).map(|s| s.count as u32).sum()
    }

    /// `n` of `item` put in: onto its stacks with room first, then into the first free place
    /// in its pocket (turned a quarter if that's how it fits), as many slots as it takes. How
    /// many didn't fit (no room left; or an item or a pocket the content doesn't know).
    pub fn add(&mut self, cat: &Catalogue, item: &str, n: u32) -> u32 {
        let Some(f) = cat.item(item).cloned() else { return n };
        let Some(room) = cat.pocket_cells(&f.pocket) else { return n };
        let mut left = n;
        let p = self.pocket_mut(&f.pocket);
        for s in p.slots.iter_mut().filter(|s| s.item == item) {
            let put = (f.stack.saturating_sub(s.count) as u32).min(left);
            s.count += put as u16;
            left -= put;
            if left == 0 {
                return 0;
            }
        }
        while left > 0 {
            let Some((at, turned)) = p.room_for(cat, room, f.cells) else { break };
            let put = (f.stack as u32).min(left);
            p.slots.push(Slot { item: item.to_string(), at, turned, count: put as u16 });
            left -= put;
        }
        left
    }

    /// `n` of `item` taken out, from its smallest stacks first (an emptied slot goes): how
    /// many were.
    pub fn take(&mut self, item: &str, n: u32) -> u32 {
        let mut left = n;
        for p in &mut self.pockets {
            let mut order: Vec<usize> = (0..p.slots.len()).filter(|&i| p.slots[i].item == item).collect();
            order.sort_by_key(|&i| p.slots[i].count);
            for i in order {
                let took = (p.slots[i].count as u32).min(left);
                p.slots[i].count -= took as u16;
                left -= took;
                if left == 0 {
                    break;
                }
            }
            p.slots.retain(|s| s.count > 0);
        }
        n - left
    }

    /// A slot moved to `at`, turned a quarter or not: whether it could go there (over no
    /// other, within the pocket); it stays where it was if not.
    pub fn move_slot(&mut self, cat: &Catalogue, pocket: &str, i: usize, at: [u8; 2], turned: bool) -> bool {
        let Some(room) = cat.pocket_cells(pocket) else { return false };
        let p = self.pocket_mut(pocket);
        let Some(s) = p.slots.get(i) else { return false };
        let cells = cat.item(&s.item).map_or([1, 1], |f| f.cells);
        if !p.fits(cat, room, Some(i), at, span(cells, turned)) {
            return false;
        }
        let s = &mut p.slots[i];
        s.at = at;
        s.turned = turned;
        true
    }

    /// The cells a pocket has free.
    pub fn free(&self, cat: &Catalogue, pocket: &str) -> usize {
        let room = cat.pocket_cells(pocket).map_or(0, |c| c[0] as usize * c[1] as usize);
        room.saturating_sub(self.pocket(pocket).map_or(0, |p| p.taken(cat)))
    }

    /// What of it a battle can use: each item the battle knows (the record's `battle_use`),
    /// with how many.
    pub fn battle_bag(&self, content: &Content) -> BTreeMap<BattleItem, u32> {
        let mut out = BTreeMap::new();
        for s in self.pockets.iter().flat_map(|p| &p.slots) {
            if let Some(item) = content.items.get(&s.item).and_then(|i| i.battle_use.as_deref()).and_then(BattleItem::from_key) {
                *out.entry(item).or_insert(0) += s.count as u32;
            }
        }
        out
    }

    /// What a battle used, taken out: what it went in with less what it came out with, off
    /// the records that use each item.
    pub fn after_battle(&mut self, content: &Content, went_in: &BTreeMap<BattleItem, u32>, came_out: &BTreeMap<BattleItem, u32>) {
        for (item, &n) in went_in {
            let mut used = n.saturating_sub(came_out.get(item).copied().unwrap_or(0));
            if used == 0 {
                continue;
            }
            let ids: Vec<String> = content.items.iter().filter(|i| i.battle_use.as_deref().and_then(BattleItem::from_key) == Some(*item)).map(|i| i.id.clone()).collect();
            for id in ids {
                used -= self.take(&id, used);
                if used == 0 {
                    break;
                }
            }
        }
    }

    /// The Bag a new story starts with: the usual few of each (`presets::DEFAULT_BAG`),
    /// through the records that use them.
    pub fn starting(content: &Content) -> Bag {
        let cat = Catalogue::of(content);
        let mut bag = Bag::default();
        for (item, n) in crate::presets::DEFAULT_BAG {
            if let Some(rec) = content.items.iter().find(|i| i.battle_use.as_deref().and_then(BattleItem::from_key) == Some(item)) {
                bag.add(&cat, &rec.id, n);
            }
        }
        bag
    }

    /// For the checks: each pocket's cells taken of its room ("consumables 7/30 items 0/24
    /// idps 2/4 key 0/16 tools 1/4").
    pub fn summary(&self, cat: &Catalogue) -> String {
        cat.pockets().iter().map(|p| format!("{} {}/{}", p.id, self.pocket(&p.id).map_or(0, |q| q.taken(cat)), p.room[0] as usize * p.room[1] as usize)).collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{BagSettings, Item, Pocket, Settings, Space};

    fn content() -> Content {
        let mut c = Content::default();
        let item = |id: &str, pocket: &str, size: [u8; 2], stack: u16, battle_use: Option<&str>| Item { id: id.into(), name: id.into(), pocket: pocket.into(), size, stack, battle_use: battle_use.map(String::from), ..Default::default() };
        c.items.put(item("potion", "consumables", [1, 1], 10, Some("potion")), Space::Content);
        c.items.put(item("hyper_potion", "consumables", [1, 2], 5, Some("hyper_potion")), Space::Content);
        c.items.put(item("revive", "consumables", [2, 1], 3, Some("revive")), Space::Content);
        c.items.put(item("idp", "idps", [1, 1], 1, Some("idp")), Space::Content);
        c.items.put(item("idp_reinforced", "idps", [1, 1], 1, Some("idp_reinforced")), Space::Content);
        c.items.put(item("map", "key", [2, 2], 1, None), Space::Content);
        c.items.put(item("scooter", "tools", [1, 1], 1, None), Space::Content);
        c.items.put(item("rod", "tools", [1, 1], 1, None), Space::Content);
        c.items.put(item("net", "tools", [1, 1], 1, None), Space::Content);
        let pockets = vec![
            Pocket { id: "consumables".into(), cells: [3, 2], ..Default::default() },
            Pocket { id: "idps".into(), kind: "dispenser".into(), tubes: 2, holds: 20, ..Default::default() },
            Pocket { id: "key".into(), cells: [2, 2], ..Default::default() },
            Pocket { id: "tools".into(), kind: "rack".into(), hooks: 2, ..Default::default() },
        ];
        c.settings.put(Settings { id: "world".into(), bag: BagSettings { pockets }, ..Default::default() }, Space::Content);
        c
    }

    /// Items go in by their pocket, stacking first, then into the first free cells, turned a
    /// quarter when that's how they fit, and what won't fit stays out; taken out from the
    /// smallest stacks, an emptied slot going; moved and turned where there's room, left
    /// where they were otherwise. The IDPs' dispenser: a tube a kind, as many as it holds;
    /// the tools' rack: one a hook. A battle gets the items it knows with their counts, and
    /// what it used comes out.
    #[test]
    fn the_bag_packs_its_pockets() {
        let c = content();
        let cat = Catalogue::of(&c);
        let mut bag = Bag::default();
        // 12 potions: a stack of 10 at the first cell, 2 at the next
        assert_eq!(bag.add(&cat, "potion", 12), 0);
        let m = bag.pocket("consumables").unwrap();
        assert_eq!(m.slots.len(), 2);
        assert_eq!((m.slots[0].at, m.slots[0].count, m.slots[1].at, m.slots[1].count), ([0, 0], 10, [1, 0], 2));
        assert_eq!(bag.count("potion"), 12);
        // 3 more top the second stack up
        assert_eq!(bag.add(&cat, "potion", 3), 0);
        assert_eq!((bag.pocket("consumables").unwrap().slots[1].count, bag.count("potion")), (5, 15));
        // a hyper potion (1 across, 2 down) fits at the third cell; the pocket's 3 by 2 has
        // the bottom row's first two cells left: a revive (2 by 1) goes there as it is, and
        // nothing else fits
        assert_eq!(bag.add(&cat, "hyper_potion", 1), 0);
        let m = bag.pocket("consumables").unwrap();
        assert_eq!((m.slots[2].at, m.slots[2].turned), ([2, 0], false));
        assert_eq!(bag.add(&cat, "revive", 1), 0);
        let m = bag.pocket("consumables").unwrap();
        assert_eq!((m.slots[3].at, m.slots[3].turned, m.taken(&cat)), ([0, 1], false, 6));
        assert_eq!(bag.free(&cat, "consumables"), 0);
        assert_eq!(bag.add(&cat, "potion", 20), 20 - 5, "the stacks topped up, no room for a third");
        // the key pocket is 2 by 2: a map fills it; an unknown item goes nowhere
        assert_eq!((bag.add(&cat, "map", 1), bag.add(&cat, "stone", 1)), (0, 1));
        assert_eq!(bag.pocket("key").unwrap().slots[0].at, [0, 0]);
        // a 2 by 1 revive in a pocket one across and two down: turned a quarter to fit
        let narrow = Catalogue::of(&{
            let mut c = content();
            c.settings.put(Settings { id: "world".into(), bag: BagSettings { pockets: vec![Pocket { id: "consumables".into(), cells: [1, 2], ..Default::default() }] }, ..Default::default() }, Space::Content);
            c
        });
        let mut one = Bag::default();
        assert_eq!(one.add(&narrow, "revive", 1), 0);
        let s = &one.pocket("consumables").unwrap().slots[0];
        assert_eq!((s.at, s.turned), ([0, 0], true), "turned a quarter");
        // taken out: from the smallest stack first, the emptied slot gone
        assert_eq!(bag.take("potion", 7), 7);
        let m = bag.pocket("consumables").unwrap();
        assert_eq!((bag.count("potion"), m.slots.iter().filter(|s| s.item == "potion").count()), (13, 2));
        assert_eq!(bag.take("potion", 13), 13);
        assert_eq!((bag.count("potion"), bag.pocket("consumables").unwrap().slots.len()), (0, 2));
        assert_eq!(bag.take("potion", 1), 0, "none left");
        // moved: the hyper potion to the first column (free now), turned across the top; not
        // onto the revive, nor out of the pocket
        let i = bag.pocket("consumables").unwrap().slots.iter().position(|s| s.item == "hyper_potion").unwrap();
        assert!(bag.move_slot(&cat, "consumables", i, [0, 0], true));
        assert_eq!(bag.pocket("consumables").unwrap().span_of(&cat, i), [2, 1]);
        assert!(!bag.move_slot(&cat, "consumables", i, [0, 1], true), "the revive is there");
        assert!(!bag.move_slot(&cat, "consumables", i, [2, 0], true), "out of the pocket");
        assert_eq!(bag.pocket("consumables").unwrap().slots[i].at, [0, 0], "left where it was");
        // the dispenser: 25 IDPs fill a tube (20) and start the next; a second kind has no
        // tube left until that one is emptied; a tube's IDPs moved to the other tube
        assert_eq!(bag.add(&cat, "idp", 25), 0);
        let d = bag.pocket("idps").unwrap();
        assert_eq!((d.slots[0].at, d.slots[0].count, d.slots[1].at, d.slots[1].count), ([0, 0], 20, [1, 0], 5));
        assert_eq!(bag.add(&cat, "idp_reinforced", 3), 3, "no tube free");
        assert_eq!(bag.take("idp", 5), 5);
        assert_eq!(bag.add(&cat, "idp_reinforced", 3), 0);
        let d = bag.pocket("idps").unwrap();
        assert_eq!((d.slots[1].item.as_str(), d.slots[1].at), ("idp_reinforced", [1, 0]));
        assert!(!bag.move_slot(&cat, "idps", 1, [0, 0], false), "the other tube has the plain ones");
        assert_eq!(bag.take("idp", 20), 20);
        assert!(bag.move_slot(&cat, "idps", 0, [0, 0], false), "into the emptied tube");
        // the rack: one tool a hook, two hooks
        assert_eq!((bag.add(&cat, "scooter", 1), bag.add(&cat, "rod", 1), bag.add(&cat, "net", 1)), (0, 0, 1));
        assert_eq!(bag.add(&cat, "scooter", 1), 1, "one of a tool");
        let r = bag.pocket("tools").unwrap();
        assert_eq!((r.slots[0].at, r.slots[1].at), ([0, 0], [1, 0]));
        assert!(bag.summary(&cat).contains("idps 1/2") && bag.summary(&cat).ends_with("tools 2/2"), "{}", bag.summary(&cat));
        // the battle's: the items it knows (not the map, nor the tools), by their counts;
        // what it used comes out
        assert_eq!(bag.add(&cat, "idp", 7), 0);
        let went_in = bag.battle_bag(&c);
        assert_eq!(went_in, BTreeMap::from([(BattleItem::HyperPotion, 1), (BattleItem::Revive, 1), (BattleItem::Idp, 7), (BattleItem::ReinforcedIdp, 3)]));
        let came_out = BTreeMap::from([(BattleItem::HyperPotion, 0), (BattleItem::Revive, 1), (BattleItem::Idp, 4), (BattleItem::ReinforcedIdp, 3)]);
        bag.after_battle(&c, &went_in, &came_out);
        assert_eq!((bag.count("hyper_potion"), bag.count("revive"), bag.count("idp"), bag.count("idp_reinforced")), (0, 1, 4, 3));
        // a new story's Bag: the usual few of what the content has (here potions and IDPs)
        let first = Bag::starting(&c);
        assert_eq!((first.count("potion"), first.count("idp"), first.count("stone")), (5, 10, 0));
    }
}
