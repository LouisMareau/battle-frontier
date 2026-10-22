// The scene's shaders: the map (lit, with the sun's shadow, the lamps and the indirect light
// of what glows), sprites and effects (unlit, tinted), the shadow pass, the bloom, and the
// blit to the screen (the picture exposed and tone mapped) with the toon look's ink lines.
// What's solid also writes its look (toon or not, how far, which way it faces) to a second
// target, which the lines are found in, and what shines of it to a third, which the bloom
// blurs and the indirect light (`gi.wgsl`, after this) bounces.
// Everything here works on WebGL 2 as well as WebGPU: uniform buffers only, but for the
// skinned models' joints, which WebGPU's draws read from storage (WebGL 2's from a uniform buffer);
// the indirect light's compute shaders run on WebGPU alone (WebGL 2 reads blank probes).

struct Globals {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>,      // xyz: towards the sun
    sun_color: vec4<f32>,    // rgb: colour x intensity / pi
    ambient: vec4<f32>,      // rgb: ambient light / pi
    hemi_sky: vec4<f32>,     // rgb: hemisphere light from above / pi
    hemi_ground: vec4<f32>,  // rgb: ... from below / pi
    fog_color: vec4<f32>,    // rgb, w: fog start
    params: vec4<f32>,       // x: fog end, y: shadows on, z: encode sRGB in the blit
    camera: vec4<f32>,
    world: vec4<f32>,        // the map: x: width, y: height (cells), z: level height, w: time (s)
    editor: vec4<f32>,       // x: grid lines on, y: darken cells outside the focus
    shadow: vec4<f32>,       // x: normal offset (world), y: depth margin (light depth), z: a texel (uv)
    focus: vec4<f32>,        // the editor's focus box: x0, z0, x1, z1 (when editor.y is on)
    light_count: vec4<f32>,  // x: how many lamps
    // the toon look: a: bands, softness, shadow tint's strength, rim; b: shadow tint (rgb),
    // rim width; c: glint, glint size, ink width (pixels of the scene), ink opacity;
    // d: ink colour (rgb), depth step that draws a line (share of the distance);
    // e: x: cos of the fold that draws a line (over 1: none), y: ink lines on, z: colour
    // steps over the whole picture (0: none), w: how soft their edges are
    toon_a: vec4<f32>,
    toon_b: vec4<f32>,
    toon_c: vec4<f32>,
    toon_d: vec4<f32>,
    toon_e: vec4<f32>,
    // the screen back to the world, for the post-process
    inv_view_proj: mat4x4<f32>,
    // the clouds' shadows: a: x how dark (0: none), y 1 / a cloud's width, z how much of the
    // sky, w how soft their edges; b: xy how far they've drifted (cells), z their height, w
    // how far they've morphed
    clouds_a: vec4<f32>,
    clouds_b: vec4<f32>,
    // the old handhelds' screens: a: x their pixel's size in the screen's pixels (0: the
    // screen's own), y the LCD's colours (0: as they are, 1: the GBA's, 2: the GBC's), z how
    // dark the gaps between the pixels, w a channel's steps (31: 15-bit colour; 0: all);
    // b: xy the screen's size in pixels
    retro_a: vec4<f32>,
    retro_b: vec4<f32>,
    // the wind: xy the way it blows on the ground (x, z), z how strong (0 to 1), w how gusty;
    // b: xy how far its gusts have gone (cells), z the sway's phase, w the flutter's
    // (radians); c: x the ripples' phase, y the laps' (turns). Its clocks are added up frame
    // by frame (a change of the wind turns them, never jumps them).
    wind: vec4<f32>,
    wind_b: vec4<f32>,
    wind_c: vec4<f32>,
    // what's kept in sight through the trees and props in front of it (the walker): xyz where,
    // w the see-through cone's radius there (cells; 0: none)
    see_through: vec4<f32>,
    // the picture's light: x the exposure, y how much of the glow's blur (the bloom) is added,
    // z the tone map (0: a soft shoulder, 1: filmic), w what else blooms (the colour's
    // brightness past this)
    tone: vec4<f32>,
    // the indirect light (the radiance cascades, `gi.wgsl`): x how strong (0: none), y 1 / the
    // region's width (cells), z the region's width, w the probes' spacing (cells); b: xy the
    // region's north-west corner (x, z), z the lamps' bounce, w the screen's glow's share
    gi: vec4<f32>,
    gi_b: vec4<f32>,
    // who tramples the grass (the walker, the actors, the creatures roaming), and ghosts of
    // them where they've just been: x how many; each: xyz where they stand (their feet), w how
    // wide they push (cells); `pushed`: x how much each bends it (1 standing there, less the
    // longer gone, as the grass springs back)
    push: vec4<f32>,
    pushers: array<vec4<f32>, 48>,
    pushed: array<vec4<f32>, 48>,
    lights: array<vec4<f32>, 96>, // per lamp: xyz + range, direction + cos outer, colour + cos inner
};

@group(0) @binding(0) var<uniform> g: Globals;

// where a world map's own space sits in the scene (the globals' bind group picks one per
// map; what isn't a world map's reads one of zeros): moved by `offset.xyz`, its north-west
// corner on the world's grid at `grid.xy` (the ground's patterns run on across maps),
// dimmed by `grid.z`
struct Place {
    offset: vec4<f32>,
    grid: vec4<f32>,
};

@group(0) @binding(1) var<uniform> place: Place;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;
// the indirect light's probes (`gi.wgsl`'s `gi_resolve`): a: rgb the light from all round, w
// the ground's height at the probe; b: xy the way most of it comes from (x, z, a unit when
// all of it comes one way), z 1 when the probe stands under something (in a wall, where it
// carries the light of the probes round it)
@group(2) @binding(2) var gi_a: texture_2d<f32>;
@group(2) @binding(3) var gi_b: texture_2d<f32>;

// the indirect light on a point facing `n`: the four probes round it, each as much as it
// stands near (and none that stands on something higher than the point: a roof's, a crate's
// top), the light from all round plus what comes its way; fading up off the ground (what
// stands high over the probes is lit by them less), and at the region's edge
fn indirect(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    if (g.gi.x <= 0.0) {
        return vec3<f32>(0.0);
    }
    let uv = (world.xz - g.gi_b.xy) * g.gi.y;
    if (uv.x <= 0.0 || uv.y <= 0.0 || uv.x >= 1.0 || uv.y >= 1.0) {
        return vec3<f32>(0.0);
    }
    let edge = smoothstep(0.0, 0.12, min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y)));
    let size = vec2<i32>(textureDimensions(gi_a));
    let q = uv * vec2<f32>(size) - 0.5;
    let b = vec2<i32>(floor(q));
    let f = q - floor(q);
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    var ground = 0.0;
    for (var k = 0; k < 4; k = k + 1) {
        let o = vec2<i32>(k & 1, k >> 1);
        let p = clamp(b + o, vec2<i32>(0, 0), size - vec2<i32>(1, 1));
        let a = textureLoad(gi_a, p, 0);
        let d = textureLoad(gi_b, p, 0);
        let w = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1) * (1.0 - smoothstep(0.25, 1.0, a.w - world.y));
        // more of it on the side it comes from, less on the other (a wall's two faces)
        sum = sum + a.rgb * (1.0 + 0.707 * (n.x * d.x + n.z * d.y)) * w;
        weight = weight + w;
        ground = ground + a.w * w;
    }
    if (weight <= 0.0001) {
        return vec3<f32>(0.0);
    }
    let above = max(world.y - ground / weight, 0.0);
    return sum / weight * edge * (1.0 - smoothstep(1.5, 4.0, above)) * g.gi.x;
}

fn fog(rgb: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let depth = distance(world, g.camera.xyz);
    let f = smoothstep(g.fog_color.w, g.params.x, depth);
    return mix(rgb, g.fog_color.rgb, f);
}

// darker outside the editor's focus box, by g.editor.y (0: not at all)
fn focus_dim(rgb: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (g.editor.y > 0.001 && (world.x < g.focus.x || world.z < g.focus.y || world.x > g.focus.z || world.z > g.focus.w)) {
        return rgb * mix(1.0, 0.16, g.editor.y);
    }
    return rgb;
}

// a fading mesh's shadow: vertex alpha under 1 thins it, speck by speck, in the world
fn dissolved(world: vec3<f32>, alpha: f32) -> bool {
    if (alpha >= 0.999) {
        return false;
    }
    let cell = floor(world * 24.0);
    return hash2(cell.xz + vec2<f32>(cell.y * 7.31, cell.y * 1.17)) > alpha;
}

fn lit_by_sun(light_pos: vec4<f32>) -> f32 {
    if (g.params.y < 0.5) {
        return 1.0;
    }
    let p = light_pos.xyz / light_pos.w;
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || p.z > 1.0) {
        return 1.0;
    }
    let texel = g.shadow.z;
    let z = p.z - g.shadow.y;
    var sum = 0.0;
    sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(-0.5, -0.5) * texel, z);
    sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(0.5, -0.5) * texel, z);
    sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(-0.5, 0.5) * texel, z);
    sum += textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(0.5, 0.5) * texel, z);
    return sum * 0.25;
}

// ---------------------------------------------------------------- the map

struct MapIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    // 1: drawn with the toon look; 2: unlit, as drawn (an FX)
    @location(4) toon: f32,
    // how it moves in the wind: sway, flutter, phase, height above its foot (8 cells: 1)
    @location(5) wind: vec4<f32>,
};

struct MapOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
    @location(3) light_pos: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) toon: f32,
};

// what something solid writes: its colour, its look for the ink lines, and what shines of it
// (rgb: lit by nothing; a: 1 on the ground, whose glow the indirect light knows of itself,
// 0 elsewhere), which the bloom blurs and the indirect light bounces
struct Solid {
    @location(0) color: vec4<f32>,
    @location(1) look: vec4<u32>,
    @location(2) emit: vec4<f32>,
};

// how far the look's distances reach
const LOOK_FAR: f32 = 1000.0;

// a direction folded onto a square, -1 to 1 (octahedral)
fn octa(n: vec3<f32>) -> vec2<f32> {
    let p = n.xy / (abs(n.x) + abs(n.y) + abs(n.z));
    if (n.z < 0.0) {
        return (1.0 - abs(p.yx)) * select(vec2<f32>(-1.0), vec2<f32>(1.0), p >= vec2<f32>(0.0));
    }
    return p;
}

