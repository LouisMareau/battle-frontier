// Meadows' blades and flowers culled and their rows picked on the GPU each frame (`blades.rs`;
// `blade_lod.wgsl` goes before this): one out of the eye's sight is left out (one just outside
// it, within the margin the planes are pushed out by, still drawn: it casts its shadow in), and
// so is one that isn't drawn where it stands (too far off, or one of the share left out
// there); the rest each go to the list of their shape and their rows, by its index. Then each
// draw's count of blades is set from its list's (`spread_blades`): the draws are indirect. As
// `blades.rs`'s `pick` does it on the CPU.

struct Cull {
    view_proj: mat4x4<f32>,
    // the eye's sight pushed out by the margin: each plane's normal (a unit, pointing in) and
    // distance
    planes: array<vec4<f32>, 6>,
    // x: the blades, y: the room for each list
    count: vec4<u32>,
};

@group(0) @binding(0) var<uniform> cull: Cull;
@group(0) @binding(1) var<uniform> meadow: Kinds;
@group(0) @binding(2) var<storage, read> blades: array<Blade>;
// each list's blades (their indices), room for `count.y`, one after another
@group(0) @binding(3) var<storage, read_write> lists: array<u32>;
@group(0) @binding(4) var<storage, read_write> counters: array<atomic<u32>>;

@compute @workgroup_size(64)
fn cull_blades(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= cull.count.x) {
        return;
    }
    let b = blades[i];
    let k = kind_of(b);
    let tall = placed_tall(k, b);
    let mid = blade_middle(b, tall);
    // how far round its middle it reaches: leaning over, its head, the wind
    let reach = tall * 0.75 + k.bend.w + 0.3;
    for (var p = 0u; p < 6u; p++) {
        let plane = cull.planes[p];
        if (dot(plane.xyz, mid) + plane.w < -reach) {
            return;
        }
    }
    let px = cell_px((cull.view_proj * vec4<f32>(mid, 1.0)).w);
    if (!kept(px, b.seed)) {
        return;
    }
    let tier = tier_of(tall * tallness(px, b.seed) * px);
    let list = min(u32(k.form.x), 1u) * 3u + tier;
    let slot = atomicAdd(&counters[list], 1u);
    if (slot < cull.count.y) {
        lists[list * cull.count.y + slot] = i;
    }
}

// ---------------------------------------------------------------- the draws' counts

// the lists' counts (the counters, read once the culling's done; no more than a list's room),
// each draw's list, and the draws (five numbers each, as `DrawIndexedIndirectArgs`: the second
// its blades)
@group(0) @binding(5) var<storage, read> totals: array<u32>;
@group(0) @binding(6) var<storage, read> draw_lists: array<u32>;
@group(0) @binding(7) var<storage, read_write> args: array<u32>;

@compute @workgroup_size(64)
fn spread_blades(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = id.x;
    if (d >= arrayLength(&draw_lists)) {
        return;
    }
    args[d * 5u + 1u] = min(totals[draw_lists[d]], cull.count.y);
}
