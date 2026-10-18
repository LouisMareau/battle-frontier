//! Rigs and animations, as glTF files carry them: a tree of nodes, some of them joints a
//! mesh is skinned to, and animations that move nodes over time. `pose` gives the joint
//! matrices for a moment of an animation: what the renderer multiplies each vertex by,
//! weighted by its joints.

use glam::{Mat4, Quat, Vec3};

/// A node's place under its parent, at rest.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub parent: Option<usize>,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

/// A model's skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    /// every node of the file
    pub nodes: Vec<Node>,
    /// the joints a mesh's vertices name, in the skin's order: (node, inverse bind matrix)
    pub joints: Vec<(usize, Mat4)>,
    /// the nodes, parents before children
    pub order: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Translation,
    Rotation,
    Scale,
}

/// One node's property over time: keyframes at `times`, their values (a vec3 padded, or
/// a quaternion), stepped or blended between.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub node: usize,
    pub path: Path,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
    pub step: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub name: String,
    /// seconds
    pub duration: f32,
    pub channels: Vec<Channel>,
}

impl Channel {
    /// The value at `t` seconds (the first before the start, the last past the end).
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let n = self.times.len().min(self.values.len());
        if n == 0 {
            return [0.0; 4];
        }
        if t <= self.times[0] || n == 1 {
            return self.values[0];
        }
        if t >= self.times[n - 1] {
            return self.values[n - 1];
        }
        let i = self.times[..n].partition_point(|&k| k <= t) - 1;
        let (t0, t1) = (self.times[i], self.times[i + 1]);
        let (a, b) = (self.values[i], self.values[i + 1]);
        if self.step || t1 <= t0 {
            return a;
        }
        let k = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
        match self.path {
            Path::Rotation => Quat::from_array(a).normalize().slerp(Quat::from_array(b).normalize(), k).to_array(),
            _ => [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, 0.0],
        }
    }
}

/// The joint matrices of a pose: the rig at rest, moved by `animation` at `t` seconds
/// (looping). Each is the joint's global transform times its inverse bind matrix.
pub fn pose(rig: &Rig, animation: Option<&Animation>, t: f32) -> Vec<Mat4> {
    let mut local: Vec<(Vec3, Quat, Vec3)> = rig.nodes.iter().map(|n| (n.translation, n.rotation, n.scale)).collect();
    if let Some(a) = animation {
        let t = if a.duration > 0.0 { t.rem_euclid(a.duration) } else { 0.0 };
        for ch in &a.channels {
            let Some(l) = local.get_mut(ch.node) else { continue };
            let v = ch.sample(t);
            match ch.path {
                Path::Translation => l.0 = Vec3::new(v[0], v[1], v[2]),
                Path::Rotation => l.1 = Quat::from_array(v).normalize(),
                Path::Scale => l.2 = Vec3::new(v[0], v[1], v[2]),
            }
        }
    }
    let mut global = vec![Mat4::IDENTITY; rig.nodes.len()];
    for &i in &rig.order {
        let (t, r, s) = local[i];
        let m = Mat4::from_scale_rotation_translation(s, r, t);
        global[i] = match rig.nodes[i].parent {
            Some(p) => global[p] * m,
            None => m,
        };
    }
    rig.joints.iter().map(|&(node, ibm)| global.get(node).copied().unwrap_or(Mat4::IDENTITY) * ibm).collect()
}

/// A point moved by its joints' matrices, weighted.
pub fn skin_point(p: Vec3, joints: [u16; 4], weights: [f32; 4], pose: &[Mat4]) -> Vec3 {
    let mut out = Vec3::ZERO;
    let mut total = 0.0;
    for k in 0..4 {
        let w = weights[k];
        if w <= 0.0 {
            continue;
        }
        let m = pose.get(joints[k] as usize).copied().unwrap_or(Mat4::IDENTITY);
        out += m.transform_point3(p) * w;
        total += w;
    }
    if total > 0.0 { out / total } else { p }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An arm of two bones: the root at the origin, the elbow a unit up it.
    fn arm() -> Rig {
        Rig {
            nodes: vec![
                Node { parent: None, translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE },
                Node { parent: Some(0), translation: Vec3::Y, rotation: Quat::IDENTITY, scale: Vec3::ONE },
            ],
            joints: vec![(0, Mat4::IDENTITY), (1, Mat4::from_translation(-Vec3::Y))],
            order: vec![0, 1],
        }
    }

    #[test]
    fn a_rest_pose_moves_nothing() {
        let p = pose(&arm(), None, 0.0);
        let tip = skin_point(Vec3::new(0.0, 2.0, 0.0), [1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], &p);
        assert!((tip - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-5, "{tip:?}");
    }

    #[test]
    fn a_bend_at_the_elbow_swings_the_tip() {
        let bend = Animation {
            name: "bend".into(),
            duration: 2.0,
            channels: vec![Channel {
                node: 1,
                path: Path::Rotation,
                times: vec![0.0, 1.0, 2.0],
                values: vec![Quat::IDENTITY.to_array(), Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array(), Quat::IDENTITY.to_array()],
                step: false,
            }],
        };
        let p = pose(&arm(), Some(&bend), 1.0);
        // a point a unit above the elbow swings to a unit left of it
        let tip = skin_point(Vec3::new(0.0, 2.0, 0.0), [1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], &p);
        assert!((tip - Vec3::new(-1.0, 1.0, 0.0)).length() < 1e-4, "{tip:?}");
        // half way there at half a second; back at rest when it loops round
        let half = skin_point(Vec3::new(0.0, 2.0, 0.0), [1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], &pose(&arm(), Some(&bend), 0.5));
        let r = std::f32::consts::FRAC_1_SQRT_2;
        assert!((half.x + r).abs() < 1e-3 && (half.y - (1.0 + r)).abs() < 1e-3, "{half:?}");
        let again = skin_point(Vec3::new(0.0, 2.0, 0.0), [1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], &pose(&arm(), Some(&bend), 4.0));
        assert!((again - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-4, "{again:?}");
        // the root is unmoved
        let base = skin_point(Vec3::new(0.0, 0.5, 0.0), [0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], &p);
        assert!((base - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-5);
    }
}
