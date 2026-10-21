// Grass copies culled and their levels picked on the GPU, each frame (`grass.rs`): a copy
// out of the eye's sight is left out, and so is one whose cells are too small to see; one
// just outside it (within the margin the planes are pushed out by: it casts its shadow in)
// goes to its thinnest level; the rest each to the finest level of its model that costs no
// more than it looks big from the eye (about `screen.w` vertices to a pixel of the ground it
// covers, and of a quarter of it standing up: the rest of that covers the ground of the copies
// behind it; a little more or less copy to copy, so where one level gives
// way to the next differs). Then each draw's count of copies is set from its list's
// (`spread`): the draws are indirect. As `grass.rs`'s `pick` does it on the CPU.

struct Cull {
    view_proj: mat4x4<f32>,
    // the eye's sight pushed out by the margin: each plane's normal (a unit, pointing in) and
    // distance
    planes: array<vec4<f32>, 6>,
    // x: the margin (cells)
    margin: vec4<f32>,
    // xy: the picture's size in pixels; z: the fewest pixels a cell's ground covers to draw
    // it; w: how many vertices a pixel of it may cost
    screen: vec4<f32>,
};

struct Model {
    // x: its copies, y: its levels, z: its first list (counter), w: the room for each list
    count: vec4<u32>,
    // xyz: the middle of its box (its own space), w: how far round that it reaches
    middle: vec4<f32>,
    // x: the cells it covers, y: how tall it stands (cells)
    cells: vec4<f32>,
    // each level's vertices, four to a row
    cost: array<vec4<f32>, 3>,
};

// as `GrassCopy`: its foot, its turn, its size, its map's dimming
struct Copy {
    at: vec3<f32>,
    turn: f32,
    scale: f32,
    dim: f32,
    pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> cull: Cull;
@group(0) @binding(1) var<uniform> model: Model;
@group(0) @binding(2) var<storage, read> copies: array<Copy>;
// each level's list, room for `count.w` copies, one after another
@group(0) @binding(3) var<storage, read_write> out: array<Copy>;
@group(0) @binding(4) var<storage, read_write> counters: array<atomic<u32>>;

fn level_cost(k: u32) -> f32 {
    return model.cost[k / 4u][k % 4u];
}

// 0 to 1, a copy's own (as `grass.rs`'s `jitter`)
fn jitter(i: u32) -> f32 {
    var h = i * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h & 0xffffu) / 65535.0;
}

@compute @workgroup_size(64)
fn cull_copies(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= model.count.x) {
        return;
    }
    let c = copies[i];
    let s = sin(c.turn);
    let k = cos(c.turn);
    let m = model.middle.xyz * c.scale;
    let mid = c.at + vec3<f32>(k * m.x + s * m.z, m.y, k * m.z - s * m.x);
    let r = model.middle.w * c.scale;
    var seen = true;
    for (var p = 0u; p < 6u; p++) {
        let plane = cull.planes[p];
        let d = dot(plane.xyz, mid) + plane.w;
        if (d < -r) {
            return;
        }
        seen = seen && d >= cull.margin.x - r;
    }
    let levels = model.count.y;
    var level = levels - 1u;
    if (seen) {
        // how big a cell of it looks: the pixels its ground covers, and it standing up (none
        // behind the eye: the finest)
        let a = cull.view_proj * vec4<f32>(mid, 1.0);
        let x = cull.view_proj * vec4<f32>(mid + vec3<f32>(1.0, 0.0, 0.0), 1.0);
        let y = cull.view_proj * vec4<f32>(mid + vec3<f32>(0.0, model.cells.y * c.scale, 0.0), 1.0);
        let z = cull.view_proj * vec4<f32>(mid + vec3<f32>(0.0, 0.0, 1.0), 1.0);
        var area = 1e12;
        if (min(min(a.w, x.w), min(y.w, z.w)) > 0.0001) {
            let half = cull.screen.xy * 0.5;
            let o = a.xy / a.w * half;
            let dx = x.xy / x.w * half - o;
            let dy = y.xy / y.w * half - o;
            let dz = z.xy / z.w * half - o;
            area = abs(dx.x * dz.y - dx.y * dz.x) + 0.25 * max(abs(dx.x * dy.y - dx.y * dy.x), abs(dz.x * dy.y - dz.y * dy.x));
        }
        if (area < cull.screen.z) {
            return;
        }
        let want = cull.screen.w * area * (0.75 + 0.5 * jitter(i));
        for (var l = 0u; l < levels; l++) {
            if (level_cost(l) / model.cells.x <= want) {
                level = l;
                break;
            }
        }
    }
    let slot = atomicAdd(&counters[model.count.z + level], 1u);
    out[level * model.count.w + slot] = c;
}

// ---------------------------------------------------------------- the draws' counts

// the lists' counts (the counters, read once the culling's done), each draw's list, and the
// draws (five numbers each, as `DrawIndexedIndirectArgs`: the second its copies)
@group(0) @binding(5) var<storage, read> totals: array<u32>;
@group(0) @binding(6) var<storage, read> lists: array<u32>;
@group(0) @binding(7) var<storage, read_write> args: array<u32>;

@compute @workgroup_size(64)
fn spread(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = id.x;
    if (d >= arrayLength(&lists)) {
        return;
    }
    args[d * 5u + 1u] = totals[lists[d]];
}