fn unocta(e: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(e.x, e.y, 1.0 - abs(e.x) - abs(e.y));
    let t = max(-n.z, 0.0);
    n.x = n.x + select(t, -t, n.x >= 0.0);
    n.y = n.y + select(t, -t, n.y >= 0.0);
    return normalize(n);
}

// the look of a point: toon or not, how far from the eye (near: high; nothing: 0), and
// the way it faces
fn look(world: vec3<f32>, n: vec3<f32>, toon: f32) -> vec4<u32> {
    let d = distance(world, g.camera.xyz);
    let depth = u32(clamp(1.0 - sqrt(d / LOOK_FAR), 0.0, 1.0) * 65535.0);
    let e = vec2<u32>(clamp((octa(normalize(n)) * 0.5 + 0.5) * 65535.0, vec2<f32>(0.0), vec2<f32>(65535.0)));
    return vec4<u32>(select(0u, 1u, toon > 0.5 && toon < 1.5), depth, e.x, e.y);
}

// ---------------------------------------------------------------- the wind

// how much stronger the wind is at a point just now, about 1: gusts are broad patches of
// stronger wind going by with it (as much of it as `g.wind.w` says)
fn gust_at(p: vec2<f32>) -> f32 {
    let q = (p - g.wind_b.xy) * 0.06;
    let n = noise(q) * 0.65 + noise(q * 2.3 + 7.1) * 0.35;
    return mix(1.0, 0.35 + 1.45 * n, g.wind.w);
}

// how far a point of the grass is pushed aside by whoever stands on it or has just walked
// through it (`g.pushers`: where each stands, and ghosts of it where it has just been, each
// bending the grass as much as is left as it springs back, `g.pushed`): away from whichever
// bends it most, the more the nearer (nothing past their reach), only what's about their feet
// (never a crown over their heads, nor a foot's grass under a cliff they stand on), `bend` how
// far the point gives (0 at a root, 1 at a tip)
fn trampled(p: vec3<f32>, bend: f32) -> vec3<f32> {
    var out = vec3<f32>(0.0);
    var most = 0.0;
    let n = i32(g.push.x);
    for (var k = 0; k < n; k++) {
        let q = g.pushers[k];
        let d = p.xz - q.xz;
        let dist = length(d);
        if (dist >= q.w) {
            continue;
        }
        let up = p.y - q.y;
        let near = (1.0 - smoothstep(1.1, 1.8, up)) * smoothstep(-0.8, -0.3, up);
        let away = select(normalize(d), vec2<f32>(1.0, 0.0), dist < 0.001);
        let s = 1.0 - (dist / q.w) * (dist / q.w);
        let push = s * q.w * 0.4 * near * bend * g.pushed[k].x;
        if (push > most) {
            most = push;
            out = vec3<f32>(away.x, 0.0, away.y) * push;
        }
    }
    return out;
}

// how far the wind at strength `s` (0 to 1) bends what gives to it, as a share of its height
// at the tip: a breeze shows (a fifth at 0.1, a third at 0.2), a gale lays it nearly flat;
// what's stiffer gives less by its weights (a painted model's, a terrain's rigidity), never
// by a setting of its own
fn wind_give(s: f32) -> f32 {
    return 0.9 * pow(max(s, 0.0), 0.6);
}

// a point of something painted for the wind, moved by it: `w` its weights (sway, flutter,
// phase, its height above its foot over 8 cells). It leans downwind as far as its sway, its
// height and the wind (with its gust) say, swaying about that, and flutters quickly up and
// down and across the wind; whatever isn't painted stays. Nothing in it goes by the way a
// point faces: the points of one place (where a mesh's faces meet at an edge, as all round a
// leaf made thick, its two faces and its rim) carry the same paint, so they move as one and
// the mesh never comes apart. And whoever walks through it pushes it aside (`trampled`), as
// far as its sway weight gives.
fn swayed(p: vec3<f32>, w: vec4<f32>) -> vec3<f32> {
    if (w.x <= 0.0 && w.y <= 0.0) {
        return p;
    }
    var out = p;
    if (g.push.x > 0.0 && w.x > 0.0) {
        let t = trampled(p, w.x);
        // bent over, a tip comes down a little
        out = out + t - vec3<f32>(0.0, dot(t, t) * 0.5 / max(w.w * 8.0, 0.3), 0.0);
    }
    let s = g.wind.z;
    if (s <= 0.0) {
        return out;
    }
    let way = vec3<f32>(g.wind.x, 0.0, g.wind.y);
    let height = w.w * 8.0;
    // its own time, and a little of where it stands (a gust goes through a wood, not all at once)
    let phase = w.z * 6.2832 + dot(p.xz, vec2<f32>(0.11, 0.07));
    let gust = gust_at(p.xz);
    let lean = wind_give(s) * gust * (0.5 + 0.5 * sin(g.wind_b.z + phase));
    let bend = w.x * lean * (0.18 + 0.22 * height);
    var q = out + way * bend;
    // bent over, a tip comes down a little
    q.y = q.y - w.x * bend * bend * 0.35 / max(height, 0.3);
    let across = vec3<f32>(-way.z, 0.0, way.x);
    let f = w.y * (0.25 + 0.75 * s) * gust * 0.07 * sin(g.wind_b.w + phase * 3.1 + dot(p, vec3<f32>(2.9, 2.1, 3.3)));
    // up and down and across the wind, each piece a little its own way round (by its phase):
    // the same for both faces of what's made two-sided (a leaf with a back a hair behind it),
    // the front staying in front
    let turn = w.z * 37.7;
    let flap = normalize(vec3<f32>(0.0, 0.8, 0.0) + across * (0.6 + 0.3 * cos(turn)) + way * (0.3 * sin(turn)));
    return q + flap * f;
}

@vertex
fn vs_map(v: MapIn) -> MapOut {
    return mapped(v);
}

// a vertex of the map, placed, moved by the wind
fn mapped(v: MapIn) -> MapOut {
    var o: MapOut;
    let pos = swayed(v.pos + place.offset.xyz, v.wind);
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    o.uv = v.uv;
    o.normal = v.normal;
    o.world = pos;
    // looked up a little off the surface, along its normal: no acne, and no margin wide
    // enough to part a shadow from its caster
    o.light_pos = g.light_view_proj * vec4<f32>(pos + v.normal * g.shadow.x, 1.0);
    o.color = v.color;
    o.toon = v.toon;
    return o;
}

// a lamp's light on a point: by how far it is and how far off the lamp's axis
fn lamp(k: i32, world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let p = g.lights[k * 3];
    let d = g.lights[k * 3 + 1];
    let c = g.lights[k * 3 + 2];
    let to = p.xyz - world;
    let dist = length(to);
    if (dist >= p.w) {
        return vec3<f32>(0.0);
    }
    let l = to / max(dist, 0.001);
    let falloff = pow(1.0 - dist / p.w, 1.5);
    let cone = smoothstep(d.w, c.w, dot(-l, d.xyz));
    return c.rgb * falloff * cone * max(dot(n, l), 0.0);
}

fn sunlit(normal: vec3<f32>, light_pos: vec4<f32>, world: vec3<f32>) -> vec3<f32> {
    return sunlit_with(normal, light_pos, world, 1.0);
}

// ... with `bounced` of the indirect light (what shines of itself takes less of it: a lava
// lake lit by its own glow would go white)
fn sunlit_with(normal: vec3<f32>, light_pos: vec4<f32>, world: vec3<f32>, bounced: f32) -> vec3<f32> {
    let n = normalize(normal);
    let ndl = max(dot(n, g.sun_dir.xyz), 0.0);
    let hemi = mix(g.hemi_ground.rgb, g.hemi_sky.rgb, n.y * 0.5 + 0.5);
    var lit = g.ambient.rgb + hemi + g.sun_color.rgb * ndl * lit_by_sun(light_pos);
    for (var k = 0; k < i32(g.light_count.x); k = k + 1) {
        lit = lit + lamp(k, world, n);
    }
    return lit + indirect(world, n) * bounced;
}

// light in steps: `toon_a.x` of them from dark to lit, each step's edge as soft as
// `toon_a.y` says
fn bands(x: f32) -> f32 {
    let n = max(g.toon_a.x, 1.0);
    let k = clamp(x, 0.0, 1.0) * n;
    let s = max(g.toon_a.y, 0.002) * 0.5;
    return (floor(k) + smoothstep(0.5 - s, 0.5 + s, fract(k))) / n;
}

// the toon look's light: the sun's and the lamps' in steps, the shade tinted, a rim of
// light round the edge and a glint where the sun catches
fn toon_light(normal: vec3<f32>, light_pos: vec4<f32>, world: vec3<f32>) -> vec3<f32> {
    let n = normalize(normal);
    let v = normalize(g.camera.xyz - world);
    let sun = bands(max(dot(n, g.sun_dir.xyz), 0.0) * lit_by_sun(light_pos));
    let hemi = mix(g.hemi_ground.rgb, g.hemi_sky.rgb, select(0.3, 0.8, n.y > 0.0));
    var lamps = vec3<f32>(0.0);
    for (var k = 0; k < i32(g.light_count.x); k = k + 1) {
        let c = g.lights[k * 3 + 2].rgb;
        let amount = dot(lamp(k, world, n), vec3<f32>(1.0)) / max(dot(c, vec3<f32>(1.0)), 0.0001);
        lamps = lamps + c * bands(amount);
    }
    let shade = mix(vec3<f32>(1.0), g.toon_b.rgb, g.toon_a.z * (1.0 - sun));
    var lit = (g.ambient.rgb + hemi) * shade + g.sun_color.rgb * sun + lamps;
    let edge = 1.0 - max(dot(n, v), 0.0);
    let rim = smoothstep(1.0 - g.toon_b.w - 0.03, 1.0 - g.toon_b.w + 0.03, edge) * g.toon_a.w * (0.35 + 0.65 * sun);
    let reach = 1.0 - g.toon_c.y * 0.25;
    let glint = smoothstep(reach - 0.004, reach + 0.004, dot(n, normalize(g.sun_dir.xyz + v))) * g.toon_c.x * sun;
    // (the indirect light smooth, whatever the steps: a glow's soft reach)
    return lit + g.sun_color.rgb * (rim + glint) + indirect(world, n);
}

