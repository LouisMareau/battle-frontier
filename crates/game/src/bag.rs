//! The player's Bag: pockets of so many cells each (`Settings::bag`; grown by the bigger
//! bags found or given, `rooms`), the items in them taking cells (an item's `size`; turned a
//! quarter, across and down swap), each its own, none stacking in a grid (a dispenser's tubes
//! hold what the pocket says), moved and turned to make room, as in packing. Kept in the
//! story's save (`SaveGame::bag`); what of it a battle can use goes in with the party
//! (`battle_bag`) and what the battle used comes out of it (`after_battle`); what's found
//! in the world or given goes in through `receive`.

use std::collections::BTreeMap;

use content::Content;
use rules::battle::Item as BattleItem;
use serde::{Deserialize, Serialize};

/// The Bag: each pocket's slots, and the pockets grown past the settings' cells (a bigger
/// bag found or given: pocket id -> its cells across and down).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bag {
    pub pockets: Vec<PocketItems>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub rooms: BTreeMap<String, [u8; 2]>,
}

/// What came of an item received (`Bag::receive`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Received {
    /// put in its pocket
    Kept,
    /// a bigger bag: the pockets grown (which, to what)
    Grew(Vec<(String, [u8; 2])>),
    /// no room for it in its pocket (named)
    NoRoom(String),
    /// no such item
    Unknown,
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

