// ---------------------------------------------------------------- meadows' blades and flowers

// Each blade made as it's drawn (Ghost of Tsushima's way): its vertices from nothing but which
// of its template's points it is (`vertex_index`: its row and side, the template its rows'),
// the blade as placed (`Blade`) and its kind (`Kind`). A blade is a curve (a quadratic Bézier)
// from its foot to its tip, leaning its own way and curving over as much as its kind says, bent
// by the wind from its rooted foot (its tip furthest, as far as the wind's strength and its own
// give say) and pushed aside by whoever walks through it (`trampled`), narrowing to its point;
// a flower a thinner stem with a head of petals round its heart, nodding as the stem bends. How fine, how wide and how tall it's drawn go by how big it looks (`blade_lod.wgsl`).
// Lit as the ground under it is (its normals turned half way up), or in the toon look's steps;
// never inked. With compute shaders the culling lists the blades drawn and the drawing reads
// them (`vs_blade_listed`); without, each square's blades come as instances and the drawing
// leaves out what isn't drawn itself (`vs_blade`).

@group(1) @binding(6) var<uniform> meadow: Kinds;
// the blades (all the maps', in the scene's space) and a list's indices into them (the
// draw's list at its offset)
@group(3) @binding(1) var<storage, read> placed: array<Blade>;
@group(3) @binding(2) var<storage, read> listed: array<u32>;

struct BladeIn {
    @location(6) at: vec3<f32>,
    @location(7) turn: f32,
    @location(8) size: f32,
    @location(9) dim: f32,
    @location(10) kind: u32,
    @location(11) seed: u32,
};

fn blade_in(v: BladeIn) -> Blade {
    return Blade(v.at, v.turn, v.size, v.dim, v.kind, v.seed);
}

// nothing drawn: a point out of sight (its triangles have no area)
fn unseen() -> MapOut {
    var o: MapOut;
    o.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    o.light_pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    return o;
}

// the wind on a blade: how far its tip goes downwind, as a share of its height, by the wind's
// strength (with its gusts, swaying in its own time) and the plant's own give (`sway`: a thin
// blade 1, a stiff stem less, 0 rigid); and a flutter across
fn blade_wind(at: vec3<f32>, phase: f32, sway: f32) -> vec3<f32> {
    let s = g.wind.z;
    if (s <= 0.0 || sway <= 0.0) {
        return vec3<f32>(0.0);
    }
    let way = vec3<f32>(g.wind.x, 0.0, g.wind.y);
    let ph = phase * 6.2832 + dot(at.xz, vec2<f32>(0.11, 0.07));
    let gust = gust_at(at.xz);
    let push = s * gust * (0.5 + 0.5 * sin(g.wind_b.z + ph)) * 0.85 * sway;
    let across = vec3<f32>(-way.z, 0.0, way.x);
    let flutter = sway * (0.25 + 0.75 * s) * gust * 0.08 * sin(g.wind_b.w + ph * 3.1);
    return way * push + across * flutter;
}

// a point of a blade's curve (`at`: its point and its way at `t`, 0 its foot to 1 its tip) bent
// by the wind: pushed `wind` (how far its tip goes) more the further up it is, as t squared, so
// its foot stays rooted and its tip goes furthest, as a blade painted for the wind would (no
// weight at its root, the most at its tip); coming down a little as it bends over, its length
// kept, about
fn blown(at: array<vec3<f32>, 2>, wind: vec3<f32>, t: f32, tall: f32) -> array<vec3<f32>, 2> {
    let w = wind * t * t;
    let over = dot(wind, wind) / (2.0 * max(tall, 0.05));
    let down = vec3<f32>(0.0, over * t * t * t, 0.0);
    return array<vec3<f32>, 2>(at[0] + w - down, at[1] + wind * 2.0 * t - down * 3.0 / max(t, 0.05));
}

// a point of a template: its part (0 a blade's rows, 1 a flower's stem, 2 a petal, 3 its
// heart, 4 a far flower's head), how many rows its part has, its row (the last its tip) or its
// petal, and its side or corner
struct Point {
    part: u32,
    rows: u32,
    row: u32,
    side: u32,
};

// the templates' points (`blades.rs`'s `TEMPLATES` names them): a blade's in 7, 3 or 1 rows
// from 0, 16 and 24 (two a row, then its tip); a flower's stem in 3 rows from 32, its 8
// petals' four corners from 40, its heart's from 72; in 1 row from 80, its petals' three
// corners from 84, its heart's from 108; a far flower's head from 112
fn point_of(id: u32) -> Point {
    var p: Point;
    if (id < 32u) {
        p.part = 0u;
        var base = 0u;
        p.rows = 7u;
        if (id >= 24u) {
            base = 24u;
            p.rows = 1u;
        } else if (id >= 16u) {
            base = 16u;
            p.rows = 3u;
        }
        p.row = (id - base) / 2u;
        p.side = (id - base) % 2u;
    } else if (id < 40u) {
        p.part = 1u;
        p.rows = 3u;
        p.row = (id - 32u) / 2u;
        p.side = (id - 32u) % 2u;
    } else if (id < 72u) {
        p.part = 2u;
        p.rows = 4u;
        p.row = (id - 40u) / 4u;
        p.side = (id - 40u) % 4u;
    } else if (id < 80u) {
        p.part = 3u;
        p.side = id - 72u;
    } else if (id < 84u) {
        p.part = 1u;
        p.rows = 1u;
        p.row = (id - 80u) / 2u;
        p.side = (id - 80u) % 2u;
    } else if (id < 108u) {
        p.part = 2u;
        p.rows = 3u;
        p.row = (id - 84u) / 3u;
        p.side = (id - 84u) % 3u;
    } else if (id < 112u) {
        p.part = 3u;
        p.side = id - 108u;
    } else {
        p.part = 4u;
        p.side = id - 112u;
    }
    return p;
}