fn light(normal: vec3<f32>, light_pos: vec4<f32>, world: vec3<f32>, toon: f32) -> vec3<f32> {
    if (toon > 1.5) {
        return vec3<f32>(1.0);
    }
    if (toon > 0.5) {
        return toon_light(normal, light_pos, world);
    }
    return sunlit(normal, light_pos, world);
}

// a point of the map lit, its colour `c` (its texture's times its vertex's)
fn shade_map(i: MapOut, c: vec4<f32>) -> Solid {
    var o: Solid;
    o.color = vec4<f32>(fog(focus_dim(c.rgb * (1.0 - place.grid.z) * light(i.normal, i.light_pos, i.world, i.toon), i.world), i.world), 1.0);
    o.look = look(i.world, i.normal, i.toon);
    o.emit = vec4<f32>(0.0);
    return o;
}

@fragment
fn fs_map(i: MapOut) -> Solid {
    let c = textureSample(tex, samp, i.uv) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    return shade_map(i, c);
}

// the same without cut-outs (vertex colours on white): with no discard, the GPU can skip
// what's hidden before shading it
@fragment
fn fs_map_opaque(i: MapOut) -> Solid {
    return shade_map(i, textureSample(tex, samp, i.uv) * i.color);
}

// the 4 x 4 ordered dither's step at a pixel, 0 to 1 (Bayer's matrix, from its bits)
fn dither(frag: vec2<f32>) -> f32 {
    let x = u32(frag.x) & 3u;
    let y = u32(frag.y) & 3u;
    let v = x ^ y;
    let b = ((v & 1u) << 3u) | ((y & 1u) << 2u) | (v & 2u) | ((y & 2u) >> 1u);
    return (f32(b) + 0.5) / 16.0;
}

// whether a point of what stands in front of the walker goes, so they stay in sight: in a
// cone from the eye to them (`see_through.w` wide round them), more of it gone the nearer the
// cone's middle, dithered, never quite all; nothing past them, nor right in front of them
fn seen_through(world: vec3<f32>, frag: vec2<f32>) -> bool {
    let r = g.see_through.w;
    if (r <= 0.0) {
        return false;
    }
    let eye = g.camera.xyz;
    let axis = g.see_through.xyz - eye;
    let t = dot(world - eye, axis) / max(dot(axis, axis), 0.0001);
    if (t <= 0.0 || t >= 0.96) {
        return false;
    }
    let reach = r * t;
    let k = 1.0 - smoothstep(reach * 0.5, reach, distance(world, eye + axis * t));
    return k * 0.85 > dither(frag);
}

// trees and props, with the walker kept in sight through them
@fragment
fn fs_map_through(i: MapOut) -> Solid {
    let c = textureSample(tex, samp, i.uv) * i.color;
    if (c.a < 0.5 || seen_through(i.world, i.clip.xy)) {
        discard;
    }
    return shade_map(i, c);
}

@fragment
fn fs_map_opaque_through(i: MapOut) -> Solid {
    // (sampled first: a texture is read in uniform control flow)
    let c = textureSample(tex, samp, i.uv) * i.color;
    if (seen_through(i.world, i.clip.xy)) {
        discard;
    }
    return shade_map(i, c);
}

// a fading mesh: its depth alone first (its nearest surfaces, so each pixel blends once)
@fragment
fn fs_map_depth(i: MapOut) -> @location(0) vec4<f32> {
    if (textureSample(tex, samp, i.uv).a < 0.5) {
        discard;
    }
    return vec4<f32>(0.0);
}

// then its colour, as see-through as its vertex alpha says
@fragment
fn fs_map_fade(i: MapOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.5) {
        discard;
    }
    let c = t.rgb * i.color.rgb * (1.0 - place.grid.z);
    return vec4<f32>(fog(focus_dim(c * light(i.normal, i.light_pos, i.world, i.toon), i.world), i.world), i.color.a);
}

// ---------------------------------------------------------------- the world's ground
// Cell tops and cliff faces: each vertex carries its cell's centre in uv (a face, the cell
// above it's), and in its colour's alpha how much rock it is: 0 on the tops, 1 on the
// faces, between on the bevels along the cliffs' tops (the faces' rgb darkens their foot).
// The terrain of every cell is in `cells` (r: terrain, g: level); `palette` has a column
// per terrain: row 0 its colour, row 1 its second colour, row 2 (pattern, priority + 128,
// water: 0 none, 1 inland, 2 oceanic; beach), row 3 its cliffs' colour. Where terrains
// meet, the border is drawn here: each of the four nearest cells votes for its terrain by
// closeness, with noise, so borders come out organic and corners round, whatever was
// painted, and the painter never picks border tiles. Where water meets land, `shore` says
// how far each cell's centre is from the shore (negative on the water), and the water's edge
// runs along it, wavering: the waves and the foam, and the swash up a beach, go by it.

@group(1) @binding(2) var cells: texture_2d<f32>;
@group(1) @binding(3) var palette: texture_2d<f32>;
@group(1) @binding(4) var shore: texture_2d<f32>;

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    return (noise(p) * 0.5 + noise(p * 2.03 + 17.1) * 0.25 + noise(p * 4.1 + 31.7) * 0.125) / 0.875;
}

// a cell of the map (its own coordinates; the texture has a ring of the cells around the
// map first, the next maps' where there are some)
fn cell_at(c: vec2<i32>) -> vec2<i32> {
    let size = vec2<i32>(textureDimensions(cells));
    let t = textureLoad(cells, clamp(c + vec2<i32>(1, 1), vec2<i32>(0, 0), size - vec2<i32>(1, 1)), 0);
    return vec2<i32>(i32(round(t.r * 255.0)), i32(round(t.g * 255.0)));
}

// is the cell in the editor's focus (the cells texture's blue channel)?
fn focused(c: vec2<i32>) -> bool {
    let size = vec2<i32>(textureDimensions(cells));
    return textureLoad(cells, clamp(c + vec2<i32>(1, 1), vec2<i32>(0, 0), size - vec2<i32>(1, 1)), 0).b > 0.5;
}

fn pal(t: i32, row: i32) -> vec4<f32> {
    return textureLoad(palette, vec2<i32>(t, row), 0);
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3<f32>(2.2));
}

// a terrain's colour at p; `inside` is 0 on its border and grows further in
fn terrain_color(t: i32, p: vec2<f32>, inside: f32) -> vec3<f32> {
    let c1 = to_linear(pal(t, 0).rgb);
    let c2 = to_linear(pal(t, 1).rgb);
    let kind = i32(round(pal(t, 2).r * 255.0));
    var c = c1;
    switch kind {
        case 1: { // grass
            c = mix(c1, c2, smoothstep(0.35, 0.8, fbm(p * 0.45)) * 0.75);
            c = c * (0.93 + 0.14 * noise(p * vec2<f32>(13.0, 9.0)));
        }
        case 2: { // meadow: grass with flowers
            c = mix(c1, c1 * 0.86, fbm(p * 0.5));
            let q = p * 3.0;
            let k = floor(q);
            let centre = k + vec2<f32>(0.25 + 0.5 * hash2(k + 3.1), 0.25 + 0.5 * hash2(k + 7.7));
            if (hash2(k) > 0.7 && distance(q, centre) < 0.14) {
                c = c2;
            }
        }
        case 3: { // under tall grass
            c = mix(c1, c2, noise(p * 6.0)) * 0.8;
        }
        case 4: { // dirt, with pebbles
            c = mix(c1, c2, smoothstep(0.3, 0.8, fbm(p * 0.9)));
            let q = p * 5.0;
            if (hash2(floor(q)) > 0.86 && distance(fract(q), vec2<f32>(0.5, 0.5)) < 0.2) {
                c = c2 * 0.82;
            }
        }
        case 5: { // sand
            c = mix(c1, c2, fbm(p * 1.3) * 0.6);
            c = c * (0.95 + 0.08 * hash2(floor(p * 24.0)));
            c = c * (0.97 + 0.03 * sin(p.x * 3.0 + fbm(p) * 4.0));
        }
        case 6: { // stones: cobbles
            let q = p * 1.7;
            let base = floor(q);
            var d1 = 9.0;
            var d2 = 9.0;
            var id = vec2<f32>(0.0, 0.0);
            for (var j = -1; j <= 1; j = j + 1) {
                for (var i = -1; i <= 1; i = i + 1) {
                    let k = base + vec2<f32>(f32(i), f32(j));
                    let o = k + vec2<f32>(hash2(k), hash2(k + 5.3)) * 0.8 + 0.1;
                    let d = distance(q, o);
                    if (d < d1) {
                        d2 = d1;
                        d1 = d;
                        id = k;
                    } else if (d < d2) {
                        d2 = d;
                    }
                }
            }
            let stone = c1 * (0.82 + 0.25 * hash2(id + 9.1));
            c = mix(c2 * 0.75, stone, smoothstep(0.03, 0.09, d2 - d1));
        }
        case 7: { // rock
            c = mix(c1, c2, fbm(p * 0.8));
            c = c * mix(0.72, 1.0, smoothstep(0.0, 0.05, abs(noise(p * 2.5) - 0.5)));
        }
        default: {}
    }
    return c;
}

// ---------------------------------------------------------------- a look of a terrain's own

// a terrain at a point as drawn (`terrain_shade`): its colour, what glows of it, how much
// higher it stands, how glossy it is
struct Shade {
    col: vec3<f32>,
    glow: vec3<f32>,
    height: f32,
    shine: f32,
};

// a size from the palette (`world::mesh::size_byte`): cells, 1/64 to 32
fn size_of(v: f32) -> f32 {
    return exp2(v * 11.0 - 6.0);
}

// the way an angle from the palette points (a turn from 0 to 1, clockwise from east seen from
// above: x east, z south)
fn way_of(turn: f32) -> vec2<f32> {
    return vec2<f32>(cos(turn * TAU), sin(turn * TAU));
}

// a point as seen along that way: how far along it, and across it
fn along(p: vec2<f32>, turn: f32) -> vec2<f32> {
    let w = way_of(turn);
    return vec2<f32>(dot(p, w), dot(p, vec2<f32>(-w.y, w.x)));
}

// how bright a glow is just now, pulsing `speed` times a second from its own `phase` (0:
// steady)
fn pulsing(speed: f32, phase: f32) -> f32 {
    if (speed <= 0.0) {
        return 1.0;
    }
    return 0.65 + 0.35 * sin(g.world.w * speed * TAU + phase);
}

