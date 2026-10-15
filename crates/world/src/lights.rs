//! The lights props give off, placed on a map: from the props on the ground, those inside
//! and outside buildings and those hung on their walls, each moved and turned with its
//! prop. A lamp post and a wall lamp are props with a `light`; a frame takes the nearest
//! of them (`render::scene::MAX_LIGHTS`), so a creature carrying a light fits the same way.

use content::{Content, Light};
use glam::{Mat4, Vec3};

use crate::buildings::anchor_placement;
use crate::mesh::placement;
use crate::WorldMap;

/// A light where its prop stands: in the world, pointing its way.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedLight {
    pub pos: Vec3,
    /// the way it points, a unit vector
    pub dir: Vec3,
    pub light: Light,
}

fn put(out: &mut Vec<PlacedLight>, light: &Light, m: Mat4) {
    let at = Vec3::new(light.at[0] as f32, light.at[1] as f32, light.at[2] as f32);
    let aim = Vec3::new(light.aim[0] as f32, light.aim[1] as f32, light.aim[2] as f32);
    out.push(PlacedLight { pos: m.transform_point3(at), dir: m.transform_vector3(aim).normalize_or(Vec3::NEG_Y), light: light.clone() });
}

/// A light carried by something standing at `feet` and facing `forward` (along the
/// ground): the light's `at` and `aim` are from the feet, +z the way it faces.
pub fn carried(light: &Light, feet: Vec3, forward: Vec3) -> PlacedLight {
    let f = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::Z);
    let mut out = vec![];
    put(&mut out, light, Mat4::from_translation(feet) * Mat4::from_rotation_y(f.x.atan2(f.z)));
    out.remove(0)
}

/// Every light on the map, where its prop stands.
pub fn lights(map: &WorldMap, content: &Content, level_height: f32) -> Vec<PlacedLight> {
    let mut out = vec![];
    let light = |id: &str| content.props.get(id).and_then(|p| p.light.as_ref().map(|l| (l, p.scalable())));
    for p in &map.props {
        let Some((l, scalable)) = light(&p.prop) else { continue };
        let scale = if scalable { p.scale as f32 } else { 1.0 };
        put(&mut out, l, placement(Vec3::new(p.pos()[0] as f32, p.level as f32 * level_height, p.pos()[1] as f32), p.turn as f32, scale));
    }
    for b in &map.buildings {
        let base = b.level as f32 * level_height;
        let (ox, oz) = (b.at[0] as f32, b.at[1] as f32);
        for (f, floor) in b.floors.iter().enumerate() {
            let y0 = base + f as f32 * level_height;
            for p in &floor.props {
                let Some((l, scalable)) = light(&p.prop) else { continue };
                let scale = if scalable { p.scale as f32 } else { 1.0 };
                put(&mut out, l, placement(Vec3::new(ox + p.pos()[0] as f32, y0 + 0.05, oz + p.pos()[1] as f32), p.turn as f32, scale));
            }
        }
        for a in &b.anchors {
            if let Some((l, _)) = light(&a.prop) {
                put(&mut out, l, anchor_placement(b, a, level_height));
            }
        }
        for p in &b.outside {
            let Some((l, scalable)) = light(&p.prop) else { continue };
            let scale = if scalable { p.scale as f32 } else { 1.0 };
            put(&mut out, l, placement(Vec3::new(ox + p.pos()[0] as f32, base, oz + p.pos()[1] as f32), p.turn as f32, scale));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{Anchor, Building, Floor, Placed, Prop};

    #[test]
    fn lights_turn_with_their_props() {
        let mut content = Content::default();
        let lamp = Prop { id: "lamp".into(), category: "decoration".into(), light: Some(Light { at: [0.0, 2.0, 0.5], aim: [0.0, -1.0, 1.0], ..Default::default() }), ..Default::default() };
        content.props.put(lamp, content::Space::Content);
        let mut map = WorldMap::new("m", "M", 8, 8, "grass");
        map.props.push(Placed { prop: "lamp".into(), at: [2.0, 3.0], level: 0, turn: 90.0, scale: 1.0, ..Default::default() });
        map.buildings.push(Building {
            id: "b".into(),
            at: [4, 4],
            cells: vec!["##".into(), "##".into()],
            floors: vec![Floor::default()],
            anchors: vec![Anchor { prop: "lamp".into(), cell: [0, 1], side: "s".into(), floor: 0, height: 1.0, inside: false }],
            ..Default::default()
        });
        let all = lights(&map, &content, 2.0);
        assert_eq!(all.len(), 2);
        // turned 90 degrees clockwise (seen from above): +z goes to -x
        let on_ground = &all[0];
        assert!((on_ground.pos - Vec3::new(1.5, 2.0, 3.0)).length() < 1e-4, "{:?}", on_ground.pos);
        assert!((on_ground.dir - Vec3::new(-1.0, -1.0, 0.0).normalize()).length() < 1e-4, "{:?}", on_ground.dir);
        // on the south wall of the building's cell (0, 1): facing south, half a cell out
        let on_wall = &all[1];
        assert!((on_wall.pos - Vec3::new(4.5, 3.0, 6.5)).length() < 1e-4, "{:?}", on_wall.pos);
        assert!(on_wall.dir.z > 0.5 && on_wall.dir.y < 0.0, "{:?}", on_wall.dir);
        // a tail light behind something walking west: behind it is east, and it points west
        let tail = Light { at: [0.0, 0.5, -0.4], aim: [0.0, -0.3, 1.0], ..Default::default() };
        let c = carried(&tail, Vec3::new(10.0, 0.0, 10.0), Vec3::new(-1.0, 0.0, 0.0));
        assert!((c.pos - Vec3::new(10.4, 0.5, 10.0)).length() < 1e-4, "{:?}", c.pos);
        assert!(c.dir.x < -0.9, "{:?}", c.dir);
    }
}