/// What the Bag knows of an item, from its record: its pocket, its cells and its stack (one
/// in a grid; a dispenser's tube's worth), and the pockets it grows when it's a bigger bag.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemFacts {
    pub pocket: String,
    pub cells: [u8; 2],
    pub stack: u16,
    pub grows: Vec<(String, [u8; 2])>,
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
                (i.id.clone(), ItemFacts { pocket: i.pocket.clone(), cells: p.map_or(i.cells(), |p| p.cells_of(i)), stack: p.map_or(1, |p| p.stack_of(i)), grows: i.grows.iter().map(|g| (g.pocket.clone(), g.cells)).collect() })
            })
            .collect();
        Catalogue { items, pockets }
    }

    /// The pockets in order.
    /// With the pockets a Bag has grown (`Bag::rooms`): each grid's cells the bigger of the
    /// settings' and the Bag's.
    pub fn grown(mut self, bag: &Bag) -> Catalogue {
        for p in &mut self.pockets {
            if let Some(cells) = bag.rooms.get(&p.id).filter(|_| p.kind == "grid") {
                p.room = [p.room[0].max(cells[0]), p.room[1].max(cells[1])];
            }
        }
        self
    }

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

    /// An item found in the world or given: put in its pocket, or, a bigger bag, the pockets
    /// it names grown to its cells (never smaller than they are; the item isn't kept). What
    /// came of it; `cat` is the catalogue as the Bag has grown it so far.
    pub fn receive(&mut self, cat: &Catalogue, item: &str) -> Received {
        let Some(f) = cat.item(item).cloned() else { return Received::Unknown };
        if !f.grows.is_empty() {
            let mut grown = vec![];
            for (pocket, cells) in &f.grows {
                let now = cat.pocket_cells(pocket).unwrap_or([0, 0]);
                let to = [now[0].max(cells[0]), now[1].max(cells[1])];
                if to != now {
                    self.rooms.insert(pocket.clone(), to);
                    grown.push((pocket.clone(), to));
                }
            }
            return Received::Grew(grown);
        }
        if self.add(cat, item, 1) == 0 {
            Received::Kept
        } else {
            Received::NoRoom(cat.pocket(&f.pocket).map(|p| p.name.clone()).filter(|n| !n.is_empty()).unwrap_or(f.pocket.clone()))
        }
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
        c.items.put(Item { id: "big_bag".into(), name: "Big bag".into(), pocket: "key".into(), grows: vec![content::Grow { pocket: "consumables".into(), cells: [4, 3] }, content::Grow { pocket: "key".into(), cells: [2, 2] }], ..Default::default() }, Space::Content);
        let pockets = vec![
            Pocket { id: "consumables".into(), cells: [3, 2], ..Default::default() },
            Pocket { id: "idps".into(), kind: "dispenser".into(), tubes: 2, holds: 20, ..Default::default() },
            Pocket { id: "key".into(), cells: [2, 2], ..Default::default() },
            Pocket { id: "tools".into(), kind: "rack".into(), hooks: 2, ..Default::default() },
        ];
        c.settings.put(Settings { id: "world".into(), bag: BagSettings { pockets }, ..Default::default() }, Space::Content);
        c
    }

    /// Items go in by their pocket, each its own cells (none stacking in a grid), into the
    /// first free cells, turned a quarter when that's how they fit, and what won't fit stays
    /// out; taken out a slot each, the emptied slot going; moved and turned where there's
    /// room, left where they were otherwise. The IDPs' dispenser: a tube a kind, as many as
    /// it holds; the tools' rack: one a hook. A battle gets the items it knows with their
    /// counts, and what it used comes out. A bigger bag received grows the pockets it names.
    #[test]
    fn the_bag_packs_its_pockets() {
        let c = content();
        let cat = Catalogue::of(&c);
        let mut bag = Bag::default();
        // each potion its own cell: six fill the 3 by 2 pocket, the other two stay out
        assert_eq!(bag.add(&cat, "potion", 8), 2);
        let m = bag.pocket("consumables").unwrap();
        assert_eq!(m.slots.len(), 6);
        assert!(m.slots.iter().all(|s| s.count == 1) && m.slots[0].at == [0, 0] && m.slots[5].at == [2, 1], "{:?}", m.slots);
        assert_eq!((bag.count("potion"), bag.free(&cat, "consumables")), (6, 0));
        // four taken out (a slot each, the first ones): room for a hyper potion (1 across, 2
        // down) down the first column and a revive (2 by 1) along the top beside it; then full
        assert_eq!(bag.take("potion", 4), 4);
        assert_eq!((bag.add(&cat, "hyper_potion", 1), bag.add(&cat, "revive", 1)), (0, 0));
        let m = bag.pocket("consumables").unwrap();
        let at = |item: &str| m.slots.iter().find(|s| s.item == item).map(|s| (s.at, s.turned)).unwrap();
        assert_eq!((at("hyper_potion"), at("revive"), m.taken(&cat)), (([0, 0], false), ([1, 0], false), 6));
        assert_eq!(bag.free(&cat, "consumables"), 0);
        assert_eq!(bag.add(&cat, "potion", 1), 1, "full");
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
        // taken out: a slot each, the emptied slot gone; none left
        assert_eq!(bag.take("potion", 1), 1);
        assert_eq!((bag.count("potion"), bag.pocket("consumables").unwrap().slots.len()), (1, 3));
        assert_eq!(bag.take("potion", 5), 1);
        assert_eq!((bag.take("potion", 1), bag.pocket("consumables").unwrap().slots.len()), (0, 2), "none left");
        // moved: the hyper potion to the bottom row's free cells, turned along it; not onto
        // the revive, nor out of the pocket
        let i = bag.pocket("consumables").unwrap().slots.iter().position(|s| s.item == "hyper_potion").unwrap();
        assert!(bag.move_slot(&cat, "consumables", i, [1, 1], true));
        assert_eq!(bag.pocket("consumables").unwrap().span_of(&cat, i), [2, 1]);
        assert!(!bag.move_slot(&cat, "consumables", i, [1, 0], true), "the revive is there");
        assert!(!bag.move_slot(&cat, "consumables", i, [2, 1], true), "out of the pocket");
        assert_eq!(bag.pocket("consumables").unwrap().slots[i].at, [1, 1], "left where it was");
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
        // a new story's Bag: the usual few of what the content has (here potions and IDPs,
        // the potions a cell each)
        let first = Bag::starting(&c);
        assert_eq!((first.count("potion"), first.count("idp"), first.count("stone")), (5, 10, 0));
        // received: an item into its pocket, none when it's full, nothing unknown; a bigger
        // bag grows the pockets it names (never smaller), isn't kept, and the catalogue grown
        // by the Bag has the room
        let mut found = Bag::default();
        assert_eq!(found.receive(&cat, "map"), Received::Kept);
        assert_eq!(found.receive(&cat, "map"), Received::NoRoom("key".into()), "the key pocket is full");
        assert_eq!(found.receive(&cat, "stone"), Received::Unknown);
        assert_eq!(found.receive(&cat, "big_bag"), Received::Grew(vec![("consumables".into(), [4, 3])]), "the key pocket is 2 by 2 already: only Consumables grows");
        assert_eq!((found.rooms.get("consumables"), found.count("big_bag")), (Some(&[4, 3]), 0));
        let bigger = Catalogue::of(&c).grown(&found);
        assert_eq!((bigger.pocket_cells("consumables"), found.free(&bigger, "consumables"), cat.pocket_cells("consumables")), (Some([4, 3]), 12, Some([3, 2])));
        assert_eq!(found.add(&bigger, "potion", 12), 0, "twelve potions in the bigger pocket");
    }
}
