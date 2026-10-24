//! A prop deleted everywhere the content names it: its record goes, and so do the props
//! placed on every map and in every template, the tree areas drop it, the actors that looked
//! like it look like nothing, the creatures it stood for keep their pictures, the terrains growing it (tall ground's grass props, a meadow's
//! model plants) stop. What's named where comes first (`uses_of_prop`: for a dialog to say
//! what deleting it means), what changed comes back by id, for the editor to write.

use crate::model::Prop;
use crate::{Content, Space};

/// Where a prop is named: on which maps (how many placed, in how many tree areas, how many
/// actors look like it), in which templates (how many placed, how many actors), which terrains
/// grow it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uses {
    pub maps: Vec<(String, usize, usize, usize)>,
    pub templates: Vec<(String, usize, usize)>,
    pub terrains: Vec<String>,
    /// the creatures it stands for on the map (their `prop`)
    pub creatures: Vec<String>,
    /// the containers it's the emptied look of (their `emptied`)
    pub looks: Vec<String>,
}

impl Uses {
    pub fn is_empty(&self) -> bool {
        self.maps.is_empty() && self.templates.is_empty() && self.terrains.is_empty() && self.creatures.is_empty() && self.looks.is_empty()
    }

    /// In words: "placed 3 times on 2 maps, in 1 template, grown by 1 terrain, 2 actors look
    /// like it"; "" when it's named nowhere.
    pub fn text(&self) -> String {
        let placed: usize = self.maps.iter().map(|m| m.1).sum::<usize>() + self.templates.iter().map(|t| t.1).sum::<usize>();
        let areas: usize = self.maps.iter().map(|m| m.2).sum();
        let actors: usize = self.maps.iter().map(|m| m.3).sum::<usize>() + self.templates.iter().map(|t| t.2).sum::<usize>();
        let plural = |n: usize, one: &str, more: &str| if n == 1 { one.to_string() } else { more.to_string() };
        let mut out = vec![];
        if placed > 0 {
            let maps = self.maps.iter().filter(|m| m.1 > 0).count();
            let templates = self.templates.iter().filter(|t| t.1 > 0).count();
            let mut on = vec![];
            if maps > 0 {
                on.push(format!("{maps} {}", plural(maps, "map", "maps")));
            }
            if templates > 0 {
                on.push(format!("{templates} {}", plural(templates, "template", "templates")));
            }
            out.push(format!("placed {placed} {} on {}", plural(placed, "time", "times"), on.join(" and ")));
        }
        if areas > 0 {
            out.push(format!("picked by {areas} tree {}", plural(areas, "area", "areas")));
        }
        if !self.terrains.is_empty() {
            out.push(format!("grown by {} {}", self.terrains.len(), plural(self.terrains.len(), "terrain", "terrains")));
        }
        if actors > 0 {
            out.push(format!("{actors} {} like it", plural(actors, "actor looks", "actors look")));
        }
        if !self.creatures.is_empty() {
            out.push(format!("the model of {} {}", self.creatures.len(), plural(self.creatures.len(), "creature", "creatures")));
        }
        if !self.looks.is_empty() {
            out.push(format!("the emptied look of {} {}", self.looks.len(), plural(self.looks.len(), "container", "containers")));
        }
        out.join(", ")
    }
}

/// What a deletion changed, for the editor to write: the prop's space and its model's file
/// (when it had one), and the maps, templates and terrains that named it.
#[derive(Clone, Debug, PartialEq)]
pub struct Deleted {
    pub prop: Prop,
    pub space: Space,
    pub maps: Vec<String>,
    pub templates: Vec<String>,
    pub terrains: Vec<String>,
    /// the creatures it stood for: their pictures now
    pub creatures: Vec<String>,
    /// the containers it was the emptied look of: as they are, once emptied
    pub looks: Vec<String>,
}

