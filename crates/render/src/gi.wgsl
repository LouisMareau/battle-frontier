// ---------------------------------------------------------------- the indirect light
// What glows lights what's round it, with soft shadows behind walls and cliffs: radiance
// cascades (Alexander Sannikov's) over the world's ground, in compute shaders, each frame
// (`gi.rs` runs them; `scene.rs` draws the height pass and reads the probes in `indirect`).
// Over a square region round the camera's target, snapped to the probes' grid so nothing
// swims as the camera moves:
//   1. the height pass (`fs_height`, from straight above) writes how high what blocks light
//      stands at each texel of the region: the ground, its cliffs, buildings, props (not the
//      trees: a lamp lights the ground under a crown); far below where there's nothing;
//   2. `gi_clear`, then `gi_map` once per world map, make the field: at each texel what
//      shines there (the ground's own glow, worked out as the ground shader draws it, the
//      lamps' light bounced off the ground's colour, and what the screen showed shining there
//      last frame but the ground: the blades) and the ground's height; and what blocks
//      light there, from the height pass, a texel wider each way (a thin wall counts whole,
//      and no ray's step, each shorter than that, steps over it);
//   3. `gi_cascade`, from the coarsest cascade to the finest: each probe sends rays all
//      round, each over its cascade's interval of distances (further out in the coarser ones,
//      from fewer probes further apart, in more directions), adding up what shines along it,
//      nearer counting more, blocked by what stands higher than the probe's ground (a wall, a
//      step up: the edge's own glow seen a little), what lies lower counting less (a flat
//      receiver can't see below itself); past its interval, the coarser cascade's rays from
//      the four probes round it (those standing on another level counting little);
//   4. `gi_resolve` keeps, per probe of the finest, the light from all round and the way
//      most of it comes from, which the lighting reads.

struct GiParams {
    // x, y: the region's north-west corner (x, z), z: a field texel's size (cells), w: the
    // field's texels a side
    region: vec4<f32>,
    // x: this cascade's probes a side, y: its directions, z: their spacing (cells), w: a
    // ray's step (cells)
    probes: vec4<f32>,
    // x: the interval's start (cells), y: its end, z: the coarser cascade's probes a side
    // (0: none), w: its directions
    reach: vec4<f32>,
    // z: the lamps' bounce, w: the screen's glow's share
    heights: vec4<f32>,
};

// the field's passes: the field and the blocking heights written, from the height pass and
// the screen's last picture
@group(2) @binding(8) var<uniform> gi_params: GiParams;
@group(2) @binding(9) var gi_field_out: texture_storage_2d<rgba16float, write>;
@group(2) @binding(10) var gi_height: texture_2d<f32>;
@group(2) @binding(11) var gi_emit: texture_2d<f32>;
@group(2) @binding(12) var gi_looks: texture_2d<u32>;
@group(2) @binding(13) var gi_block_out: texture_storage_2d<r32float, write>;
// the cascades' and the resolve's: the field and the blocking heights read, the coarser
// cascade read, this one written; the probes written
@group(2) @binding(14) var gi_field: texture_2d<f32>;
@group(2) @binding(15) var gi_block: texture_2d<f32>;
@group(2) @binding(16) var gi_coarser: texture_2d<f32>;
@group(2) @binding(17) var gi_cascade_out: texture_storage_2d<rgba16float, write>;
@group(2) @binding(18) var gi_a_out: texture_storage_2d<rgba16float, write>;
@group(2) @binding(19) var gi_b_out: texture_storage_2d<rgba16float, write>;

// how high what blocks light stands at a texel of the region, from the height pass
fn gi_height_at(texel: vec2<i32>) -> f32 {
    let size = vec2<i32>(textureDimensions(gi_height));
    return textureLoad(gi_height, clamp(texel, vec2<i32>(0, 0), size - vec2<i32>(1, 1)), 0).r;
}

// a point of the world (x, z) in the field's texels, and back (a texel's middle at + 0.5)
fn gi_texel_of(p: vec2<f32>) -> vec2<f32> {
    return (p - gi_params.region.xy) / gi_params.region.z;
}

