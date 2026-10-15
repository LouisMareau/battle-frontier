// The scene's shaders: the map (lit, with the sun's shadow), sprites and effects
// (unlit, tinted), the shadow pass, and the blit to the screen.
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

// darker outside the editor's focus box
fn focus_dim(rgb: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (g.editor.y > 0.5 && (world.x < g.focus.x || world.z < g.focus.y || world.x > g.focus.z || world.z > g.focus.w)) {
        return rgb * 0.16;
    }
    return rgb;
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
};

struct MapOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
    @location(3) light_pos: vec4<f32>,
    @location(4) color: vec4<f32>,
};

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

@fragment
fn fs_map(i: MapOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv) * i.color;
    if (c.a < 0.5) {
        discard;
    }
    return vec4<f32>(fog(focus_dim(c.rgb * sunlit(i.normal, i.light_pos, i.world), i.world), i.world), 1.0);
}

// the same without cut-outs (vertex colours on white): with no discard, the GPU can skip
// what's hidden before shading it
@fragment
fn fs_map_opaque(i: MapOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv) * i.color;
    return vec4<f32>(fog(focus_dim(c.rgb * sunlit(i.normal, i.light_pos, i.world), i.world), i.world), 1.0);
}

// ---------------------------------------------------------------- the world's ground
// Cell tops: each vertex carries its cell's centre in uv. The terrain of every cell is in
// `cells` (r: terrain, g: level); `palette` has a column per terrain: row 0 its colour,
// row 1 its second colour, row 2 (pattern, priority + 128, water). Where terrains meet,
// the border is drawn here: each of the four nearest cells votes for its terrain by
// closeness, with noise, so borders come out organic and corners round, whatever was
// painted, and the painter never picks border tiles.

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

@fragment
fn fs_ground(i: MapOut) -> @location(0) vec4<f32> {
    let p = i.world.xz;
    let own_cell = vec2<i32>(floor(i.uv));
    let own = cell_at(own_cell);
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
    let fc = p - vec2<f32>(own_cell);
    var shade = 1.0;
    if (cell_at(own_cell + vec2<i32>(0, -1)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, fc.y)); }
    if (cell_at(own_cell + vec2<i32>(0, 1)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, 1.0 - fc.y)); }
    if (cell_at(own_cell + vec2<i32>(-1, 0)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, fc.x)); }
    if (cell_at(own_cell + vec2<i32>(1, 0)).y > own.y) { shade = shade * mix(0.62, 1.0, smoothstep(0.0, 0.7, 1.0 - fc.x)); }
    col = col * shade;
    if (g.editor.y > 0.5 && !focused(own_cell)) {
        col = col * 0.16;
    }
    if (g.editor.x > 0.5) {
        let line = min(min(fc.x, 1.0 - fc.x), min(fc.y, 1.0 - fc.y));
        col = col * mix(0.72, 1.0, smoothstep(0.012, 0.035, line));
    }
    return vec4<f32>(fog(col * sunlit(i.normal, i.light_pos, i.world), i.world), 1.0);
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
    return o;
}

// ---------------------------------------------------------------- sprites and effects

struct SpriteIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct SpriteOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_sprite(v: SpriteIn) -> SpriteOut {
    var o: SpriteOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.world = v.pos;
    return o;
}

@vertex
fn vs_sprite_shadow(v: SpriteIn) -> SpriteOut {
    var o: SpriteOut;
    o.clip = g.light_view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.world = v.pos;
    return o;
}

// billboards: alpha-tested, tinted (a tint over 1 flashes them white), and lit by the
// lamps as if they faced them and up (they turn with the camera)
@fragment
fn fs_sprite_cutout(i: SpriteOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    if (c.a < 0.4) {
        discard;
    }
    let n = normalize(vec3<f32>(0.0, 1.0, 0.0) + normalize(g.camera.xyz - i.world));
    var lamps = vec3<f32>(0.0);
    for (var k = 0; k < i32(g.light_count.x); k = k + 1) {
        lamps = lamps + lamp(k, i.world, n);
    }
    return vec4<f32>(fog(focus_dim(c.rgb * (i.color.rgb + lamps), i.world), i.world), 1.0);
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

@fragment
fn fs_blit(i: BlitOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv).rgb;
    if (g.params.z > 0.5) {
        return vec4<f32>(to_srgb(c), 1.0);
    }
    return vec4<f32>(c, 1.0);
}