// the nearest two points of a Voronoi pattern (one a cell, as far off its middle as `jitter`
// says) to q: how far each is, and the nearest's cell
struct Near {
    d1: f32,
    d2: f32,
    id: vec2<f32>,
};

fn near_points(q: vec2<f32>, jitter: f32) -> Near {
    let base = floor(q);
    var c: Near;
    c.d1 = 9.0;
    c.d2 = 9.0;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let k = base + vec2<f32>(f32(i), f32(j));
            let o = k + 0.5 + (vec2<f32>(hash2(k + 1.7), hash2(k + 8.3)) - 0.5) * jitter;
            let d = distance(q, o);
            if (d < c.d1) {
                c.d2 = c.d1;
                c.d1 = d;
                c.id = k;
            } else if (d < c.d2) {
                c.d2 = d;
            }
        }
    }
    return c;
}

// a layer's mark at a point of its own (`q`: the point over its size): how much of the layer
// is there (0 to 1), how high it stands there, and a little of its own (0 to 1: one stone's,
// one dot's) to vary its colour and its pulse. Its shapes as `content::LAYER_SHAPES` names
// them, from 1.
struct Mark {
    m: f32,
    h: f32,
    own: f32,
};

fn layer_mark(shape: i32, q: vec2<f32>, amount: f32, softness: f32, turn: f32) -> Mark {
    var k: Mark;
    let s = max(softness, 0.02);
    switch shape {
        case 1: { // patches
            k.m = smoothstep(1.0 - amount - s * 0.5, 1.0 - amount + s * 0.5, fbm(q));
            k.h = k.m;
            k.own = noise(q * 3.1);
        }
        case 2: { // stones: cells with gaps between them, covering as much as `amount` says
            let c = near_points(q, 0.8);
            let gap = (1.0 - amount) * 0.35;
            k.m = smoothstep(gap, gap + 0.02 + s * 0.15, c.d2 - c.d1);
            k.h = k.m * smoothstep(0.0, 0.3, c.d2 - c.d1);
            k.own = hash2(c.id + 4.7);
        }
        case 3: { // cracks: a net of thin lines, wavering
            let w = q + vec2<f32>(noise(q * 1.3), noise(q * 1.3 + 7.7)) * 0.6;
            let c = near_points(w, 0.9);
            let width = 0.02 + amount * 0.2;
            k.m = 1.0 - smoothstep(width, width + 0.02 + s * 0.2, c.d2 - c.d1);
            k.h = k.m;
            k.own = hash2(c.id);
        }
        case 4: { // dots: round, scattered, as many as `amount` says
            let c = near_points(q, 0.7);
            let on = step(hash2(c.id + 2.3), amount);
            let r = 0.22 + 0.16 * hash2(c.id + 5.9);
            k.m = on * (1.0 - smoothstep(r - s * 0.2, r + 0.02, c.d1));
            k.h = k.m * clamp(1.0 - c.d1 / r, 0.0, 1.0);
            k.own = hash2(c.id + 9.1);
        }
        case 5: { // streaks: long marks along its way
            let a = along(q, turn);
            k.m = smoothstep(1.0 - amount - s * 0.5, 1.0 - amount + s * 0.5, fbm(vec2<f32>(a.x * 0.25, a.y * 2.0)));
            k.h = k.m;
            k.own = noise(a * 2.0);
        }
        case 6: { // veins: thin wavering lines, where its noise crosses its middle
            let ridge = 1.0 - abs(fbm(q) * 2.0 - 1.0);
            let line = 0.98 - amount * 0.16;
            k.m = smoothstep(line - s * 0.2, line + 0.01, ridge);
            k.h = k.m;
            k.own = noise(q * 2.3);
        }
        case 7: { // ripples: waves across its way, wavering
            let a = along(q, turn);
            let wave = sin((a.x + fbm(q * 0.5) * 1.5) * TAU);
            let at = 1.0 - 2.0 * amount;
            k.m = smoothstep(at - s, at + s, wave);
            k.h = wave * 0.5 + 0.5;
            k.own = 0.5;
        }
        default: {}
    }
    return k;
}

// a terrain's look of its own at p (the world's grid), as its palette says
// (`world::mesh::look_rows`): its two colours in blotches, drifting along its way, grainy,
// maybe glowing; then each layer over it, its colour varied a little thing to thing, its glow
// pulsing thing by thing, its relief raising or sinking it
fn own_look(t: i32, p: vec2<f32>) -> Shade {
    var o: Shade;
    let base = pal(t, 4);
    let feel = pal(t, 5);
    let more = pal(t, 6);
    let q = (p - way_of(more.x) * feel.w * 2.0 * g.world.w) / size_of(base.x);
    let s = mix(0.35, 0.02, base.y);
    var col = mix(to_linear(pal(t, 0).rgb), to_linear(pal(t, 1).rgb), smoothstep(0.5 - s, 0.5 + s, fbm(q) + base.z - 0.5));
    col = col * (1.0 + (hash2(floor(p * 24.0)) - 0.5) * base.w * 0.6);
    o.glow = col * feel.y * 4.0 * pulsing(feel.z * 2.0, 0.0);
    o.shine = feel.x;
    let layers = i32(round(more.y * 255.0));
    for (var k = 0; k < 4; k = k + 1) {
        if (k >= layers) {
            break;
        }
        let a = pal(t, 7 + 3 * k);
        let b = pal(t, 8 + 3 * k);
        let c = pal(t, 9 + 3 * k);
        let lq = (p - way_of(c.w) * c.y * 2.0 * g.world.w) / size_of(b.y) + f32(k) * 17.31;
        let mk = layer_mark(i32(round(b.x * 255.0)), lq, b.z, b.w, c.w);
        let lc = to_linear(a.rgb) * (0.85 + 0.3 * mk.own);
        col = mix(col, lc, mk.m);
        o.glow = o.glow * (1.0 - mk.m) + lc * a.w * 4.0 * mk.m * pulsing(c.x * 2.0, mk.own * TAU);
        o.height = o.height + mk.h * (c.z * 2.0 - 1.0);
    }
    o.col = col;
    return o;
}

// a terrain at p as drawn: the engine's patterns their colour (`terrain_color`), a look of its
// own (pattern 9) with what glows of it, its relief and its shine
fn terrain_shade(t: i32, p: vec2<f32>, inside: f32) -> Shade {
    if (i32(round(pal(t, 2).r * 255.0)) == 9) {
        return own_look(t, p);
    }
    var o: Shade;
    o.col = terrain_color(t, p, inside);
    return o;
}

// a normal tilted by a height's slope across the picture (`dh`: its change a pixel right and
// a pixel down; `wx`, `wy` the world's), the surface's own way round (Mikkelsen's surface
// gradient)
fn bumped(n: vec3<f32>, wx: vec3<f32>, wy: vec3<f32>, dh: vec2<f32>) -> vec3<f32> {
    let r1 = cross(wy, n);
    let r2 = cross(n, wx);
    let det = dot(wx, r1);
    let grad = sign(det) * (dh.x * r1 + dh.y * r2);
    return normalize(abs(det) * n - grad);
}

// ---------------------------------------------------------------- water

// a terrain's water: 0 none, 1 inland, 2 oceanic; a beach (the water washes onto it)
fn water_kind(t: i32) -> i32 {
    return i32(round(pal(t, 2).b * 255.0));
}

fn is_beach(t: i32) -> bool {
    return pal(t, 2).a > 0.5;
}

// the shore at a point of the map (its own cells): how far it is (cells; negative out on
// the water) and which way the land is (none where there's no telling), from the field at
// the four nearest cells' centres (r the distance, gb the way, smoothed: it turns gently)
struct Shore {
    d: f32,
    land: vec2<f32>,
};

fn shore_at(p: vec2<f32>) -> Shore {
    let size = vec2<i32>(textureDimensions(shore)) - vec2<i32>(1, 1);
    // the field has the ring of cells round the map first: a cell's centre is a texel's
    let q = p + 0.5;
    let b = vec2<i32>(floor(q));
    let f = q - floor(q);
    let v00 = textureLoad(shore, clamp(b, vec2<i32>(0, 0), size), 0);
    let v10 = textureLoad(shore, clamp(b + vec2<i32>(1, 0), vec2<i32>(0, 0), size), 0);
    let v01 = textureLoad(shore, clamp(b + vec2<i32>(0, 1), vec2<i32>(0, 0), size), 0);
    let v11 = textureLoad(shore, clamp(b + vec2<i32>(1, 1), vec2<i32>(0, 0), size), 0);
    let v = mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
    var s: Shore;
    s.d = v.r;
    s.land = select(vec2<f32>(0.0, 0.0), v.gb / max(length(v.gb), 0.0001), length(v.gb) > 0.05);
    return s;
}

// how far off the field's own line the water's edge runs, wavering, so coasts come out
// organic (within half a cell: the four nearest cells always hold both)
fn shore_wobble(pg: vec2<f32>) -> f32 {
    return (noise(pg * 1.7) - 0.5) * 0.36 + (noise(pg * 4.3 + 9.1) - 0.5) * 0.12;
}

// a point of the ground as drawn: its colour, the way it faces (the water's waves tilt it),
// how much the sun glints on it, what glows of it (a look of a terrain's own: lit by
// nothing) and how much higher it stands (its layers' relief, for the light)
struct Ground {
    col: vec3<f32>,
    n: vec3<f32>,
    shine: f32,
    glow: vec3<f32>,
    height: f32,
};

const TAU: f32 = 6.2831853;
const FOAM: vec3<f32> = vec3<f32>(0.88, 0.94, 0.97);

// the sea's waves here: rough by default, rougher when the wind blows to the land, calmer
// when it blows out to sea
fn swell(land: vec2<f32>) -> f32 {
    return 0.75 * (1.0 + 0.65 * g.wind.z * dot(g.wind.xy, land));
}

// how hard the wind pushes inland water onto this shore: as much as it blows to the land
fn onshore(land: vec2<f32>) -> f32 {
    return clamp(dot(g.wind.xy, land), 0.0, 1.0) * g.wind.z;
}