impl Content {
    /// Where the prop `id` is named.
    pub fn uses_of_prop(&self, id: &str) -> Uses {
        let look = format!("prop:{id}");
        let mut out = Uses::default();
        for m in self.maps.iter() {
            let placed = m.props.iter().filter(|p| p.prop == id).count();
            let areas = m.tree_areas.iter().filter(|a| a.trees.iter().any(|t| t == id)).count();
            let actors = m.actors.iter().filter(|a| a.look == look).count();
            if placed + areas + actors > 0 {
                out.maps.push((m.id.clone(), placed, areas, actors));
            }
        }
        for t in self.templates.iter() {
            let placed = t.props.iter().filter(|p| p.prop == id).count();
            let actors = t.actors.iter().filter(|a| a.look == look).count();
            if placed + actors > 0 {
                out.templates.push((t.id.clone(), placed, actors));
            }
        }
        for t in self.terrains.iter() {
            if t.growth.props.iter().any(|g| g.prop == id) || t.growth.meadow.plants.iter().any(|p| p.prop == id) {
                out.terrains.push(t.id.clone());
            }
        }
        out.creatures = self.creatures.iter().filter(|c| c.prop.as_deref() == Some(id)).map(|c| c.id.clone()).collect();
        out.looks = self.props.iter().filter(|q| q.emptied.as_deref() == Some(id)).map(|q| q.id.clone()).collect();
        out
    }