// a quadratic Bézier from `a` through towards `c` to `b`, at `t`: its point and its way
fn bezier(a: vec3<f32>, c: vec3<f32>, b: vec3<f32>, t: f32) -> array<vec3<f32>, 2> {
    let s = 1.0 - t;
    return array<vec3<f32>, 2>(a * s * s + c * (2.0 * s * t) + b * (t * t), (c - a) * (2.0 * s) + (b - c) * (2.0 * t));
}

// a point of a blade or a flower, in the world, with its normal (towards the eye), its colour,
// how much it glows of itself (its colour times this, whatever the light) and whether it's
// toon; none when it isn't drawn
struct Made {
    pos: vec3<f32>,
    normal: vec3<f32>,
    color: vec3<f32>,
    glow: f32,
    toon: f32,
    drawn: bool,
};

fn made(b: Blade, id: u32) -> Made {
    var m: Made;
    m.drawn = false;
    let k = kind_of(b);
    let placed_height = placed_tall(k, b);
    // how big it looks from the eye (in the sun's pass too: the eye's blades cast the shadows)
    let px = cell_px((g.view_proj * vec4<f32>(blade_middle(b, placed_height), 1.0)).w);
    if (!kept(px, b.seed)) {
        return m;
    }
    let tall = placed_height * tallness(px, b.seed);
    if (tall < 0.002) {
        return m;
    }
    let wide = widened(px);
    let r1 = unit(mix_bits(b.seed));
    let r2 = unit(mix_bits(b.seed ^ 0x9e3779b9u));
    let r3 = unit(mix_bits(b.seed ^ 0x85ebca6bu));
    let r4 = unit(mix_bits(b.seed ^ 0xc2b2ae35u));
    let p = point_of(id);
    let flower = k.form.x > 0.5;
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let dir = vec3<f32>(cos(b.turn), 0.0, -sin(b.turn));
    let side = vec3<f32>(sin(b.turn), 0.0, cos(b.turn));
    // its curve: leaning over its own way, curving over; the wind then bends it from its foot
    // (`blown`: rooted there, its tip going furthest)
    var lean = k.bend.x * (0.6 + 0.8 * r1);
    var bend = k.bend.y * (0.5 + 1.0 * r2);
    if (flower) {
        lean = lean * 0.5;
        bend = bend * 0.5;
    }
    let a = min(lean, 1.0) * 1.3;
    let tip_way = normalize(up * cos(a) + dir * sin(a));
    let tip = b.at + tip_way * tall;
    let ctrl = mix((b.at + tip) * 0.5, b.at + up * tall, clamp(bend, 0.0, 1.0));
    // the wind, and whoever walks through it (pushing its tip aside by up to its height and a
    // bit, whatever its give)
    let trample = trampled(b.at, 1.0);
    let wind = blade_wind(b.at, r3, k.bend.z) * tall + trample * min(1.0, 1.2 * tall / max(length(trample), 0.001));
    let dimmed = 1.0 - b.dim;
    m.toon = k.tip.w;
    if (p.part <= 1u) {
        // a blade's rows, or a flower's stem: two points a row across it, its tip one
        let t = f32(p.row) / f32(p.rows);
        let at = blown(bezier(b.at, ctrl, tip, t), wind, t, tall);
        var half = 0.5 * k.form.w * (0.8 + 0.4 * r4) * wide;
        if (flower) {
            half = half * (1.0 - 0.5 * t);
        } else {
            half = half * pow(1.0 - t, 0.8);
        }
        if (p.row >= p.rows) {
            half = 0.0;
        }
        let s = f32(p.side) * 2.0 - 1.0;
        m.pos = at[0] + side * half * s;
        // its face's normal, rounded across it (its sides turned out)
        var n = normalize(cross(side, at[1]));
        n = normalize(n + side * s * 0.45);
        m.normal = n;
        var c = mix(k.foot.rgb, k.tip.rgb, smoothstep(0.0, 1.0, t));
        c = c * (0.82 + 0.36 * r4) * mix(0.55, 1.0, sqrt(t));
        m.color = c * dimmed;
        // a glowing blade towards its tip (a flower's stem: none)
        if (!flower) {
            m.glow = k.heart.w * smoothstep(0.25, 1.0, t);
        }
    } else {
        // a flower's head at its stem's tip, facing up and nodding the way it leans (and the
        // wind bends it): its petals round its heart (fewer than eight: the rest none), or far
        // off a dot of its colour
        let top = blown(bezier(b.at, ctrl, tip, 1.0), wind, 1.0, tall);
        let face = normalize(mix(up, normalize(top[1]), 0.35));
        let e1 = normalize(cross(face, side));
        let e2 = cross(face, e1);
        let radius = 0.5 * k.bend.w * (0.8 + 0.4 * r4) * sqrt(wide);
        let centre = top[0] + face * radius * 0.05;
        m.normal = face;
        if (p.part == 2u) {
            let petals = u32(clamp(k.foot.w, 3.0, 8.0));
            if (p.row >= petals) {
                return m;
            }
            let ang = (f32(p.row) + r1 * 0.3) / f32(petals) * 6.2832 + r2 * 6.2832;
            let out = e1 * cos(ang) + e2 * sin(ang);
            let across = e1 * -sin(ang) + e2 * cos(ang);
            // its corners: at its foot (two, or one in the simpler template), at its tip two
            var q = centre + out * radius * 0.12;
            var shade = 0.78;
            if (p.rows == 4u) {
                if (p.side >= 2u) {
                    q = centre + out * radius + face * radius * 0.18;
                    shade = 1.0;
                }
                let s = select(-1.0, 1.0, p.side % 2u == 1u);
                q = q + across * s * radius * select(0.14, 0.34, p.side >= 2u);
            } else if (p.side >= 1u) {
                let s = select(-1.0, 1.0, p.side == 2u);
                q = centre + out * radius + face * radius * 0.18 + across * s * radius * 0.4;
                shade = 1.0;
            }
            m.pos = q;
            m.color = k.petal.rgb * shade * (0.88 + 0.24 * r3) * dimmed;
            m.glow = k.heart.w;
        } else {
            // its heart (a small square over the petals), or the far head (a larger one, its
            // petals' colour with some of its heart's)
            let corner = vec2<f32>(select(-1.0, 1.0, (p.side & 1u) == 1u), select(-1.0, 1.0, p.side >= 2u));
            var size = radius * 0.24;
            var c = k.heart.rgb;
            if (p.part == 4u) {
                size = radius * 0.75;
                c = mix(k.petal.rgb, k.heart.rgb, 0.25);
            }
            m.pos = centre + face * radius * 0.04 + (e1 * corner.x + e2 * corner.y) * size;
            m.color = c * dimmed;
            m.glow = k.heart.w;
        }
    }
    // facing the eye (both its faces drawn alike)
    if (dot(m.normal, g.camera.xyz - m.pos) < 0.0) {
        m.normal = -m.normal;
    }
    m.drawn = true;
    return m;
}

