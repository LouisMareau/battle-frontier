// The scene's shaders: the map (lit, with the sun's shadow), sprites and effects
// (unlit, tinted), the shadow pass, and the blit to the screen with the toon look's ink
// lines. What's solid also writes its look (toon or not, how far, which way it faces) to a
// second target, which the lines are found in.
// Everything here works on WebGL 2 as well as WebGPU: uniform buffers only.

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
    lights: array<vec4<f32>, 96>, // per lamp: xyz + range, direction + cos outer, colour + cos inner
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;

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
    // 1: drawn with the toon look
    @location(4) toon: f32,
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

// what something solid writes: its colour, and its look for the ink lines
struct Solid {
    @location(0) color: vec4<f32>,
    @location(1) look: vec4<u32>,
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
    return vec4<u32>(select(0u, 1u, toon > 0.5), depth, e.x, e.y);
}

@vertex
fn vs_map(v: MapIn) -> MapOut {
    var o: MapOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.normal = v.normal;
    o.world = v.pos;
    // looked up a little off the surface, along its normal: no acne, and no margin wide
    // enough to part a shadow from its caster
    o.light_pos = g.light_view_proj * vec4<f32>(v.pos + v.normal * g.shadow.x, 1.0);
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
    let n = normalize(normal);
    let ndl = max(dot(n, g.sun_dir.xyz), 0.0);
    let hemi = mix(g.hemi_ground.rgb, g.hemi_sky.rgb, n.y * 0.5 + 0.5);
    var lit = g.ambient.rgb + hemi + g.sun_color.rgb * ndl * lit_by_sun(light_pos);
    for (var k = 0; k < i32(g.light_count.x); k = k + 1) {
        lit = lit + lamp(k, world, n);
    }
    return lit;
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
    return lit + g.sun_color.rgb * (rim + glint);
}

fn light(normal: vec3<f32>, light_pos: vec4<f32>, world: vec3<f32>, toon: f32) -> vec3<f32> {
    if (toon > 0.5) {
        return toon_light(normal, light_pos, world);
    }
    return sunlit(normal, light_pos, world);
}

@fragment
fn fs_map(i: MapOut) -> Solid {
    let c = textureSample(tex, samp, i.uv) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    var o: Solid;
    o.color = vec4<f32>(fog(focus_dim(c.rgb * light(i.normal, i.light_pos, i.world, i.toon), i.world), i.world), 1.0);
    o.look = look(i.world, i.normal, i.toon);
    return o;
}

// the same without cut-outs (vertex colours on white): with no discard, the GPU can skip
// what's hidden before shading it
@fragment
fn fs_map_opaque(i: MapOut) -> Solid {
    let c = textureSample(tex, samp, i.uv) * i.color;
    var o: Solid;
    o.color = vec4<f32>(fog(focus_dim(c.rgb * light(i.normal, i.light_pos, i.world, i.toon), i.world), i.world), 1.0);
    o.look = look(i.world, i.normal, i.toon);
    return o;
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
    let c = t.rgb * i.color.rgb;
    return vec4<f32>(fog(focus_dim(c * light(i.normal, i.light_pos, i.world, i.toon), i.world), i.world), i.color.a);
}

// ---------------------------------------------------------------- the world's ground
// Cell tops and cliff faces: each vertex carries its cell's centre in uv (a face, the cell
// above it's), and in its colour's alpha how much rock it is: 0 on the tops, 1 on the
// faces, between on the bevels along the cliffs' tops (the faces' rgb darkens their foot).
// The terrain of every cell is in `cells` (r: terrain, g: level); `palette` has a column
// per terrain: row 0 its colour, row 1 its second colour, row 2 (pattern, priority + 128,
// water), row 3 its cliffs' colour. Where terrains meet, the border is drawn here: each of
// the four nearest cells votes for its terrain by closeness, with noise, so borders come
// out organic and corners round, whatever was painted, and the painter never picks border
// tiles.