    /// The prop `id` deleted everywhere: what changed. Refused, nothing changed, when there's
    /// no such prop.
    pub fn delete_prop(&mut self, id: &str) -> Result<Deleted, String> {
        let Some(e) = self.props.remove(id) else { return Err(format!("no prop {id}")) };
        let look = format!("prop:{id}");
        let mut out = Deleted { prop: e.value, space: e.space, maps: vec![], templates: vec![], terrains: vec![], creatures: vec![], looks: vec![] };
        let maps: Vec<_> = self
            .maps
            .entries()
            .iter()
            .filter_map(|e| {
                let mut m = e.value.clone();
                let before = (m.props.len(), m.actors.iter().filter(|a| a.look == look).count(), m.tree_areas.iter().map(|a| a.trees.len()).sum::<usize>());
                m.props.retain(|p| p.prop != id);
                for a in m.tree_areas.iter_mut() {
                    a.trees.retain(|t| t != id);
                }
                for a in m.actors.iter_mut().filter(|a| a.look == look) {
                    a.look = String::new();
                }
                let after = (m.props.len(), 0, m.tree_areas.iter().map(|a| a.trees.len()).sum::<usize>());
                (before != after).then_some((m, e.space))
            })
            .collect();
        for (m, space) in maps {
            out.maps.push(m.id.clone());
            self.maps.put(m, space);
        }
        let templates: Vec<_> = self
            .templates
            .entries()
            .iter()
            .filter_map(|e| {
                let mut t = e.value.clone();
                let before = (t.props.len(), t.actors.iter().filter(|a| a.look == look).count());
                t.props.retain(|p| p.prop != id);
                for a in t.actors.iter_mut().filter(|a| a.look == look) {
                    a.look = String::new();
                }
                (before != (t.props.len(), 0)).then_some((t, e.space))
            })
            .collect();
        for (t, space) in templates {
            out.templates.push(t.id.clone());
            self.templates.put(t, space);
        }
        let terrains: Vec<_> = self
            .terrains
            .entries()
            .iter()
            .filter_map(|e| {
                let mut t = e.value.clone();
                let before = (t.growth.props.len(), t.growth.meadow.plants.len());
                t.growth.props.retain(|g| g.prop != id);
                t.growth.meadow.plants.retain(|p| p.prop != id);
                (before != (t.growth.props.len(), t.growth.meadow.plants.len())).then_some((t, e.space))
            })
            .collect();
        for (t, space) in terrains {
            out.terrains.push(t.id.clone());
            self.terrains.put(t, space);
        }
        let creatures: Vec<_> = self.creatures.entries().iter().filter(|e| e.value.prop.as_deref() == Some(id)).map(|e| (e.value.clone(), e.space)).collect();
        for (mut c, space) in creatures {
            c.prop = None;
            out.creatures.push(c.id.clone());
            self.creatures.put(c, space);
        }
        let looks: Vec<_> = self.props.entries().iter().filter(|e| e.value.emptied.as_deref() == Some(id)).map(|e| (e.value.clone(), e.space)).collect();
        for (mut q, space) in looks {
            q.emptied = None;
            out.looks.push(q.id.clone());
            self.props.put(q, space);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Actor, Grower, Map, Meadow, Placed, Plant, Template, Terrain, TreeArea};

    /// Where a prop is named is counted and said; deleted, it's gone from everywhere, what
    /// changed is told, and the rest is left as it was; a prop that isn't, refused.
    #[test]
    fn a_prop_deleted_is_gone_from_everywhere() {
        let mut c = Content::default();
        c.props.put(Prop { id: "rock".into(), name: "Rock".into(), model: Some("models/rock.glb".into()), ..Default::default() }, Space::Content);
        c.props.put(Prop { id: "tree".into(), name: "Tree".into(), ..Default::default() }, Space::Content);
        let put = |prop: &str| Placed { prop: prop.into(), at: [1.5, 1.5], ..Default::default() };
        c.maps.put(Map { id: "field".into(), props: vec![put("rock"), put("tree"), put("rock")], tree_areas: vec![TreeArea { trees: vec!["rock".into(), "tree".into()], ..Default::default() }], actors: vec![Actor { id: "statue".into(), look: "prop:rock".into(), ..Default::default() }], ..Default::default() }, Space::Content);
        c.maps.put(Map { id: "shore".into(), props: vec![put("tree")], ..Default::default() }, Space::Content);
        c.templates.put(Template { id: "cairn".into(), props: vec![put("rock")], ..Default::default() }, Space::Temporary);
        c.terrains.put(Terrain { id: "scree".into(), growth: crate::model::Growth { props: vec![Grower { prop: "rock".into(), weight: 1.0 }, Grower { prop: "tree".into(), weight: 1.0 }], meadow: Meadow { plants: vec![Plant { shape: "model".into(), prop: "rock".into(), ..Default::default() }], ..Default::default() }, ..Default::default() }, ..Default::default() }, Space::Content);
        c.creatures.put(crate::model::Creature { id: "golem".into(), name: "Golem".into(), prop: Some("rock".into()), ..Default::default() }, Space::Temporary);
        c.props.put(Prop { id: "mound".into(), name: "Mound".into(), category: "container".into(), emptied: Some("rock".into()), ..Default::default() }, Space::Content);
        let uses = c.uses_of_prop("rock");
        assert_eq!(uses, Uses { maps: vec![("field".into(), 2, 1, 1)], templates: vec![("cairn".into(), 1, 0)], terrains: vec!["scree".into()], creatures: vec!["golem".into()], looks: vec!["mound".into()] });
        assert_eq!(uses.text(), "placed 3 times on 1 map and 1 template, picked by 1 tree area, grown by 1 terrain, 1 actor looks like it, the model of 1 creature, the emptied look of 1 container");
        assert!(c.uses_of_prop("tree").text().starts_with("placed 2 times on 2 maps"));
        assert!(c.uses_of_prop("stone").is_empty() && c.uses_of_prop("stone").text().is_empty());
        let d = c.delete_prop("rock").unwrap();
        assert_eq!((d.prop.id.as_str(), d.space, d.maps.as_slice(), d.templates.as_slice(), d.terrains.as_slice()), ("rock", Space::Content, ["field".to_string()].as_slice(), ["cairn".to_string()].as_slice(), ["scree".to_string()].as_slice()));
        assert_eq!((d.creatures.as_slice(), c.creatures.get("golem").unwrap().prop.as_deref()), (["golem".to_string()].as_slice(), None), "the creature keeps its pictures");
        assert_eq!((d.looks.as_slice(), c.props.get("mound").unwrap().emptied.as_deref()), (["mound".to_string()].as_slice(), None), "the container stays as it is once emptied");
        assert!(!c.props.contains("rock") && c.props.contains("tree"));
        let field = c.maps.get("field").unwrap();
        assert_eq!((field.props.len(), field.props[0].prop.as_str(), field.tree_areas[0].trees.as_slice(), field.actors[0].look.as_str()), (1, "tree", ["tree".to_string()].as_slice(), ""));
        assert_eq!(c.maps.get("shore").unwrap().props.len(), 1, "untouched");
        assert!(c.templates.get("cairn").unwrap().props.is_empty());
        let scree = c.terrains.get("scree").unwrap();
        assert_eq!((scree.growth.props.len(), scree.growth.props[0].prop.as_str(), scree.growth.meadow.plants.len()), (1, "tree", 0));
        assert!(c.uses_of_prop("rock").is_empty());
        assert!(c.delete_prop("rock").is_err());
    }
}