// a point of a blade drawn for the eye: lit with its normal turned half way up (as the ground
// under it is lit)
fn blade_out(m: Made) -> MapOut {
    if (!m.drawn) {
        return unseen();
    }
    var o: MapOut;
    o.clip = g.view_proj * vec4<f32>(m.pos, 1.0);
    o.uv = vec2<f32>(0.5, 0.5);
    o.normal = normalize(mix(m.normal, vec3<f32>(0.0, 1.0, 0.0), 0.45));
    o.world = m.pos;
    o.light_pos = g.light_view_proj * vec4<f32>(m.pos + m.normal * g.shadow.x, 1.0);
    // (its glow in the alpha: blades have none of their own)
    o.color = vec4<f32>(m.color, m.glow);
    o.toon = m.toon;
    return o;
}

// the same for the sun
fn blade_shadow(m: Made) -> MapOut {
    if (!m.drawn) {
        return unseen();
    }
    var o: MapOut;
    o.clip = g.light_view_proj * vec4<f32>(m.pos, 1.0);
    o.world = m.pos;
    o.light_pos = o.clip;
    return o;
}

@vertex
fn vs_blade(@builtin(vertex_index) id: u32, v: BladeIn) -> MapOut {
    return blade_out(made(blade_in(v), id));
}

@vertex
fn vs_blade_shadow(@builtin(vertex_index) id: u32, v: BladeIn) -> MapOut {
    return blade_shadow(made(blade_in(v), id));
}

@vertex
fn vs_blade_listed(@builtin(vertex_index) id: u32, @builtin(instance_index) i: u32) -> MapOut {
    return blade_out(made(placed[listed[i]], id));
}

@vertex
fn vs_blade_listed_shadow(@builtin(vertex_index) id: u32, @builtin(instance_index) i: u32) -> MapOut {
    return blade_shadow(made(placed[listed[i]], id));
}

@fragment
fn fs_blade(i: MapOut) -> Solid {
    var o: Solid;
    // lit, and what glows of it after (its glow in the colour's alpha): that shines
    let shining = i.color.rgb * i.color.a;
    let c = i.color.rgb * light(i.normal, i.light_pos, i.world, i.toon) + shining;
    o.color = vec4<f32>(fog(focus_dim(c, i.world), i.world), 1.0);
    o.look = look(i.world, i.normal, 0.0);
    o.emit = vec4<f32>(shining, 0.0);
    return o;
}

// the sun's: its depth alone
@fragment
fn fs_blade_shadow(i: MapOut) {
}