// the water of terrain `t` (its kind: 1 inland, 2 oceanic) at `pg` (the world's grid), `d`
// from its edge (cells, negative out on it), the land that way. Inland: ripples going with
// the wind (barely any when it's calm), the calm water's slow glints, small waves landing on
// the shore the wind blows to, foam along it. Oceanic: waves always rolling in to the land,
// shorter and steeper as it shoals, breaking near the shore in foam.
fn water(kind: i32, t: i32, pg: vec2<f32>, d: f32, land_way: vec2<f32>) -> Ground {
    let time = g.world.w;
    let c1 = to_linear(pal(t, 0).rgb);
    let c2 = to_linear(pal(t, 1).rgb);
    let s = g.wind.z;
    let way = g.wind.xy;
    // far from any land the field has no slope: there the waves go with the wind
    let land = select(way, land_way, dot(land_way, land_way) > 0.5);
    let depth = max(-d, 0.0);
    // deep out there, lighter in the shallows
    var col = mix(c2 * 0.9, c1, smoothstep(0.0, select(2.2, 4.5, kind == 2), depth));
    if (kind == 2) {
        col = mix(col, c1 * 0.72, smoothstep(5.0, 16.0, depth));
    }
    col = col * (0.94 + 0.12 * fbm((pg - g.wind_b.xy * 0.04) * 0.18));
    var slope = vec2<f32>(0.0, 0.0);
    var foam = 0.0;
    if (kind == 2) {
        // (the waves' speed never changes from place to place: only how big they are)
        let k = mix(0.45, 1.25, smoothstep(9.0, 0.0, depth));
        let strong = swell(land);
        let ph = d * k - time * 0.38 + fbm(pg * 0.22) * 0.8;
        let wave = 0.5 + 0.5 * sin(ph * TAU);
        let crest = smoothstep(0.8, 0.97, wave);
        col = col * (1.0 - 0.12 * strong * (1.0 - wave));
        col = mix(col, c2 * 1.1, crest * 0.3 * min(strong, 1.2));
        slope = land * cos(ph * TAU) * 0.22 * strong;
        let breaking = smoothstep(2.6, 0.3, depth) * strong;
        foam = foam + crest * breaking * (0.5 + 0.5 * noise(pg * 2.6 + vec2<f32>(time * 0.4, 0.0)));
        let wash = 0.5 + 0.5 * sin((time * 0.38 - d * k) * TAU);
        foam = foam + smoothstep(0.22 + 0.25 * strong * wash, 0.0, depth) * (0.6 + 0.4 * wash);
    } else {
        let across = vec2<f32>(-way.y, way.x);
        let u = dot(pg, way);
        let v = dot(pg, across);
        // the ripples and the glints a cell off the shore, at least: there the shore's own
        // waves and foam have it
        let off = smoothstep(1.0, 1.6, depth);
        let ph = u * 2.4 - g.wind_c.x + fbm(vec2<f32>(u * 0.45, v * 1.2)) * 2.0;
        let lines = smoothstep(0.84, 0.97, 0.5 + 0.5 * sin(ph * TAU));
        let broken = smoothstep(0.4, 0.7, noise(vec2<f32>(u * 0.9 - time * 0.4, v * 2.2)));
        col = mix(col, c2 * 1.12, lines * broken * (0.12 + 0.88 * s) * gust_at(pg) * 0.6 * off);
        slope = way * cos(ph * TAU) * 0.12 * (0.15 + 0.85 * s) * off;
        let w = noise(pg * 1.2 + vec2<f32>(time * 0.25, time * 0.1)) + noise(pg * 2.1 - vec2<f32>(time * 0.15, time * 0.3));
        col = mix(col, c2, smoothstep(1.25, 1.5, w) * 0.4 * (1.0 - 0.6 * s) * off);
        let on = onshore(land);
        let lap = g.wind_c.y + depth * 4.5;
        let laps = smoothstep(0.72, 0.95, 0.5 + 0.5 * sin(lap * TAU)) * smoothstep(1.0, 0.15, depth);
        foam = foam + laps * (0.12 + 0.75 * on);
        foam = foam + smoothstep(0.1 + 0.12 * on * (0.5 + 0.5 * sin(lap * TAU)), 0.0, depth) * (0.35 + 0.6 * on);
    }
    let f = clamp(foam, 0.0, 1.0);
    var out: Ground;
    out.col = mix(col, FOAM, f);
    out.n = normalize(vec3<f32>(-slope.x, 1.0, -slope.y));
    out.shine = 1.0 - f;
    return out;
}

// the water washing up a beach and back: `sand` the beach's colour, `d` how far up it the
// point is (cells), the land that way, the water's kind and terrain. Up quickly, back slowly,
// each stretch of the beach in its own time, as far as the waves (the sea's) or the wind
// pushing onto the shore (inland water) say; the sand it reached darker, wet, and foam along
// its edge.
fn swash(sand: vec3<f32>, kind: i32, wt: i32, pg: vec2<f32>, d: f32, land: vec2<f32>) -> Ground {
    let time = g.world.w;
    // (its time never changes from place to place but by smooth noise: only how far it goes)
    var reach = 0.12 + 0.5 * onshore(land);
    var period = 3.2;
    if (kind == 2) {
        reach = 0.45 + 0.7 * swell(land);
        period = 5.0;
    }
    let cycle = fract(time / period + noise(pg * 0.16) * 0.8 + noise(pg * 0.05 + 3.3) * 0.4);
    // up quickly, back slowly: smooth through its turn and from one wave to the next
    let up = smoothstep(0.0, 0.3, cycle) * (1.0 - smoothstep(0.3, 1.0, cycle));
    let front = reach * up + (noise(pg * 3.7 + vec2<f32>(time * 0.2, 0.0)) - 0.5) * 0.07;
    // the sand the water reaches, wet: darker the nearer the water, fading softly past the
    // furthest it goes (where it is just now doesn't move it)
    let wet = (1.0 - smoothstep(reach * 0.3, reach + 0.12, d)) * 0.2;
    // the water's sheet, thinning to its edge
    let sheet = 1.0 - smoothstep(front - 0.12, front + 0.02, d);
    let thin = 1.0 - smoothstep(front - 0.3, front, d);
    var out: Ground;
    out.n = vec3<f32>(0.0, 1.0, 0.0);
    out.shine = mix(wet * 0.6, 0.8, sheet);
    let col = mix(sand * (1.0 - wet), to_linear(pal(wt, 1).rgb) * 0.95, (0.22 + 0.3 * thin) * sheet);
    let advancing = 1.0 - smoothstep(0.22, 0.36, cycle);
    let edge = smoothstep(0.07, 0.0, abs(d - front)) * (0.55 + 0.35 * advancing) * smoothstep(0.0, 0.05, front);
    out.col = mix(col, FOAM, edge);
    return out;
}

fn score(ts: array<i32, 4>, ws: array<f32, 4>, t: i32, p: vec2<f32>) -> f32 {
    var w = 0.0;
    if (ts[0] == t) { w = w + ws[0]; }
    if (ts[1] == t) { w = w + ws[1]; }
    if (ts[2] == t) { w = w + ws[2]; }
    if (ts[3] == t) { w = w + ws[3]; }
    let priority = pal(t, 2).g * 255.0 - 128.0;
    return w + priority * 0.035 + (noise(p * 2.2 + f32(t) * 13.7) - 0.5) * 0.42 + (noise(p * 5.5 + f32(t) * 3.1) - 0.5) * 0.12;
}

// rock, in a terrain's cliff colour `c`: slabs (a little warped, much wider than tall,
// like layers of rock) with cracks between some of them, some greyer, some warmer, in wavering layers, grainy, with
// streaks running down; `q` runs along the face and up it. Also how the block leans (for
// the light).
struct Rock {
    color: vec3<f32>,
    tilt: vec2<f32>,
};

fn rock(c: vec3<f32>, q: vec2<f32>) -> Rock {
    let w = q + vec2<f32>(fbm(q * 0.7), fbm(q * 0.7 + 5.2)) * 0.55;
    let s = w * vec2<f32>(0.75, 2.3);
    let base = floor(s);
    var d1 = 9.0;
    var d2 = 9.0;
    var id = vec2<f32>(0.0, 0.0);
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let k = base + vec2<f32>(f32(i), f32(j));
            let o = k + vec2<f32>(hash2(k + 1.7), hash2(k + 8.3)) * 0.8 + 0.1;
            let d = distance(s, o);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                id = k;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    let grey = vec3<f32>(dot(c, vec3<f32>(0.3, 0.55, 0.15)));
    var col = mix(c, grey, 0.1 + 0.35 * hash2(id + 3.3)) * (0.8 + 0.32 * hash2(id + 4.7));
    col = col * (0.9 + 0.13 * sin(q.y * 6.0 + fbm(q * vec2<f32>(0.5, 1.6)) * 5.0));
    col = col * (0.86 + 0.22 * fbm(q * 3.7 + 11.0));
    col = col * (0.9 + 0.14 * noise(vec2<f32>(q.x * 7.0, q.y * 0.35)));
    let crack = (1.0 - smoothstep(0.02, 0.07, d2 - d1)) * smoothstep(0.3, 0.6, fbm(q * 1.9 + 3.0));
    var r: Rock;
    r.color = mix(col, c * 0.4, crack * 0.7);
    r.tilt = (vec2<f32>(hash2(id + 2.9), hash2(id + 6.1)) - 0.5) * 0.45;
    return r;
}