fn gi_world_of(texel: vec2<f32>) -> vec2<f32> {
    return gi_params.region.xy + texel * gi_params.region.z;
}

@compute @workgroup_size(8, 8)
fn gi_clear(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = u32(gi_params.region.w);
    if (id.x >= n || id.y >= n) {
        return;
    }
    let p = vec2<i32>(id.xy);
    let h = gi_height_at(p);
    var block = h;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            block = max(block, gi_height_at(p + vec2<i32>(i, j)));
        }
    }
    textureStore(gi_field_out, p, vec4<f32>(0.0, 0.0, 0.0, h));
    textureStore(gi_block_out, p, vec4<f32>(block, 0.0, 0.0, 0.0));
}

// what the screen showed shining at a point last frame, but the ground (the field knows
// its glow of itself): where the point lands on the screen, four taps over the point's own
// width (`width`, in the world), each where the screen shows that point or something
// standing on it (up to two cells high: a blade's tip, a flower's head)
fn gi_screen_glow(at: vec3<f32>, width: f32) -> vec3<f32> {
    let clip = g.view_proj * vec4<f32>(at, 1.0);
    if (clip.w <= 0.01) {
        return vec3<f32>(0.0);
    }
    let ndc = clip.xy / clip.w;
    if (abs(ndc.x) >= 1.0 || abs(ndc.y) >= 1.0) {
        return vec3<f32>(0.0);
    }
    let size = vec2<f32>(textureDimensions(gi_looks));
    let px = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * size;
    let clip2 = g.view_proj * vec4<f32>(at + vec3<f32>(width, 0.0, 0.0), 1.0);
    let px2 = vec2<f32>(clip2.x / clip2.w * 0.5 + 0.5, 0.5 - clip2.y / clip2.w * 0.5) * size;
    let r = clamp(length(px2 - px) * 0.25, 0.5, 6.0);
    let d = distance(at, g.camera.xyz);
    var sum = vec3<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        let o = vec2<f32>(select(-r, r, (k & 1) == 1), select(-r, r, k >= 2));
        let t = clamp(vec2<i32>(px + o), vec2<i32>(0, 0), vec2<i32>(size) - vec2<i32>(1, 1));
        let v = textureLoad(gi_looks, t, 0);
        let k2 = 1.0 - f32(v.y) / 65535.0;
        let dist = k2 * k2 * LOOK_FAR;
        if (dist > d - 2.0 && dist < d + 0.75) {
            let e = textureLoad(gi_emit, t, 0);
            sum = sum + e.rgb * (1.0 - e.a);
        }
    }
    return sum * 0.25;
}

// a world map's texels of the field: what shines there and the ground's height
@compute @workgroup_size(8, 8)
fn gi_map(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = u32(gi_params.region.w);
    if (id.x >= n || id.y >= n) {
        return;
    }
    let texel = vec2<i32>(id.xy);
    let p = gi_world_of(vec2<f32>(texel) + 0.5);
    // in the map's own cells
    let local = p - place.offset.xz;
    let size = vec2<f32>(textureDimensions(cells)) - 2.0;
    if (local.x < 0.0 || local.y < 0.0 || local.x >= size.x || local.y >= size.y) {
        return;
    }
    let cell = vec2<i32>(floor(local));
    let c = cell_at(cell);
    let ground = f32(c.y) * g.world.z + place.offset.y;
    var e = vec3<f32>(0.0);
    // nothing shines from under what stands on it (a crate on the lava, a house's floor)
    if (gi_height_at(texel) <= ground + 0.35) {
        let s = terrain_shade(c.x, local + place.grid.xy, 10.0);
        e = s.glow;
        let at = vec3<f32>(p.x, ground, p.y);
        var bounce = vec3<f32>(0.0);
        for (var k = 0; k < i32(g.light_count.x); k = k + 1) {
            bounce = bounce + lamp(k, at, vec3<f32>(0.0, 1.0, 0.0));
        }
        e = e + s.col * bounce * gi_params.heights.z;
        if (gi_params.heights.w > 0.0) {
            e = e + gi_screen_glow(at + vec3<f32>(0.0, 0.02, 0.0), gi_params.region.z) * gi_params.heights.w;
        }
    }
    textureStore(gi_field_out, texel, vec4<f32>(e * (1.0 - place.grid.z), ground));
}