@group(1) @binding(2) var cells: texture_2d<f32>;
@group(1) @binding(3) var palette: texture_2d<f32>;

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

fn cell_at(c: vec2<i32>) -> vec2<i32> {
    let size = vec2<i32>(textureDimensions(cells));
    let t = textureLoad(cells, clamp(c, vec2<i32>(0, 0), size - vec2<i32>(1, 1)), 0);
    return vec2<i32>(i32(round(t.r * 255.0)), i32(round(t.g * 255.0)));
}

// is the cell in the editor's focus (the cells texture's blue channel)?
fn focused(c: vec2<i32>) -> bool {
    let size = vec2<i32>(textureDimensions(cells));
    return textureLoad(cells, clamp(c, vec2<i32>(0, 0), size - vec2<i32>(1, 1)), 0).b > 0.5;
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
    let time = g.world.w;
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
        case 8: { // water: moving ripples, lighter at the shore
            let w = noise(p * 1.2 + vec2<f32>(time * 0.25, time * 0.1)) + noise(p * 2.1 - vec2<f32>(time * 0.15, time * 0.3));
            c = mix(c1, c1 * 0.78, fbm(p * 0.3));
            c = mix(c, c2, smoothstep(1.2, 1.5, w) * 0.8);
            c = mix(c2, c, smoothstep(0.0, 0.22, inside));
        }
        default: {}
    }
    return c;
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
    let p = i.world.xz;
    let own_cell = vec2<i32>(floor(i.uv));
    let own = cell_at(own_cell);
    let rocky = i.color.a;
    var n = normalize(i.normal);
    // the surface's own facet (a cliff face is lumpy; its vertices keep the flat normal)
    let facet = cross(dpdx(i.world), dpdy(i.world));
    var col = vec3<f32>(0.0);
    let fc = p - vec2<f32>(own_cell);
    if (rocky < 0.999) {
        col = ground_color(own_cell, own, p, fc);
    }
    if (rocky > 0.001) {
        // along the face (across its normal, on the ground) and up it
        let level_n = vec2<f32>(n.x, n.z);
        let h = select(vec2<f32>(1.0, 0.0), normalize(level_n), length(level_n) > 0.01);
        let along = vec2<f32>(-h.y, h.x);
        let q = vec2<f32>(dot(p, along), i.world.y);
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
    }
    if (g.editor.x > 0.5 && rocky < 0.999) {
        let line = min(min(fc.x, 1.0 - fc.x), min(fc.y, 1.0 - fc.y));
        col = col * mix(0.72, 1.0, smoothstep(0.012, 0.035, line));
    }
    var o: Solid;
    o.color = vec4<f32>(fog(col * sunlit(n, i.light_pos, i.world), i.world), 1.0);
    o.look = look(i.world, n, 0.0);
    return o;
}