@fragment
fn fs_ground(i: MapOut) -> Solid {
    // in the map's own cells, and on the world's grid (where the patterns are drawn, so
    // they run on from one map into the next)
    let p = i.world.xz - place.offset.xz;
    let pg = p + place.grid.xy;
    let own_cell = vec2<i32>(floor(i.uv));
    let own = cell_at(own_cell);
    let rocky = i.color.a;
    var n = normalize(i.normal);
    // the surface's own facet (a cliff face is lumpy; its vertices keep the flat normal)
    let facet = cross(dpdx(i.world), dpdy(i.world));
    var col = vec3<f32>(0.0);
    var shine = 0.0;
    var glow = vec3<f32>(0.0);
    var height = 0.0;
    let fc = p - vec2<f32>(own_cell);
    if (rocky < 0.999) {
        let gr = ground_color(own_cell, own, p, pg, fc);
        col = gr.col;
        shine = gr.shine;
        glow = gr.glow;
        height = gr.height;
        // a top tilts as the water's waves say
        if (rocky < 0.001) {
            n = gr.n;
        }
    }
    // a look's relief tilts the light as its height runs across the picture (worked out
    // here, where every pixel of a quad takes the same way)
    let wx = dpdx(i.world);
    let wy = dpdy(i.world);
    let dh = vec2<f32>(dpdx(height), dpdy(height)) * 0.05;
    if (rocky < 0.001 && (dh.x != 0.0 || dh.y != 0.0)) {
        n = bumped(n, wx, wy, dh);
    }
    if (rocky > 0.001) {
        // along the face (across its normal, on the ground) and up it
        let level_n = vec2<f32>(n.x, n.z);
        let h = select(vec2<f32>(1.0, 0.0), normalize(level_n), length(level_n) > 0.01);
        let along = vec2<f32>(-h.y, h.x);
        let q = vec2<f32>(dot(pg, along), i.world.y);
        let r = rock(to_linear(pal(own.x, 3).rgb), q);
        // on a bevel the rock shows through the ground raggedly, more of it further down
        let k = select(1.0, smoothstep(0.3, 0.7, rocky + (fbm(q * 2.7) - 0.5) * 0.6), rocky < 0.999);
        col = mix(col, r.color * i.color.rgb, k);
        if (rocky >= 0.999 && length(facet) > 1e-12) {
            let f = normalize(facet);
            n = select(-f, f, dot(f, n) > 0.0);
        }
        // each block leans a little its own way, and catches the light so
        let side = vec3<f32>(along.x, 0.0, along.y);
        let up = normalize(cross(side, n));
        n = normalize(n + (side * r.tilt.x + up * r.tilt.y) * k);
    }
    if (g.editor.y > 0.5 && !focused(own_cell)) {
        col = col * 0.16;
        glow = glow * 0.16;
    }
    if (g.editor.x > 0.5 && rocky < 0.999) {
        let line = min(min(fc.x, 1.0 - fc.x), min(fc.y, 1.0 - fc.y));
        col = col * mix(0.72, 1.0, smoothstep(0.012, 0.035, line));
    }
    let bounced = 1.0 - clamp(dot(glow, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    var lit = col * (1.0 - place.grid.z) * sunlit_with(n, i.light_pos, i.world, bounced);
    // the sun glinting on the water, and on wet sand
    if (shine > 0.0) {
        let h = normalize(g.sun_dir.xyz + normalize(g.camera.xyz - i.world));
        lit = lit + g.sun_color.rgb * pow(max(dot(n, h), 0.0), 120.0) * shine * 0.8 * lit_by_sun(i.light_pos);
    }
    // what glows of itself, whatever the light (lava, magic: at night as by day)
    let shining = glow * (1.0 - place.grid.z);
    lit = lit + shining;
    var o: Solid;
    o.color = vec4<f32>(fog(lit, i.world), 1.0);
    o.look = look(i.world, n, 0.0);
    o.emit = vec4<f32>(shining, 1.0);
    return o;
}

// the four nearest cells' vote: the terrain that wins, by how much, and the runner-up
// (`dry`: water doesn't stand)
struct Vote {
    bt: i32,
    best: f32,
    st: i32,
    second: f32,
};

fn vote(ts: array<i32, 4>, f: vec2<f32>, pg: vec2<f32>, dry: bool) -> Vote {
    let ws = array<f32, 4>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
    var tv = ts;
    var v: Vote;
    v.bt = tv[0];
    v.best = -9.0;
    v.st = tv[0];
    v.second = -9.0;
    for (var k = 0; k < 4; k = k + 1) {
        let t = tv[k];
        if (dry && water_kind(t) > 0) {
            continue;
        }
        let s = score(ts, ws, t, pg);
        if (s > v.best) {
            if (t != v.bt) {
                v.second = v.best;
                v.st = v.bt;
            }
            v.best = s;
            v.bt = t;
        } else if (t != v.bt && s > v.second) {
            v.second = s;
            v.st = t;
        }
    }
    return v;
}

// the land where the vote went (its border with the runner-up drawn soft): its colour, what
// glows of it, its relief, its shine
fn voted_shade(v: Vote, pg: vec2<f32>) -> Shade {
    var s = terrain_shade(v.bt, pg, v.best - v.second);
    if (v.second > -8.0) {
        let other = terrain_shade(v.st, pg, 0.0);
        let k = 0.5 + 0.5 * smoothstep(0.0, 0.04, v.best - v.second);
        s.col = mix(other.col, s.col, k);
        s.glow = mix(other.glow, s.glow, k);
        s.height = mix(other.height, s.height, k);
        s.shine = mix(other.shine, s.shine, k);
    }
    return s;
}

// a point of the ground drawn as a terrain's shade says
fn shaded(out: Ground, s: Shade) -> Ground {
    var o = out;
    o.col = s.col;
    o.glow = s.glow;
    o.height = s.height;
    o.shine = max(o.shine, s.shine);
    return o;
}

// a cell top at p (in the map's cells; `pg`, the same point on the world's grid,
// for the patterns): its terrain, the borders with the terrains around, and the shade at
// the foot of a cliff
fn ground_color(own_cell: vec2<i32>, own: vec2<i32>, p: vec2<f32>, pg: vec2<f32>, fc: vec2<f32>) -> Ground {
    let q = p - 0.5;
    let b = vec2<i32>(floor(q));
    let f = q - floor(q);
    // the four nearest cells; one on another level counts as this cell's terrain
    let c0 = cell_at(b);
    let c1 = cell_at(b + vec2<i32>(1, 0));
    let c2 = cell_at(b + vec2<i32>(0, 1));
    let c3 = cell_at(b + vec2<i32>(1, 1));
    let ts = array<i32, 4>(
        select(own.x, c0.x, c0.y == own.y),
        select(own.x, c1.x, c1.y == own.y),
        select(own.x, c2.x, c2.y == own.y),
        select(own.x, c3.x, c3.y == own.y),
    );
    var out: Ground;
    out.n = vec3<f32>(0.0, 1.0, 0.0);
    out.shine = 0.0;
    // water among the four nearest (on this level), and land
    var tv = ts;
    var wt = -1;
    var dry = false;
    // the terrain drawn here, on the land
    var drawn = own.x;
    for (var k = 0; k < 4; k = k + 1) {
        if (water_kind(tv[k]) > 0) {
            if (wt < 0) {
                wt = tv[k];
            }
        } else {
            dry = true;
        }
    }
    if (wt >= 0) {
        // out on the water, or where it meets the land: the water's edge runs along the
        // shore's field, wavering; up a beach, the swash
        let sh = shore_at(p);
        let d = sh.d + shore_wobble(pg);
        if (!dry || d < 0.0) {
            out = water(water_kind(wt), wt, pg, min(d, 0.0), sh.land);
        } else {
            let v = vote(ts, f, pg, true);
            out = shaded(out, voted_shade(v, pg));
            if (is_beach(v.bt)) {
                out = swash(out.col, water_kind(wt), wt, pg, d, sh.land);
            }
        }
    } else if (ts[0] == ts[1] && ts[1] == ts[2] && ts[2] == ts[3]) {
        // one terrain all around (most of the ground): no border to draw, no votes
        out = shaded(out, terrain_shade(ts[0], pg, 10.0));
        drawn = ts[0];
    } else {
        let v = vote(ts, f, pg, false);
        out = shaded(out, voted_shade(v, pg));
        drawn = v.bt;
    }
    // a beach a little further from the water (what's drawn there: a beach's sand reaches
    // into the cells round it), the swash reaching it: the water its cell is near, on its
    // own level (the field's fourth: that water's terrain, plus one)
    if (wt < 0 && is_beach(drawn)) {
        let size = vec2<i32>(textureDimensions(shore)) - vec2<i32>(1, 1);
        let nt = i32(round(textureLoad(shore, clamp(own_cell + vec2<i32>(1, 1), vec2<i32>(0, 0), size), 0).a)) - 1;
        if (nt >= 0) {
            let sh = shore_at(p);
            out = swash(out.col, water_kind(nt), nt, pg, sh.d + shore_wobble(pg), sh.land);
        }
    }
    // shade at the foot of a cliff
    var shade = 1.0;
    if (cell_at(own_cell + vec2<i32>(0, -1)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, fc.y)); }
    if (cell_at(own_cell + vec2<i32>(0, 1)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, 1.0 - fc.y)); }
    if (cell_at(own_cell + vec2<i32>(-1, 0)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, fc.x)); }
    if (cell_at(own_cell + vec2<i32>(1, 0)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, 1.0 - fc.x)); }
    out.col = out.col * shade;
    return out;
}


@fragment
fn fs_map_shadow(i: MapOut) {
    if (textureSample(tex, samp, i.uv).a < 0.5) {
        discard;
    }
}

@vertex
fn vs_map_shadow(v: MapIn) -> MapOut {
    return shadowed(v);
}

// the indirect light's height pass (`gi.wgsl`): how high the point stands, seen from straight
// above (the depth keeps the highest), the cut-outs left out
@fragment
fn fs_height(i: MapOut) -> @location(0) vec4<f32> {
    if (textureSample(tex, samp, i.uv).a < 0.5) {
        discard;
    }
    return vec4<f32>(i.world.y, 0.0, 0.0, 1.0);
}

// a vertex of the map as the sun sees it
fn shadowed(v: MapIn) -> MapOut {
    var o: MapOut;
    let pos = swayed(v.pos + place.offset.xyz, v.wind);
    o.clip = g.light_view_proj * vec4<f32>(pos, 1.0);
    o.uv = v.uv;
    o.normal = v.normal;
    o.world = pos;
    o.light_pos = o.clip;
    o.color = v.color;
    o.toon = v.toon;
    return o;
}

// ---------------------------------------------------------------- grass copies

// a copy of a grass model (`grass.rs`), one to an instance: where its foot stands (the
// scene's space: drawn with the scene's own placement), its turn (radians about y, as
// `Mat4::from_rotation_y` turns), its size, and how dimmed its map is (0: not at all)
struct CopyIn {
    @location(6) at: vec3<f32>,
    @location(7) turn: f32,
    @location(8) scale: f32,
    @location(9) dim: f32,
};

// a vertex of the model as the copy has it: turned, sized and put down, its height above
// its foot as big as the copy (the wind bends a bigger one further), its colour dimmed with
// its map
fn copied(v: MapIn, c: CopyIn) -> MapIn {
    let s = sin(c.turn);
    let k = cos(c.turn);
    let p = v.pos * c.scale;
    var o = v;
    o.pos = c.at + vec3<f32>(k * p.x + s * p.z, p.y, k * p.z - s * p.x);
    o.normal = vec3<f32>(k * v.normal.x + s * v.normal.z, v.normal.y, k * v.normal.z - s * v.normal.x);
    o.wind = vec4<f32>(v.wind.xyz, min(v.wind.w * c.scale, 1.0));
    o.color = vec4<f32>(v.color.rgb * (1.0 - c.dim), v.color.a);
    return o;
}