// how much what shines `s` cells away counts on a ray (per cell of it): close, much; past a
// few cells falling as the square of the distance (as a flat glow's light on flat ground
// does), so a lake never sums to more than a few times its edge, its light reaching ten cells
// or so, and a wide one doesn't wash over the whole map
fn gi_falloff(s: f32) -> f32 {
    return 1.0 / pow(1.0 + s * s / 16.0, 1.5);
}

// how much brighter the probes are made than the rays add up to: a glow's light on the
// ground to the eye's liking (the world settings' strength scales it from there)
const GI_GAIN: f32 = 2.5;

// one ray of one probe of a cascade: the texture's texel is the probe's (its square of
// directions, `sqrt(dirs)` a side) and the direction's
@compute @workgroup_size(8, 8)
fn gi_cascade(@builtin(global_invocation_id) id: vec3<u32>) {
    let probes = u32(gi_params.probes.x);
    let dirs = u32(gi_params.probes.y);
    let side = u32(sqrt(f32(dirs)) + 0.5);
    let n = probes * side;
    if (id.x >= n || id.y >= n) {
        return;
    }
    let probe = id.xy / side;
    let sub = id.xy % side;
    let d = sub.y * side + sub.x;
    let angle = (f32(d) + 0.5) / f32(dirs) * TAU;
    let way = vec2<f32>(cos(angle), sin(angle));
    let spacing = gi_params.probes.z;
    let pos = gi_params.region.xy + (vec2<f32>(probe) + 0.5) * spacing;
    let field_size = i32(gi_params.region.w);
    let last = vec2<i32>(field_size - 1, field_size - 1);
    let own = clamp(vec2<i32>(gi_texel_of(pos)), vec2<i32>(0, 0), last);
    let hp = textureLoad(gi_field, own, 0).a;
    var rad = vec3<f32>(0.0);
    var through = 1.0;
    let step = gi_params.probes.w;
    var s = gi_params.reach.x + step * 0.5;
    loop {
        if (s >= gi_params.reach.y) {
            break;
        }
        let t = gi_texel_of(pos + way * s);
        if (t.x < 0.0 || t.y < 0.0 || t.x >= f32(field_size) || t.y >= f32(field_size)) {
            // past the region: nothing out there
            break;
        }
        let texel = vec2<i32>(t);
        let f = textureLoad(gi_field, texel, 0);
        let w = step * gi_falloff(s);
        if (textureLoad(gi_block, texel, 0).r > hp + 0.35) {
            // blocked: what shines at the edge is seen a little; nothing past it
            rad = rad + f.rgb * w * 0.5;
            through = 0.0;
            break;
        }
        rad = rad + f.rgb * w * smoothstep(-0.75, -0.25, f.a - hp);
        s = s + step;
    }
    // past the interval: the coarser cascade's rays from the four probes round this one
    // (each as near as it stands; little from one on another level), this ray's four
    if (through > 0.0 && gi_params.reach.z > 0.5) {
        let cp = i32(gi_params.reach.z);
        let cside = i32(sqrt(gi_params.reach.w) + 0.5);
        let cspacing = spacing * 2.0;
        let q = (pos - gi_params.region.xy) / cspacing - 0.5;
        let b = vec2<i32>(floor(q));
        let fr = q - floor(q);
        var sum = vec3<f32>(0.0);
        var weight = 0.0;
        for (var k = 0; k < 4; k = k + 1) {
            let o = vec2<i32>(k & 1, k >> 1);
            let cpr = clamp(b + o, vec2<i32>(0, 0), vec2<i32>(cp - 1, cp - 1));
            let ctexel = clamp(vec2<i32>(gi_texel_of(gi_params.region.xy + (vec2<f32>(cpr) + 0.5) * cspacing)), vec2<i32>(0, 0), last);
            // (one on another level counts little, and isn't made up for: a plateau's probes
            // take next to nothing from the ground's below)
            let level = select(0.05, 1.0, abs(textureLoad(gi_field, ctexel, 0).a - hp) < 0.5);
            let wk = select(1.0 - fr.x, fr.x, o.x == 1) * select(1.0 - fr.y, fr.y, o.y == 1);
            var r = vec3<f32>(0.0);
            for (var m = 0; m < 4; m = m + 1) {
                let d2 = i32(d) * 4 + m;
                r = r + textureLoad(gi_coarser, cpr * cside + vec2<i32>(d2 % cside, d2 / cside), 0).rgb;
            }
            sum = sum + r * 0.25 * wk * level;
            weight = weight + wk;
        }
        rad = rad + through * sum / max(weight, 0.0001);
    }
    textureStore(gi_cascade_out, vec2<i32>(id.xy), vec4<f32>(rad, through));
}

