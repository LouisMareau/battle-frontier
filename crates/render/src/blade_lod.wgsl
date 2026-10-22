// Meadows' blades and flowers: what they are, and how fine each is drawn by how big it looks
// from the eye (`blades.rs`), the same for the culling (`blades.wgsl`) and the drawing
// (`blades_draw.wgsl`). Each module that has this declares `meadow`, its kinds, itself.
//
// By how many pixels a cell spans where a blade stands (`px`): from `lod.y` up every blade is
// drawn; under it fewer (the share drawn going with the pixels a cell covers, so the picture
// holds as many blades whatever the distance), each kept one wider across (`lod_b.x` times at
// most) and growing in as it comes (`lod_b.w` of the share past it); under `lod.z` each
// shorter, none under `lod.w`. A blade drawn `lod_b.y` pixels tall or more is drawn in seven
// rows, `lod_b.z` or more in three, else as one triangle.

// a kind of plant (`world::meadow::Kinds`): what it is and how it looks
struct Kind {
    // x: its shape (0 a blade, 1 a flower), y: the shortest, z: the tallest (cells), w: its
    // width at its foot
    form: vec4<f32>,
    // x: how far over it leans (0 to 1), y: how much it curves, z: how much the wind moves it,
    // w: a flower's head's width (cells)
    bend: vec4<f32>,
    // rgb: at its foot (linear); w: a flower's petals
    foot: vec4<f32>,
    // rgb: at its tip; w: toon (1) or not (0)
    tip: vec4<f32>,
    // rgb: a flower's petals', its heart's; the heart's w: how much it glows of itself (0 to
    // 4: a flower its petals and heart, a blade towards its tip)
    petal: vec4<f32>,
    heart: vec4<f32>,
};

struct Kinds {
    // x: the pixels a cell spans at one unit of the eye's depth (w), y: the cell's pixels from
    // which every blade is drawn, z: under which they're shorter, w: under which none
    lod: vec4<f32>,
    // x: the most a blade is widened, y and z: the pixels tall of its finest rows and of its
    // middle ones, w: the share past what's drawn over which a blade grows in
    lod_b: vec4<f32>,
    kinds: array<Kind, 64>,
};

// a blade or a flower placed (`blades.rs`'s `BladeInstance`): its foot (the scene's space),
// the way it leans (radians about y), its size within its kind's (0 to 1), its map's dimming,
// its kind, its own random bits
struct Blade {
    at: vec3<f32>,
    turn: f32,
    size: f32,
    dim: f32,
    kind: u32,
    seed: u32,
};

// a blade's own random bits further mixed (as `blades.rs`'s `mix_bits`)
fn mix_bits(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

// 0 to 1 from the low bits
fn unit(v: u32) -> f32 {
    return f32(v & 0xffffu) / 65535.0;
}

fn kind_of(b: Blade) -> Kind {
    return meadow.kinds[min(b.kind, 63u)];
}

// how tall it is as placed (cells)
fn placed_tall(k: Kind, b: Blade) -> f32 {
    return mix(k.form.y, k.form.z, b.size);
}

// the middle of what it covers (its foot and up half its height)
fn blade_middle(b: Blade, tall: f32) -> vec3<f32> {
    return b.at + vec3<f32>(0.0, tall * 0.5, 0.0);
}

// how many pixels a cell spans at a depth `w` from the eye
fn cell_px(w: f32) -> f32 {
    return meadow.lod.x / max(w, 0.0001);
}

// the share of the blades drawn where a cell spans `px` pixels
fn keep_at(px: f32) -> f32 {
    let k = clamp(px / meadow.lod.y, 0.0, 1.0);
    return k * k;
}

// whether a blade (its own random bits `seed`) is drawn where a cell spans `px` pixels
fn kept(px: f32, seed: u32) -> bool {
    return px >= meadow.lod.w && unit(seed) < keep_at(px) * (1.0 + meadow.lod_b.w);
}

// how tall a blade is drawn there against as placed: growing in as it comes, shorter at the
// furthest
fn tallness(px: f32, seed: u32) -> f32 {
    let keep = keep_at(px);
    let band = keep * meadow.lod_b.w;
    let grown = clamp((keep + band - unit(seed)) / max(band, 0.00001), 0.0, 1.0);
    return grown * smoothstep(meadow.lod.w, meadow.lod.z, px);
}

// how much wider a blade is drawn there: as many times as fewer of them are (square-rooted)
fn widened(px: f32) -> f32 {
    return min(inverseSqrt(max(keep_at(px), 0.000001)), meadow.lod_b.x);
}

// its rows by how many pixels tall it's drawn: 0 its finest (seven), 1 three, 2 one triangle
fn tier_of(tall_px: f32) -> u32 {
    if (tall_px >= meadow.lod_b.y) {
        return 0u;
    }
    if (tall_px >= meadow.lod_b.z) {
        return 1u;
    }
    return 2u;
}