@vertex
fn vs_grass(v: MapIn, c: CopyIn) -> MapOut {
    return mapped(copied(v, c));
}

@vertex
fn vs_grass_shadow(v: MapIn, c: CopyIn) -> MapOut {
    return shadowed(copied(v, c));
}

// ---------------------------------------------------------------- skinned models

struct SkinIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
    @location(6) toon: f32,
    @location(7) wind: vec4<f32>,
};

// each model's joint matrices (placed in the world already), at the draw's offset: in storage
// (WebGPU), or where a vertex shader reads none (WebGL 2) in a uniform buffer, as many as a
// uniform block holds there (16 KB: scene.rs's BONES_WINDOW), a joint past them read as the last
@group(3) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;
const BONES_WINDOW: u32 = 256u;
@group(3) @binding(1) var<uniform> bones_uniform: array<mat4x4<f32>, BONES_WINDOW>;

fn skin(v: SkinIn) -> mat4x4<f32> {
    return bones[v.joints.x] * v.weights.x + bones[v.joints.y] * v.weights.y + bones[v.joints.z] * v.weights.z + bones[v.joints.w] * v.weights.w;
}

fn skin_uniform(v: SkinIn) -> mat4x4<f32> {
    let j = min(v.joints, vec4<u32>(BONES_WINDOW - 1u));
    return bones_uniform[j.x] * v.weights.x + bones_uniform[j.y] * v.weights.y + bones_uniform[j.z] * v.weights.z + bones_uniform[j.w] * v.weights.w;
}

// a vertex of a skinned model, posed by `m` (its joints' matrices, weighed)
fn posed(v: SkinIn, m: mat4x4<f32>) -> MapOut {
    let n = normalize((m * vec4<f32>(v.normal, 0.0)).xyz);
    let p = swayed((m * vec4<f32>(v.pos, 1.0)).xyz, v.wind);
    var o: MapOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.uv = v.uv;
    o.normal = n;
    o.world = p;
    o.light_pos = g.light_view_proj * vec4<f32>(p + n * g.shadow.x, 1.0);
    o.color = v.color;
    o.toon = v.toon;
    return o;
}

// ... as the sun sees it
fn posed_shadow(v: SkinIn, m: mat4x4<f32>) -> MapOut {
    let p = swayed((m * vec4<f32>(v.pos, 1.0)).xyz, v.wind);
    var o: MapOut;
    o.clip = g.light_view_proj * vec4<f32>(p, 1.0);
    o.uv = v.uv;
    o.normal = v.normal;
    o.world = p;
    o.light_pos = o.clip;
    o.color = v.color;
    o.toon = v.toon;
    return o;
}

@vertex
fn vs_skinned(v: SkinIn) -> MapOut {
    return posed(v, skin(v));
}

@vertex
fn vs_skinned_shadow(v: SkinIn) -> MapOut {
    return posed_shadow(v, skin(v));
}

@vertex
fn vs_skinned_uniform(v: SkinIn) -> MapOut {
    return posed(v, skin_uniform(v));
}

@vertex
fn vs_skinned_uniform_shadow(v: SkinIn) -> MapOut {
    return posed_shadow(v, skin_uniform(v));
}

// ---------------------------------------------------------------- sprites and effects

struct SpriteIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) toon: f32,
};

struct SpriteOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) toon: f32,
};

@vertex
fn vs_sprite(v: SpriteIn) -> SpriteOut {
    var o: SpriteOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.world = v.pos;
    o.toon = v.toon;
    return o;
}

@vertex
fn vs_sprite_shadow(v: SpriteIn) -> SpriteOut {
    var o: SpriteOut;
    o.clip = g.light_view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.world = v.pos;
    o.toon = v.toon;
    return o;
}

// billboards: alpha-tested, tinted (a tint over 1 flashes them white), and lit by the
// lamps as if they faced them and up (they turn with the camera); a toon one gets the
// ink lines round it
@fragment
fn fs_sprite_cutout(i: SpriteOut) -> Solid {
    let c = textureSample(tex, samp, i.uv);
    if (c.a < 0.4) {
        discard;
    }
    let n = normalize(vec3<f32>(0.0, 1.0, 0.0) + normalize(g.camera.xyz - i.world));
    var lamps = indirect(i.world, n);
    for (var k = 0; k < i32(g.light_count.x); k = k + 1) {
        lamps = lamps + lamp(k, i.world, n);
    }
    var o: Solid;
    o.color = vec4<f32>(fog(focus_dim(c.rgb * (i.color.rgb + lamps), i.world), i.world), 1.0);
    o.look = look(i.world, normalize(g.camera.xyz - i.world), i.toon);
    o.emit = vec4<f32>(0.0);
    return o;
}

@fragment
fn fs_sprite_shadow(i: SpriteOut) {
    if (textureSample(tex, samp, i.uv).a < 0.4) {
        discard;
    }
}

// discs, rings, orbs: blended (alpha or additive by the pipeline)
@fragment
fn fs_sprite_blend(i: SpriteOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv) * i.color;
    return vec4<f32>(c.rgb, c.a);
}

// ---------------------------------------------------------------- the blit

struct BlitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> BlitOut {
    var o: BlitOut;
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    o.clip = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// the looks the solid things wrote (see `look`), what shines of them (`Solid::emit`), and the
// bloom: that blurred, half the picture's size, read smoothly
@group(1) @binding(4) var looks: texture_2d<u32>;
@group(1) @binding(5) var emit_tex: texture_2d<f32>;
@group(1) @binding(6) var bloom_tex: texture_2d<f32>;
@group(1) @binding(7) var bloom_samp: sampler;

// the picture's colours to the screen's range: the exposure, then a tone map. The soft one
// (0) keeps the colours as they are up to 0.8 and bends what's brighter under 1, so a glow
// and a lamp's hot spot keep their hue where a clamp would white them out; the filmic one (1,
// an ACES fit) has a film's contrast and shoulder over the whole range.
fn tone_mapped(c: vec3<f32>) -> vec3<f32> {
    let x = max(c * g.tone.x, vec3<f32>(0.0));
    if (g.tone.z > 0.5) {
        let m = x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14);
        return clamp(m, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let knee = 0.8;
    let over = max(x - knee, vec3<f32>(0.0));
    return min(x, vec3<f32>(knee)) + (1.0 - knee) * (1.0 - exp(-over / (1.0 - knee)));
}

struct Look {
    toon: bool,
    dist: f32,
    n: vec3<f32>,
};

fn look_at(p: vec2<i32>) -> Look {
    let size = vec2<i32>(textureDimensions(looks));
    let v = textureLoad(looks, clamp(p, vec2<i32>(0, 0), size - vec2<i32>(1, 1)), 0);
    var l: Look;
    l.toon = v.x == 1u;
    let k = 1.0 - f32(v.y) / 65535.0;
    l.dist = k * k * LOOK_FAR;
    l.n = unocta(vec2<f32>(f32(v.z), f32(v.w)) / 65535.0 * 2.0 - 1.0);
    return l;
}

// how much ink a pixel of the scene gets, and how far the ink's surface is: a band
// `toon_c.z` pixels wide round what's toon, outside it, where it stands in front of what's
// behind it (its outline; soft at its outer edge), and a pixel-wide line where its surface
// folds sharply (its creases)
fn ink(p: vec2<i32>) -> vec2<f32> {
    let width = max(g.toon_c.z, 0.5);
    let reach = i32(ceil(width + 0.5));
    let me = look_at(p);
    var nearest = 99.0;
    var near = me.dist;
    let dirs = array<vec2<i32>, 8>(
        vec2<i32>(1, 0), vec2<i32>(0, 1), vec2<i32>(-1, 0), vec2<i32>(0, -1),
        vec2<i32>(1, 1), vec2<i32>(-1, -1), vec2<i32>(1, -1), vec2<i32>(-1, 1),
    );
    for (var k = 0; k < 8; k = k + 1) {
        let step = select(1.0, 1.41421, k >= 4);
        for (var s = 1; s <= reach; s = s + 1) {
            let d = f32(s) * step;
            if (d >= nearest) {
                break;
            }
            let o = look_at(p + dirs[k] * s);
            // the nearer of the two owns the edge, if it's toon
            if (o.toon && o.dist < me.dist - g.toon_d.w * o.dist - 0.02) {
                nearest = d;
                near = min(near, o.dist);
                break;
            }
        }
    }
    var amount = clamp(width + 0.5 - nearest, 0.0, 1.0);
    // in the pixelation's big pixels an edge is inked whole or not at all: its softening (a
    // pixel wide) would be all there is of a line a pixel wide, grey instead of ink
    if (g.retro_a.x > 0.5) {
        amount = select(0.0, 1.0, nearest <= width + 0.01);
    }
    if (me.toon && g.toon_e.x <= 1.0) {
        for (var k = 0; k < 2; k = k + 1) {
            let o = look_at(p + dirs[k]);
            if (o.toon && abs(o.dist - me.dist) < g.toon_d.w * me.dist + 0.05 && dot(o.n, me.n) < g.toon_e.x) {
                amount = 1.0;
            }
        }
    }
    return vec2<f32>(amount, near);
}

// the light in steps (`toon_e.z` of them, even to the eye), over the whole picture: its
// brightness only, so the colours stay; each step's edge as soft as `toon_e.w` says
fn color_steps(c: vec3<f32>) -> vec3<f32> {
    let n = g.toon_e.z;
    let l = max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.00001);
    let k = pow(l, 1.0 / 2.2) * n;
    let f = fract(k);
    let s = max(g.toon_e.w, 0.002);
    let q = clamp((floor(k) + 0.5 + smoothstep(1.0 - s, 1.0, f)) / n, 0.0, 1.0);
    return min(c * (pow(q, 2.2) / l), vec3<f32>(1.0));
}

// how much a cloud shades a point of the world: the clouds drift with the wind, and a point
// is under one where the way to the sun meets their layer. Their shadow is cast from their
// height over the ground's lowest, whatever the point's own: real clouds are so high that a
// step of the ground doesn't move them (cast from the point's own height, a level's step
// made a seam of every cliff). They morph slowly as they go: each layer of their noise
// drifts its own way through them, and a slow swirl bends them all.
fn cloud_shade(p: vec3<f32>) -> f32 {
    let sun = normalize(g.sun_dir.xyz);
    let up = g.clouds_b.z / max(sun.y, 0.2);
    let base = (p.xz + sun.xz * up - g.clouds_b.xy) * g.clouds_a.y;
    let m = g.clouds_b.w;
    let swirl = vec2<f32>(noise(base * 0.45 + vec2<f32>(m * 0.5, 1.7)), noise(base * 0.45 + vec2<f32>(5.2, 3.1 - m * 0.45))) - 0.5;
    let q = base + swirl * 1.2;
    let layers = (noise(q + vec2<f32>(m * 0.6, -m * 0.2)) * 0.5
        + noise(q * 2.03 + vec2<f32>(17.1 - m * 0.9, 4.3 + m * 0.5)) * 0.25
        + noise(q * 4.1 + vec2<f32>(31.7 + m * 1.3, 8.9 + m * 1.1)) * 0.125) / 0.875;
    // spread out (clear clouds, clear sky between), a broader one making some places cloudier
    // than others
    let n = clamp((layers - 0.5) * 2.4 + 0.5 + (noise(q * 0.31 + vec2<f32>(5.3 + m * 0.15, 5.3)) - 0.5) * 0.35, 0.0, 1.0);
    let edge = 1.0 - g.clouds_a.z;
    return smoothstep(edge - g.clouds_a.w, edge + g.clouds_a.w, n);
}

// the clouds' shadows on what a pixel shows: where it is in the world (from its distance,
// the look's), darker where a cloud is over it, the more so where it's in the light (what's
// already in shade or dark keeps its colour)
fn clouds_over(c: vec3<f32>, uv: vec2<f32>, size: vec2<f32>) -> vec3<f32> {
    let me = look_at(vec2<i32>(uv * size));
    if (me.dist >= LOOK_FAR * 0.99) {
        return c; // the sky
    }
    let far = g.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 1.0, 1.0);
    let ray = normalize(far.xyz / far.w - g.camera.xyz);
    let shade = cloud_shade(g.camera.xyz + ray * me.dist);
    let lit = smoothstep(0.02, 0.18, dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)));
    return c * (1.0 - g.clouds_a.x * shade * lit);
}

