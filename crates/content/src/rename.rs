//! A prop's id changed everywhere the content names it: its record (and its model's file,
//! when that's named after it), the props placed on every map and in every template, the
//! tree areas picking it, the actors that look like it ("prop:<id>"), the terrains growing it
//! (tall ground's grass props, a meadow's plants). What changed comes back by id, for the
//! editor to write.

use crate::model::{Actor, Placed, TreeArea};
use crate::{Content, Space};

/// What a rename changed, for the editor to save: the prop's space and its model's file
/// before and after (when it was named after the prop), and the maps, templates and
/// terrains that named it.
#[derive(Clone, Debug, PartialEq)]
pub struct Renamed {
    pub space: Space,
    pub model: Option<(String, String)>,
    pub maps: Vec<String>,
    pub templates: Vec<String>,
    pub terrains: Vec<String>,
}

/// Whether `id` can name a record: lowercase letters, digits, _ or -.
pub fn id_ok(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// A model's file named after `new` when it was named after `old` ("models/old.glb" to
/// "models/new.glb"); none when it has a name of its own.
fn model_named(model: &str, old: &str, new: &str) -> Option<String> {
    let (dir, file) = match model.rsplit_once('/') {
        Some((d, f)) => (format!("{d}/"), f),
        None => (String::new(), model),
    };
    let (stem, ext) = file.rsplit_once('.')?;
    (stem == old).then(|| format!("{dir}{new}.{ext}"))
}

fn placed(props: &mut [Placed], old: &str, new: &str) -> bool {
    let mut did = false;
    for p in props.iter_mut().filter(|p| p.prop == old) {
        p.prop = new.to_string();
        did = true;
    }
    did
}

fn areas(areas: &mut [TreeArea], old: &str, new: &str) -> bool {
    let mut did = false;
    for t in areas.iter_mut().flat_map(|a| a.trees.iter_mut()).filter(|t| *t == old) {
        *t = new.to_string();
        did = true;
    }
    did
}

fn actors(actors: &mut [Actor], old: &str, new: &str) -> bool {
    let (was, is) = (format!("prop:{old}"), format!("prop:{new}"));
    let mut did = false;
    for a in actors.iter_mut().filter(|a| a.look == was) {
        a.look = is.clone();
        did = true;
    }
    did
}

impl Content {
    /// The prop `old` renamed `new` everywhere: what changed. Refused, nothing changed, when
    /// `new` can't be an id or names a prop already, or there's no prop `old`.
    pub fn rename_prop(&mut self, old: &str, new: &str) -> Result<Renamed, String> {
        if old == new {
            return Err("the same id".into());
        }
        if !id_ok(new) {
            return Err(format!("\"{new}\" can't be an id: lowercase letters, digits, _ and - only"));
        }
        if self.props.contains(new) {
            return Err(format!("{new} is a prop already"));
        }
        let Some(e) = self.props.remove(old) else { return Err(format!("no prop {old}")) };
        let (mut prop, space) = (e.value, e.space);
        prop.id = new.to_string();
        let model = prop.model.as_ref().and_then(|m| model_named(m, old, new).map(|to| (m.clone(), to)));
        if let Some((_, to)) = &model {
            prop.model = Some(to.clone());
        }
        self.props.put(prop, space);
        let mut out = Renamed { space, model, maps: vec![], templates: vec![], terrains: vec![] };
        let maps: Vec<_> = self.maps.entries().iter().filter_map(|e| {
            let mut m = e.value.clone();
            let did = placed(&mut m.props, old, new) | areas(&mut m.tree_areas, old, new) | actors(&mut m.actors, old, new);
            did.then_some((m, e.space))
        }).collect();
        for (m, space) in maps {
            out.maps.push(m.id.clone());
            self.maps.put(m, space);
        }
        let templates: Vec<_> = self.templates.entries().iter().filter_map(|e| {
            let mut t = e.value.clone();
            let did = placed(&mut t.props, old, new) | actors(&mut t.actors, old, new);
            did.then_some((t, e.space))
        }).collect();
        for (t, space) in templates {
            out.templates.push(t.id.clone());
            self.templates.put(t, space);
        }
        let terrains: Vec<_> = self.terrains.entries().iter().filter_map(|e| {
            let mut t = e.value.clone();
            let mut did = false;
            for g in t.growth.props.iter_mut().filter(|g| g.prop == old) {
                g.prop = new.to_string();
                did = true;
            }
            for p in t.growth.meadow.plants.iter_mut().filter(|p| p.prop == old) {
                p.prop = new.to_string();
                did = true;
            }
            did.then_some((t, e.space))
        }).collect();
        for (t, space) in terrains {
            out.terrains.push(t.id.clone());
            self.terrains.put(t, space);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Grower, Map, Meadow, Plant, Prop, Template, Terrain};

    /// A prop renamed is named anew wherever the content names it, its model's file with it
    /// when the file was named after it, and what changed is told; a new id that isn't one,
    /// or is taken, or the same, or a prop that isn't, changes nothing.
    #[test]
    fn a_prop_renamed_is_named_anew_everywhere() {
        let mut c = Content::default();
        c.props.put(Prop { id: "rock".into(), name: "Rock".into(), model: Some("models/rock.glb".into()), ..Default::default() }, Space::Content);
        c.props.put(Prop { id: "tree".into(), name: "Tree".into(), model: Some("models/oak.glb".into()), ..Default::default() }, Space::Content);
        let put = |prop: &str| Placed { prop: prop.into(), at: [1.5, 1.5], ..Default::default() };
        c.maps.put(Map { id: "field".into(), props: vec![put("rock"), put("tree")], tree_areas: vec![TreeArea { trees: vec!["rock".into(), "tree".into()], ..Default::default() }], actors: vec![Actor { id: "statue".into(), look: "prop:rock".into(), ..Default::default() }], ..Default::default() }, Space::Content);
        c.maps.put(Map { id: "shore".into(), props: vec![put("tree")], ..Default::default() }, Space::Content);
        c.templates.put(Template { id: "cairn".into(), props: vec![put("rock")], ..Default::default() }, Space::Temporary);
        c.terrains.put(Terrain { id: "scree".into(), growth: crate::model::Growth { props: vec![Grower { prop: "rock".into(), weight: 1.0 }], meadow: Meadow { plants: vec![Plant { shape: "model".into(), prop: "rock".into(), ..Default::default() }], ..Default::default() }, ..Default::default() }, ..Default::default() }, Space::Content);
        c.terrains.put(Terrain { id: "lawn".into(), ..Default::default() }, Space::Content);
        let r = c.rename_prop("rock", "boulder").unwrap();
        assert_eq!(r.space, Space::Content);
        assert_eq!(r.model, Some(("models/rock.glb".to_string(), "models/boulder.glb".to_string())));
        assert_eq!((r.maps.as_slice(), r.templates.as_slice(), r.terrains.as_slice()), (["field".to_string()].as_slice(), ["cairn".to_string()].as_slice(), ["scree".to_string()].as_slice()));
        assert!(!c.props.contains("rock") && c.props.get("boulder").is_some_and(|p| p.name == "Rock" && p.model.as_deref() == Some("models/boulder.glb")));
        let field = c.maps.get("field").unwrap();
        assert_eq!((field.props[0].prop.as_str(), field.props[1].prop.as_str(), field.tree_areas[0].trees.as_slice(), field.actors[0].look.as_str()), ("boulder", "tree", ["boulder".to_string(), "tree".to_string()].as_slice(), "prop:boulder"));
        assert_eq!(c.templates.get("cairn").unwrap().props[0].prop, "boulder");
        let scree = c.terrains.get("scree").unwrap();
        assert_eq!((scree.growth.props[0].prop.as_str(), scree.growth.meadow.plants[0].prop.as_str()), ("boulder", "boulder"));
        // a model with a name of its own keeps it
        let r = c.rename_prop("tree", "oak").unwrap();
        assert_eq!((r.model, c.props.get("oak").unwrap().model.as_deref()), (None, Some("models/oak.glb")));
        assert_eq!(r.maps, ["field", "shore"]);
        // refused
        for (old, new) in [("boulder", "Boulder"), ("boulder", "oak"), ("boulder", "boulder"), ("rock", "stone"), ("boulder", "")] {
            assert!(c.rename_prop(old, new).is_err(), "{old} -> {new}");
            assert!(c.props.contains("boulder") && c.props.contains("oak") && c.props.len() == 2 && c.maps.get("field").unwrap().props[0].prop == "boulder", "nothing changed: {old} -> {new}");
        }
    }
}