// a probe of the finest cascade's light, summed up for the lighting: the light from all round
// (its four rays' mean), the way most of it comes from (the rays' ways weighed by their
// brightness, not yet divided by it) and that brightness
struct Probe {
    all: vec3<f32>,
    way: vec2<f32>,
    bright: f32,
};

fn gi_probe(p: vec2<i32>) -> Probe {
    var o: Probe;
    for (var d = 0; d < 4; d = d + 1) {
        let r = textureLoad(gi_coarser, p * 2 + vec2<i32>(d & 1, d >> 1), 0).rgb;
        let angle = (f32(d) + 0.5) / 4.0 * TAU;
        let l = dot(r, vec3<f32>(0.2126, 0.7152, 0.0722));
        o.all = o.all + r * 0.25;
        o.way = o.way + vec2<f32>(cos(angle), sin(angle)) * l;
        o.bright = o.bright + l;
    }
    return o;
}

// the field's texel under a probe of the finest cascade
fn gi_probe_texel(p: vec2<i32>) -> vec2<i32> {
    let last = i32(gi_params.region.w) - 1;
    return clamp(vec2<i32>(gi_texel_of(gi_params.region.xy + (vec2<f32>(p) + 0.5) * gi_params.probes.z)), vec2<i32>(0, 0), vec2<i32>(last, last));
}

// whether a probe stands under something: in a wall, under a crate or a roof
fn gi_covered(texel: vec2<i32>) -> bool {
    return textureLoad(gi_block, texel, 0).r > textureLoad(gi_field, texel, 0).a + 0.35;
}

// the finest cascade's probes as the lighting reads them: the light from all round with the
// ground's height there, the way most of it comes from (a unit when all of it comes one way,
// nothing when it's even all round), and whether the probe stands under something. One that
// does (its own rays all blocked) takes the light of the probes round it that don't, so a
// wall's face is lit as the ground at its foot is, and the ground along it isn't dark.
@compute @workgroup_size(8, 8)
fn gi_resolve(@builtin(global_invocation_id) id: vec3<u32>) {
    let probes = i32(gi_params.probes.x);
    if (i32(id.x) >= probes || i32(id.y) >= probes) {
        return;
    }
    let p = vec2<i32>(id.xy);
    var o = gi_probe(p);
    let own = gi_probe_texel(p);
    let h = textureLoad(gi_field, own, 0).a;
    let covered = gi_covered(own);
    if (covered) {
        var sum: Probe;
        var n = 0.0;
        for (var j = -1; j <= 1; j = j + 1) {
            for (var i = -1; i <= 1; i = i + 1) {
                let q = p + vec2<i32>(i, j);
                if ((i == 0 && j == 0) || q.x < 0 || q.y < 0 || q.x >= probes || q.y >= probes || gi_covered(gi_probe_texel(q))) {
                    continue;
                }
                let r = gi_probe(q);
                sum.all = sum.all + r.all;
                sum.way = sum.way + r.way;
                sum.bright = sum.bright + r.bright;
                n = n + 1.0;
            }
        }
        if (n > 0.0) {
            o.all = sum.all / n;
            o.way = sum.way;
            o.bright = sum.bright;
        }
    }
    textureStore(gi_a_out, p, vec4<f32>(o.all * GI_GAIN, h));
    textureStore(gi_b_out, p, vec4<f32>(o.way / max(o.bright, 0.0001), select(0.0, 1.0, covered), 0.0));
}