fn from_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// where a pixel of the screen looks in the scene: at itself; in the old handhelds' pixels
// (the scene drawn that small), at the middle of the big pixel it's in
fn scene_uv(uv: vec2<f32>) -> vec2<f32> {
    if (g.retro_a.x < 0.5) {
        return uv;
    }
    let cell = floor(uv * g.retro_b.xy / g.retro_a.x);
    return (cell + 0.5) / vec2<f32>(textureDimensions(tex));
}

// the handhelds' colours: 15 bits of them, 32 steps a channel as the screen shows it
fn color_depth(c: vec3<f32>) -> vec3<f32> {
    let n = g.retro_a.w;
    return from_srgb(floor(to_srgb(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0))) * n + 0.5) / n);
}

// an LCD's colours, as the user's emulator Pipit has them (its video/color.rs): the GBA's
// screen by the "gba-color" transform (Pokefan531 and hunterk: dimmed, the channels mixed in
// linear light, each row summing to 1 so white stays white), the GBC's as Gambatte mixes
// them (on the screen's values)
fn lcd_colors(c: vec3<f32>) -> vec3<f32> {
    if (g.retro_a.y < 1.5) {
        let l = c * 0.94;
        let m = vec3<f32>(dot(vec3<f32>(0.82, 0.24, -0.06), l), dot(vec3<f32>(0.125, 0.665, 0.21), l), dot(vec3<f32>(0.195, 0.075, 0.73), l));
        return clamp(m, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let s = to_srgb(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)));
    return from_srgb(vec3<f32>(dot(vec3<f32>(13.0, 2.0, 1.0), s) / 16.0, dot(vec3<f32>(0.0, 3.0, 1.0), s) / 4.0, dot(vec3<f32>(3.0, 2.0, 11.0), s) / 16.0));
}

// the gaps between an LCD's pixels: each big pixel's edges a little darker (where they're
// big enough to show)
fn lcd_grid(uv: vec2<f32>) -> f32 {
    let k = g.retro_a.x;
    if (k < 3.5 || g.retro_a.z <= 0.0) {
        return 1.0;
    }
    let f = fract(uv * g.retro_b.xy / k);
    let d = min(f, vec2<f32>(1.0) - f) * k;
    return 1.0 - g.retro_a.z * (1.0 - smoothstep(0.0, 1.0, min(d.x, d.y)));
}

// the scene as the screen shows it at `screen_uv`: the bloom over it, exposed and tone
// mapped to the screen's range, the clouds' shadows over it, in colour steps if asked, with
// the toon look's ink lines over it (faded by the fog); in the old handhelds' pixels and
// colours if asked
fn picture(screen_uv: vec2<f32>) -> vec3<f32> {
    let uv = scene_uv(screen_uv);
    var c = textureSampleLevel(tex, samp, uv, 0.0).rgb;
    if (g.tone.y > 0.0) {
        c = c + textureSampleLevel(bloom_tex, bloom_samp, uv, 0.0).rgb * g.tone.y;
    }
    c = tone_mapped(c);
    if (g.clouds_a.x > 0.001) {
        c = clouds_over(c, uv, vec2<f32>(textureDimensions(looks)));
    }
    if (g.toon_e.z > 0.5) {
        c = color_steps(c);
    }
    if (g.toon_e.y > 0.5) {
        let size = vec2<f32>(textureDimensions(looks));
        let e = ink(vec2<i32>(uv * size));
        if (e.x > 0.0) {
            let f = smoothstep(g.fog_color.w, g.params.x, e.y);
            c = mix(c, g.toon_d.rgb, e.x * g.toon_c.w * (1.0 - f));
        }
    }
    if (g.retro_a.w > 0.5) {
        c = color_depth(c);
    }
    if (g.retro_a.y > 0.5) {
        c = lcd_colors(c);
    }
    return c;
}

// the scene to the screen, an LCD's gaps over it if asked
@fragment
fn fs_blit(i: BlitOut) -> @location(0) vec4<f32> {
    let c = picture(i.uv) * lcd_grid(i.uv);
    if (g.params.z > 0.5) {
        return vec4<f32>(to_srgb(c), 1.0);
    }
    return vec4<f32>(c, 1.0);
}

// ---------------------------------------------------------------- frosted glass

// a blurred copy of the picture, small, for the HUD's frosted buttons (egui draws it behind
// them): the picture as the screen has it, then blurred across, then down. Its values are
// the screen's (sRGB), which is what egui takes.
@fragment
fn fs_frost(i: BlitOut) -> @location(0) vec4<f32> {
    return vec4<f32>(to_srgb(clamp(picture(i.uv), vec3<f32>(0.0), vec3<f32>(1.0))), 1.0);
}

// nine texels along `step`, weighed as a bell (sigma two texels)
fn frost_blur(uv: vec2<f32>, step: vec2<f32>) -> vec4<f32> {
    let w = array<f32, 5>(0.2042, 0.1802, 0.1238, 0.0663, 0.0276);
    var c = textureSampleLevel(tex, samp, uv, 0.0).rgb * w[0];
    for (var k = 1; k < 5; k = k + 1) {
        let o = step * f32(k);
        c = c + (textureSampleLevel(tex, samp, uv + o, 0.0).rgb + textureSampleLevel(tex, samp, uv - o, 0.0).rgb) * w[k];
    }
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_frost_x(i: BlitOut) -> @location(0) vec4<f32> {
    return frost_blur(i.uv, vec2<f32>(1.0 / f32(textureDimensions(tex).x), 0.0));
}

@fragment
fn fs_frost_y(i: BlitOut) -> @location(0) vec4<f32> {
    return frost_blur(i.uv, vec2<f32>(0.0, 1.0 / f32(textureDimensions(tex).y)));
}

// ---------------------------------------------------------------- the bloom

// What shines bleeds into what's round it: what the solid things wrote as shining (the
// ground's glow, the blades', a lamp's), and whatever of the picture is brighter than
// `tone.w`, taken at half the picture's size, then blurred by a chain of halvings and
// doublings (Kawase's dual filter: each step a few smooth taps), the doublings added up on
// the way back; the blit adds the result (`picture`).

// the chain's first: what shines, and what's too bright, at half size (the colour and what
// shines bound with a smooth sampler)
@fragment
fn fs_bloom_first(i: BlitOut) -> @location(0) vec4<f32> {
    let e = textureSampleLevel(emit_tex, samp, i.uv, 0.0).rgb;
    let c = textureSampleLevel(tex, samp, i.uv, 0.0).rgb;
    let over = max(c - vec3<f32>(g.tone.w), vec3<f32>(0.0));
    return vec4<f32>(e + over, 1.0);
}

// halved: the middle and the four corners of the texel it's over
@fragment
fn fs_bloom_down(i: BlitOut) -> @location(0) vec4<f32> {
    let o = 0.5 / vec2<f32>(textureDimensions(tex));
    var c = textureSampleLevel(tex, samp, i.uv, 0.0).rgb * 4.0;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(-o.x, -o.y), 0.0).rgb;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(o.x, -o.y), 0.0).rgb;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(-o.x, o.y), 0.0).rgb;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(o.x, o.y), 0.0).rgb;
    return vec4<f32>(c / 8.0, 1.0);
}

// doubled: eight taps round it, the diagonals twice (added onto the finer level)
@fragment
fn fs_bloom_up(i: BlitOut) -> @location(0) vec4<f32> {
    let o = 1.0 / vec2<f32>(textureDimensions(tex));
    var c = textureSampleLevel(tex, samp, i.uv + vec2<f32>(-2.0 * o.x, 0.0), 0.0).rgb;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(2.0 * o.x, 0.0), 0.0).rgb;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(0.0, -2.0 * o.y), 0.0).rgb;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(0.0, 2.0 * o.y), 0.0).rgb;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(-o.x, -o.y), 0.0).rgb * 2.0;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(o.x, -o.y), 0.0).rgb * 2.0;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(-o.x, o.y), 0.0).rgb * 2.0;
    c = c + textureSampleLevel(tex, samp, i.uv + vec2<f32>(o.x, o.y), 0.0).rgb * 2.0;
    return vec4<f32>(c / 12.0, 1.0);
}