// a cell top's colour at p: its terrain, the borders with the terrains around, and the
// shade at the foot of a cliff
fn ground_color(own_cell: vec2<i32>, own: vec2<i32>, p: vec2<f32>, fc: vec2<f32>) -> vec3<f32> {
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
    var col: vec3<f32>;
    if (ts[0] == ts[1] && ts[1] == ts[2] && ts[2] == ts[3]) {
        // one terrain all around (most of the ground): no border to draw, no votes
        col = terrain_color(ts[0], p, 10.0);
    } else {
        let ws = array<f32, 4>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
        var best = -9.0;
        var bt = own.x;
        var second = -9.0;
        var st = own.x;
        let s0 = score(ts, ws, ts[0], p);
        let s1 = score(ts, ws, ts[1], p);
        let s2 = score(ts, ws, ts[2], p);
        let s3 = score(ts, ws, ts[3], p);
        let ss = array<f32, 4>(s0, s1, s2, s3);
        for (var k = 0; k < 4; k = k + 1) {
            var t = ts[0];
            var s = ss[0];
            if (k == 1) { t = ts[1]; s = ss[1]; }
            if (k == 2) { t = ts[2]; s = ss[2]; }
            if (k == 3) { t = ts[3]; s = ss[3]; }
            if (s > best) {
                if (t != bt) {
                    second = best;
                    st = bt;
                }
                best = s;
                bt = t;
            } else if (t != bt && s > second) {
                second = s;
                st = t;
            }
        }
        col = terrain_color(bt, p, best - second);
        if (second > -8.0) {
            let other = terrain_color(st, p, 0.0);
            col = mix(other, col, 0.5 + 0.5 * smoothstep(0.0, 0.04, best - second));
        }
    }
    // shade at the foot of a cliff
    var shade = 1.0;
    if (cell_at(own_cell + vec2<i32>(0, -1)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, fc.y)); }
    if (cell_at(own_cell + vec2<i32>(0, 1)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, 1.0 - fc.y)); }
    if (cell_at(own_cell + vec2<i32>(-1, 0)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, fc.x)); }
    if (cell_at(own_cell + vec2<i32>(1, 0)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, 1.0 - fc.x)); }
    return col * shade;
}


@fragment
fn fs_map_shadow(i: MapOut) {
    if (textureSample(tex, samp, i.uv).a < 0.5) {
        discard;
    }
}

@vertex
fn vs_map_shadow(v: MapIn) -> MapOut {
    var o: MapOut;
    o.clip = g.light_view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.normal = v.normal;
    o.world = v.pos;
    o.light_pos = o.clip;
    o.color = v.color;
    o.toon = v.toon;
    return o;
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
};

// each model's joint matrices (placed in the world already), at the draw's offset
@group(3) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;

fn skin(v: SkinIn) -> mat4x4<f32> {
    return bones[v.joints.x] * v.weights.x + bones[v.joints.y] * v.weights.y + bones[v.joints.z] * v.weights.z + bones[v.joints.w] * v.weights.w;
}

@vertex
fn vs_skinned(v: SkinIn) -> MapOut {
    let m = skin(v);
    let p = (m * vec4<f32>(v.pos, 1.0)).xyz;
    let n = normalize((m * vec4<f32>(v.normal, 0.0)).xyz);
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

@vertex
fn vs_skinned_shadow(v: SkinIn) -> MapOut {
    let m = skin(v);
    let p = (m * vec4<f32>(v.pos, 1.0)).xyz;
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
    var lamps = vec3<f32>(0.0);
    for (var k = 0; k < i32(g.light_count.x); k = k + 1) {
        lamps = lamps + lamp(k, i.world, n);
    }
    var o: Solid;
    o.color = vec4<f32>(fog(focus_dim(c.rgb * (i.color.rgb + lamps), i.world), i.world), 1.0);
    o.look = look(i.world, normalize(g.camera.xyz - i.world), i.toon);
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

// the looks the solid things wrote (see `look`)
@group(1) @binding(4) var looks: texture_2d<u32>;

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

// the scene to the screen: in colour steps if asked, with the toon look's ink lines over
// it (faded by the fog)
@fragment
fn fs_blit(i: BlitOut) -> @location(0) vec4<f32> {
    var c = textureSample(tex, samp, i.uv).rgb;
    if (g.toon_e.z > 0.5) {
        c = color_steps(c);
    }
    if (g.toon_e.y > 0.5) {
        let size = vec2<f32>(textureDimensions(looks));
        let e = ink(vec2<i32>(i.uv * size));
        if (e.x > 0.0) {
            let f = smoothstep(g.fog_color.w, g.params.x, e.y);
            c = mix(c, g.toon_d.rgb, e.x * g.toon_c.w * (1.0 - f));
        }
    }
    if (g.params.z > 0.5) {
        return vec4<f32>(to_srgb(c), 1.0);
    }
    return vec4<f32>(c, 1.0);
}
