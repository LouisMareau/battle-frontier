//! The Bag's window, from the overworld's HUD: the pockets inside the bag as tabs
//! (Consumables, Items, Key items), each a grid of cells its items lie in, each taking its
//! own; on the bag's side, always to hand whatever the tab, the IDPs' dispenser (a tube a
//! kind, how many in each) and the tool rack (a tool a hook). An item dragged moves within
//! its pocket (the cells it would take shown as it goes: lit where it fits, red where it
//! doesn't; a right click or R turns it a quarter); the game's buttons do it all too: the
//! arrows move a cursor over the cells, tubes and hooks, A takes the item there up and puts
//! it down, L or R turns it, B puts it back (or, holding nothing, closes the window); L and R
//! change the tab when nothing's held. The item under the pointer or the cursor says what it
//! is at the window's foot.

use egui::{Align2, Color32, Id, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use content::Content;

use crate::bag::{span, Bag, Catalogue};
use crate::input::{Button, Input};
use crate::layout;
use crate::platform;
use crate::title::{button_label, menu_font, title_font};

/// The HUD's Bag button's mark.
pub const BAG: &str = "uiHudBag";
/// A cell's side, and the gap between cells (points).
const CELL: f32 = 46.0;
const GAP: f32 = 3.0;
/// A dispenser's tube: its width and height; a rack's hook: its side.
const TUBE: Vec2 = Vec2::new(44.0, 118.0);
const HOOK: f32 = 46.0;

/// An item lifted (by the mouse or the buttons): its pocket and slot, where it would go and
/// turned or not, and where it was (put back there if it can't go where it's dropped).
#[derive(Clone, Debug, PartialEq)]
struct Held {
    pocket: String,
    slot: usize,
    at: [u8; 2],
    turned: bool,
    by_mouse: bool,
    /// the mouse: where in the item it was taken (points from its top-left)
    grip: Vec2,
}

/// The window's own state.
#[derive(Default)]
pub struct BagUi {
    /// the grid pocket shown (among the catalogue's grids)
    pub tab: usize,
    held: Option<Held>,
    /// the cursor (the game's buttons): a cell's name ("cell:<pocket>:<x>:<y>"), once used
    cursor: Option<String>,
    ring: bool,
    /// opened this frame: the A that opened it isn't its own
    pub fresh: bool,
    /// the item under the pointer or the cursor, to say what it is
    about: Option<String>,
}

/// A pocket's colour (its items' blocks), by its id.
fn tint(pocket: &str) -> Color32 {
    match pocket {
        "consumables" => Color32::from_rgb(70, 150, 140),
        "items" => Color32::from_rgb(170, 125, 60),
        "idps" => Color32::from_rgb(80, 120, 200),
        "key" => Color32::from_rgb(140, 90, 170),
        "tools" => Color32::from_rgb(120, 120, 130),
        _ => Color32::from_rgb(100, 110, 120),
    }
}

fn cell_name(pocket: &str, x: u8, y: u8) -> String {
    format!("cell:{pocket}:{x}:{y}")
}

/// A cell's name read back: its pocket and cell.
fn cell_of(name: &str) -> Option<(&str, [u8; 2])> {
    let mut parts = name.strip_prefix("cell:")?.rsplitn(3, ':');
    let y = parts.next()?.parse().ok()?;
    let x = parts.next()?.parse().ok()?;
    Some((parts.next()?, [x, y]))
}

/// Where a pocket's cells are on screen: its origin and a cell's stride (a grid's cells
/// square; a dispenser's tubes tall; a rack's hooks square).
#[derive(Clone, Copy)]
struct Place {
    origin: Pos2,
    stride: Vec2,
    size: Vec2,
}

impl Place {
    fn cell(&self, at: [u8; 2], span: [u8; 2]) -> Rect {
        let min = self.origin + Vec2::new(at[0] as f32 * self.stride.x, at[1] as f32 * self.stride.y);
        Rect::from_min_size(min, Vec2::new(span[0] as f32 * self.stride.x - (self.stride.x - self.size.x), span[1] as f32 * self.stride.y - (self.stride.y - self.size.y)))
    }

    /// The cell a point is over (the top-left of what's dragged), within `room`.
    fn at(&self, p: Pos2, room: [u8; 2], span: [u8; 2]) -> [u8; 2] {
        let x = ((p.x - self.origin.x) / self.stride.x + 0.5).floor().clamp(0.0, (room[0].saturating_sub(span[0])) as f32) as u8;
        let y = ((p.y - self.origin.y) / self.stride.y + 0.5).floor().clamp(0.0, (room[1].saturating_sub(span[1])) as f32) as u8;
        [x, y]
    }
}

/// The Bag's window for a frame: whether it stays open.
pub fn bag_window(ctx: &egui::Context, bag: &mut Bag, cat: &Catalogue, content: &Content, input: &Input, st: &mut BagUi) -> bool {
    let grids: Vec<usize> = cat.pockets().iter().enumerate().filter(|(_, p)| p.kind != "dispenser" && p.kind != "rack").map(|(i, _)| i).collect();
    let dispensers: Vec<usize> = cat.pockets().iter().enumerate().filter(|(_, p)| p.kind == "dispenser").map(|(i, _)| i).collect();
    let racks: Vec<usize> = cat.pockets().iter().enumerate().filter(|(_, p)| p.kind == "rack").map(|(i, _)| i).collect();
    if grids.is_empty() {
        return false;
    }
    st.tab = st.tab.min(grids.len() - 1);
    let shown = cat.pockets()[grids[st.tab]].id.clone();
    // the game's buttons: the ring shows once they're used (always with a controller in
    // hand), and the mouse moving hides it; the first press only shows where it is
    let pad = crate::pads::on_pad();
    if ctx.input(|i| i.pointer.delta().length() > 2.0) {
        st.ring = false;
    }
    let dirs = [(Button::Left, Vec2::new(-1.0, 0.0)), (Button::Right, Vec2::new(1.0, 0.0)), (Button::Up, Vec2::new(0.0, -1.0)), (Button::Down, Vec2::new(0.0, 1.0))];
    let fresh = std::mem::take(&mut st.fresh);
    let mut moved = dirs.iter().find(|(b, _)| input.pressed(*b)).map(|&(_, d)| d).filter(|_| !fresh);
    let mut press = input.pressed(Button::A) && !fresh;
    let back = input.pressed(Button::B) && !fresh;
    let turn_or_tab = (input.pressed(Button::R) as i32 - input.pressed(Button::L) as i32).clamp(-1, 1);
    if moved.is_some() || press || turn_or_tab != 0 {
        if !(st.ring || pad) {
            (moved, press) = (None, false);
        }
        st.ring = true;
    }
    // a turn: R (or L) with an item held, a right click, or the R key
    let turn = (st.held.is_some() && turn_or_tab != 0) || ctx.input(|i| i.pointer.secondary_clicked() || i.key_pressed(Key::R));
    if let Some(h) = st.held.as_mut().filter(|_| turn) {
        h.turned = !h.turned;
    }
    // the tabs change with L and R while nothing's held
    if st.held.is_none() && turn_or_tab != 0 && !fresh {
        st.tab = (st.tab as i32 + turn_or_tab).rem_euclid(grids.len() as i32) as usize;
    }
    let shown = if st.held.is_none() { cat.pockets()[grids[st.tab]].id.clone() } else { shown };
    let mut open = true;
    let mut stops: Vec<layout::Stop> = vec![];
    let mut places: Vec<(String, Place)> = vec![];
    let mut clicked_tab: Option<usize> = None;
    let mut hovered: Option<String> = None;
    let mut dropped: Option<(Held, bool)> = None;
    let b = |b: Button| button_label(b);
    let controls = format!("The arrows or the d-pad: over the cells, the tubes and the hooks\n{}: take the item there up, put it down\n{}, {}: turn it a quarter (holding nothing: the next pocket)\n{}: put it back; holding nothing, close\nThe mouse: drag an item; a right click or R turns it", b(Button::A), b(Button::L), b(Button::R), b(Button::B));
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let released = ctx.input(|i| i.pointer.primary_released());
    let modal = egui::Modal::new(Id::new("hud-bag-window")).frame(egui::Frame::new().inner_margin(22.0)).show(ctx, |ui| {
        let glass = ui.painter().add(egui::Shape::Noop);
        // the title, the controls' words, Close
        ui.horizontal(|ui| {
            ui.label(RichText::new("Bag").font(title_font(26.0)).color(Color32::WHITE));
            ui.add_space(6.0);
            ui.label(RichText::new(bag.summary(cat)).font(menu_font(12.0)).color(Color32::from_white_alpha(140)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = ui.add(egui::Button::new(RichText::new("Close").font(menu_font(14.0))).frame(false));
                platform::set_hook("uiBagClose", &format!("{:.0},{:.0}", close.rect.center().x, close.rect.center().y));
                if close.clicked() {
                    open = false;
                }
                ui.label(RichText::new("?").font(menu_font(14.0)).color(Color32::from_white_alpha(170))).on_hover_text(&controls);
            });
        });
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            // inside the bag: the tabs, and the pocket shown as a grid
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    for (k, &i) in grids.iter().enumerate() {
                        let p = &cat.pockets()[i];
                        let on = k == st.tab;
                        let label = RichText::new(&p.name).font(menu_font(15.0)).color(if on { Color32::WHITE } else { Color32::from_white_alpha(150) });
                        let free = bag.free(cat, &p.id);
                        let r = ui.add(egui::Button::new(label).frame(false).selected(on)).on_hover_text(format!("{free} cell{} free", if free == 1 { "" } else { "s" }));
                        platform::set_hook(&format!("uiBagTab_{}", p.id), &format!("{:.0},{:.0}", r.rect.center().x, r.rect.center().y));
                        if r.clicked() {
                            clicked_tab = Some(k);
                        }
                    }
                });
                ui.add_space(6.0);
                let p = cat.pocket(&shown).cloned().unwrap();
                let (w, h) = (p.room[0] as f32 * (CELL + GAP) - GAP, p.room[1] as f32 * (CELL + GAP) - GAP);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
                let place = Place { origin: rect.min, stride: Vec2::splat(CELL + GAP), size: Vec2::splat(CELL) };
                for y in 0..p.room[1] {
                    for x in 0..p.room[0] {
                        let c = place.cell([x, y], [1, 1]);
                        ui.painter().rect_filled(c, 4.0, Color32::from_black_alpha(90));
                        stops.push(layout::Stop { name: cell_name(&shown, x, y), rect: c });
                    }
                }
                places.push((shown.clone(), place));
            });
            ui.add_space(18.0);
            // on the bag's side: the dispenser's tubes, the rack's hooks
            ui.vertical(|ui| {
                for &i in &dispensers {
                    let p = &cat.pockets()[i];
                    ui.label(RichText::new(&p.name).font(menu_font(13.0)).color(Color32::from_white_alpha(170)));
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(p.room[0] as f32 * (TUBE.x + GAP) - GAP, TUBE.y), Sense::hover());
                    let place = Place { origin: rect.min, stride: Vec2::new(TUBE.x + GAP, TUBE.y), size: TUBE };
                    for x in 0..p.room[0] {
                        let c = place.cell([x, 0], [1, 1]);
                        ui.painter().rect_filled(c, 8.0, Color32::from_black_alpha(90));
                        ui.painter().rect_stroke(c, 8.0, Stroke::new(1.0, Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
                        stops.push(layout::Stop { name: cell_name(&p.id, x, 0), rect: c });
                    }
                    places.push((p.id.clone(), place));
                    ui.add_space(10.0);
                }
                for &i in &racks {
                    let p = &cat.pockets()[i];
                    ui.label(RichText::new(&p.name).font(menu_font(13.0)).color(Color32::from_white_alpha(170)));
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(p.room[0] as f32 * (HOOK + GAP) - GAP, HOOK), Sense::hover());
                    let place = Place { origin: rect.min, stride: Vec2::new(HOOK + GAP, HOOK), size: Vec2::splat(HOOK) };
                    for x in 0..p.room[0] {
                        let c = place.cell([x, 0], [1, 1]);
                        ui.painter().rect_filled(c, 6.0, Color32::from_black_alpha(90));
                        ui.painter().circle_filled(c.center_top() + Vec2::new(0.0, 6.0), 2.5, Color32::from_white_alpha(70));
                        stops.push(layout::Stop { name: cell_name(&p.id, x, 0), rect: c });
                    }
                    places.push((p.id.clone(), place));
                }
            });
        });
        // the items, over their cells: each a block of its pocket's colour, its name and
        // count; the one held drawn where it would go, over everything
        let mut held_shape: Option<(Rect, bool, String, u16)> = None;
        for (pocket, place) in &places {
            let Some(items) = bag.pocket(pocket) else { continue };
            let room = cat.pocket_cells(pocket).unwrap_or([1, 1]);
            for (i, s) in items.slots.iter().enumerate() {
                let is_held = st.held.as_ref().is_some_and(|h| &h.pocket == pocket && h.slot == i);
                let sp = items.span_of(cat, i);
                let r = place.cell(s.at, sp);
                let name = content.items.get(&s.item).map(|it| it.name.clone()).unwrap_or_else(|| s.item.clone());
                let id = Id::new(("bag-slot", pocket.as_str(), i));
                let resp = ui.interact(r, id, Sense::click_and_drag());
                platform::set_hook(&format!("uiBagSlot_{pocket}_{i}"), &format!("{:.0},{:.0}", r.center().x, r.center().y));
                if resp.hovered() {
                    hovered = Some(s.item.clone());
                }
                if resp.drag_started() && st.held.is_none() {
                    // (taken where it was pressed, not where the drag was told apart from a click)
                    let grip = ctx.input(|i| i.pointer.press_origin()).or(pointer).map_or(Vec2::ZERO, |p| p - r.min);
                    st.held = Some(Held { pocket: pocket.clone(), slot: i, at: s.at, turned: s.turned, by_mouse: true, grip });
                }
                if is_held {
                    let h = st.held.as_ref().unwrap();
                    let hs = span(cat.item(&s.item).map_or([1, 1], |f| f.cells), h.turned);
                    let at = if h.by_mouse { pointer.map_or(h.at, |p| place.at(p - h.grip, room, hs)) } else { h.at };
                    let fits = items.fits(cat, room, Some(i), at, hs);
                    held_shape = Some((place.cell(at, hs), fits, name.clone(), s.count));
                    // where it was: a faint block
                    ui.painter().rect_stroke(r, 5.0, Stroke::new(1.0, tint(pocket).gamma_multiply(0.6)), egui::StrokeKind::Inside);
                    if h.by_mouse && released {
                        dropped = Some((Held { at, ..h.clone() }, fits));
                    }
                    continue;
                }
                ui.painter().rect_filled(r.shrink(1.5), 5.0, tint(pocket).gamma_multiply(if resp.hovered() { 1.0 } else { 0.85 }));
                ui.painter().rect_stroke(r.shrink(1.5), 5.0, Stroke::new(1.0, Color32::from_white_alpha(70)), egui::StrokeKind::Inside);
                let font = menu_font(if r.width() < CELL * 1.5 { 11.0 } else { 13.0 });
                ui.painter().text(r.center(), Align2::CENTER_CENTER, short(&name, if r.width() < CELL * 1.5 { 7 } else { 14 }), font, Color32::WHITE);
                if s.count > 1 {
                    ui.painter().text(r.right_bottom() - Vec2::new(5.0, 3.0), Align2::RIGHT_BOTTOM, format!("×{}", s.count), menu_font(12.0), Color32::from_white_alpha(230));
                }
            }
        }
        if let Some((r, fits, name, count)) = held_shape {
            let c = if fits { Color32::from_rgb(90, 190, 120) } else { Color32::from_rgb(200, 80, 80) };
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("bag-held")));
            p.rect_filled(r.shrink(1.5), 5.0, c.gamma_multiply(0.9));
            p.rect_stroke(r.shrink(1.5), 5.0, Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Inside);
            p.text(r.center(), Align2::CENTER_CENTER, short(&name, 14), menu_font(13.0), Color32::WHITE);
            if count > 1 {
                p.text(r.right_bottom() - Vec2::new(5.0, 3.0), Align2::RIGHT_BOTTOM, format!("×{count}"), menu_font(12.0), Color32::WHITE);
            }
        }
        // what the item under the pointer (or the cursor) is
        ui.add_space(8.0);
        let about = hovered.clone().or_else(|| st.about.clone());
        let (name, what) = about.as_deref().and_then(|id| content.items.get(id)).map(|it| (it.name.clone(), it.description.clone())).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(if name.is_empty() { "Drag an item to move it; a right click turns it" } else { &name }).font(menu_font(14.0)).color(Color32::WHITE));
            if !what.is_empty() {
                ui.label(RichText::new(what).font(menu_font(12.5)).color(Color32::from_white_alpha(200)));
            }
        });
        // the glass: frosted as the HUD's buttons, a little darker for the words on it
        let r = ui.min_rect().expand(22.0);
        let mut shapes = layout::frosted_shapes(r, 18.0, 0.0);
        let edge = shapes.len() - 1;
        shapes.insert(edge, egui::epaint::RectShape::filled(r, 18.0, Color32::from_black_alpha(95)).into());
        ui.painter().set(glass, egui::Shape::Vec(shapes));
    });
    if let Some(k) = clicked_tab {
        st.tab = k;
    }
    // dropped by the mouse: where it fits, else back where it was
    if let Some((h, fits)) = dropped {
        if fits {
            bag.move_slot(cat, &h.pocket, h.slot, h.at, h.turned);
        }
        st.held = None;
    }
    // the game's buttons: the cursor over the cells (moved, it goes to the next in that
    // direction; holding an item, the item goes), A takes up and puts down, B puts back
    if st.ring || pad {
        if let Some(h) = st.held.as_mut().filter(|h| !h.by_mouse) {
            if let Some(d) = moved {
                let room = cat.pocket_cells(&h.pocket).unwrap_or([1, 1]);
                let sp = bag.pocket(&h.pocket).map_or([1, 1], |p| p.span_of(cat, h.slot));
                let hs = if h.turned == bag.pocket(&h.pocket).is_some_and(|p| p.slots[h.slot].turned) { sp } else { [sp[1], sp[0]] };
                h.at[0] = (h.at[0] as i32 + d.x as i32).clamp(0, room[0].saturating_sub(hs[0]) as i32) as u8;
                h.at[1] = (h.at[1] as i32 + d.y as i32).clamp(0, room[1].saturating_sub(hs[1]) as i32) as u8;
                st.cursor = Some(cell_name(&h.pocket, h.at[0], h.at[1]));
            }
            if press {
                let h = h.clone();
                if bag.move_slot(cat, &h.pocket, h.slot, h.at, h.turned) {
                    st.held = None;
                }
            } else if back {
                st.held = None;
            }
        } else {
            let at = st.cursor.as_ref().and_then(|c| stops.iter().position(|s| &s.name == c));
            match (at, moved) {
                (Some(at), Some(d)) => st.cursor = Some(stops[layout::step(&stops, at, d)].name.clone()),
                (None, _) if moved.is_some() || press => st.cursor = stops.first().map(|s| s.name.clone()),
                _ => {}
            }
            // the slot under the cursor: said, and taken up with A
            let under = st.cursor.as_deref().and_then(cell_of).and_then(|(pocket, c)| {
                let items = bag.pocket(pocket)?;
                let i = items.slots.iter().enumerate().position(|(i, s)| {
                    let sp = items.span_of(cat, i);
                    c[0] >= s.at[0] && c[0] < s.at[0] + sp[0] && c[1] >= s.at[1] && c[1] < s.at[1] + sp[1]
                })?;
                Some((pocket.to_string(), i))
            });
            st.about = under.as_ref().and_then(|(p, i)| bag.pocket(p).map(|q| q.slots[*i].item.clone()));
            if press {
                if let Some((pocket, i)) = under {
                    let s = &bag.pocket(&pocket).unwrap().slots[i];
                    st.held = Some(Held { pocket: pocket.clone(), slot: i, at: s.at, turned: s.turned, by_mouse: false, grip: Vec2::ZERO });
                }
            } else if back {
                open = false;
            }
        }
        if let Some(s) = st.cursor.as_ref().and_then(|c| stops.iter().find(|s| &s.name == c)) {
            layout::ring(ctx, s.rect);
        }
    } else {
        st.about = None;
    }
    if modal.should_close() {
        open = false;
    }
    if !open {
        st.held = None;
    }
    platform::set_hook("hudbagwindow", if open { "open" } else { "" });
    platform::set_hook("bagtab", &shown);
    platform::set_hook("bag", &bag.summary(cat));
    platform::set_hook("bagheld", st.held.as_ref().map(|h| format!("{}:{}:{},{}{}", h.pocket, h.slot, h.at[0], h.at[1], if h.turned { ":turned" } else { "" })).as_deref().unwrap_or(""));
    platform::set_hook("bagcursor", st.cursor.as_deref().unwrap_or(""));
    open
}

/// A name cut to `most` characters, with an ellipsis.
fn short(name: &str, most: usize) -> String {
    if name.chars().count() <= most {
        return name.to_string();
    }
    let cut: String = name.chars().take(most.saturating_sub(1)).collect();
    format!("{cut}…")
}
