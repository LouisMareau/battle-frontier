//! The indirect light: what glows (the terrains' own glow, the lamps' light bounced off the
//! ground, the blades' glow) lighting what's round it, with soft shadows behind walls and
//! cliffs. Radiance cascades over a square region of the world round the camera's target,
//! run in compute shaders each frame (`gi.wgsl` says how); the scene draws the height pass
//! into `height_view` first and its lighting reads the probes (`views`). On WebGPU alone:
//! with no compute shaders (WebGL 2) the scene binds blank probes (`blank_views`).

use glam::{Mat4, Vec2, Vec3, Vec4};

/// The field's texels a side (what shines, the ground's and the blocking heights).
pub(crate) const FIELD: u32 = 256;
/// The finest cascade's probes a side; each coarser one has half as many, four times the
/// directions, over an interval four times as long.
pub(crate) const PROBES: u32 = 128;
pub(crate) const CASCADES: usize = 4;
/// The height pass's eye: this high over the ground's zero, seeing this far down.
const EYE: f32 = 64.0;
const FAR: f32 = 128.0;
/// Each cascade's ray step, in field texels: under the three a wall is made at least (so no
/// step crosses one), the coarser ones longer.
const STEP_TEXELS: [f32; CASCADES] = [1.0, 2.0, 2.0, 2.8];
/// Bytes between two dispatches' parameters: a dynamic offset's alignment.
const PARAMS_STRIDE: u64 = 256;
/// The parameters' slots: the field's passes, each cascade's, the resolve's.
const PARAMS: u64 = 2 + CASCADES as u64;
/// How much of the lamps' light the ground bounces, and how much of the screen's glow (the
/// blades') counts.
pub(crate) const BOUNCE: f32 = 0.5;
pub(crate) const SCREEN: f32 = 1.0;
/// The field's format, and the probes'.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The compute shaders, with the scene's (they draw the ground's glow as the ground shader
/// does, and read the lamps): a module of their own, as a storage texture anywhere in a
/// module keeps WebGL 2 from reading any of it.
const GI_SHADER: &str = concat!(include_str!("shader.wgsl"), include_str!("gi.wgsl"));

/// A dispatch's parameters, as `gi.wgsl`'s `GiParams` reads them.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    region: [f32; 4],
    probes: [f32; 4],
    reach: [f32; 4],
    heights: [f32; 4],
}

/// Where the region is this frame: its north-west corner (x, z) and its width (cells).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Region {
    pub origin: Vec2,
    pub width: f32,
}

impl Region {
    /// Round the camera's target (where its middle ray meets the ground's zero, within 300
    /// cells of the camera's foot; the foot itself when it looks up), `width` cells a side,
    /// snapped to the finest probes' grid so the probes stand still in the world as the
    /// camera moves.
    pub(crate) fn around(view_proj: &Mat4, camera: Vec3, width: f32) -> Region {
        let far = view_proj.inverse() * Vec4::new(0.0, 0.0, 1.0, 1.0);
        let foot = Vec2::new(camera.x, camera.z);
        let mut centre = foot;
        if far.w.abs() > 1e-9 {
            let dir = far.truncate() / far.w - camera;
            if dir.y < -1e-4 {
                let hit = camera + dir * (-camera.y / dir.y).min(1.0);
                centre = Vec2::new(hit.x, hit.z);
            }
        }
        let off = centre - foot;
        if off.length() > 300.0 {
            centre = foot + off.normalize() * 300.0;
        }
        let width = width.max(1.0);
        let spacing = width / PROBES as f32;
        let origin = ((centre - Vec2::splat(width / 2.0)) / spacing).floor() * spacing;
        Region { origin, width }
    }

    /// The finest probes' spacing, and a field texel's size (cells).
    pub(crate) fn spacing(&self) -> f32 {
        self.width / PROBES as f32
    }

    pub(crate) fn texel(&self) -> f32 {
        self.width / FIELD as f32
    }

    /// The height pass's view: straight down over the region, x east, z south (as the
    /// field's texels run), the depth from `EYE` down to `EYE - FAR`.
    pub(crate) fn top_view_proj(&self) -> Mat4 {
        let (ox, oz, w) = (self.origin.x, self.origin.y, self.width);
        Mat4::orthographic_rh(ox, ox + w, -(oz + w), -oz, 0.0, FAR) * Mat4::look_at_rh(Vec3::new(0.0, EYE, 0.0), Vec3::ZERO, Vec3::NEG_Z)
    }
}

/// Where there's nothing for the height pass to see: that far down.
pub(crate) const NOTHING: f64 = -1000.0;

/// The cascades on the GPU: their textures, pipelines and bind groups.
pub(crate) struct Gi {
    /// the height pass draws into it (the heights, and the depth that keeps the highest)
    pub(crate) height_view: wgpu::TextureView,
    pub(crate) height_depth_view: wgpu::TextureView,
    a_view: wgpu::TextureView,
    b_view: wgpu::TextureView,
    params: wgpu::Buffer,
    field_layout: wgpu::BindGroupLayout,
    field_entries: [wgpu::TextureView; 3],
    clear_pipeline: wgpu::ComputePipeline,
    map_pipeline: wgpu::ComputePipeline,
    cascade_pipeline: wgpu::ComputePipeline,
    resolve_pipeline: wgpu::ComputePipeline,
    cascade_binds: Vec<wgpu::BindGroup>,
    resolve_bind: wgpu::BindGroup,
}

impl Gi {
    /// Whether the device runs the cascades: compute shaders writing storage textures.
    pub(crate) fn can_compute(device: &wgpu::Device) -> bool {
        let l = device.limits();
        l.max_compute_workgroups_per_dimension > 0 && l.max_compute_workgroup_size_x >= 8 && l.max_compute_workgroup_size_y >= 8 && l.max_storage_textures_per_shader_stage >= 2
    }

    /// Blank probes (nothing from anywhere), for a device that can't run the cascades.
    pub(crate) fn blank_views(device: &wgpu::Device) -> (wgpu::TextureView, wgpu::TextureView) {
        let one = |label: &str| texture(device, label, 1, FORMAT, wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());
        (one("blank probes"), one("blank probes' ways"))
    }

    pub(crate) fn new(device: &wgpu::Device, globals_layout: &wgpu::BindGroupLayout, ground_layout: &wgpu::BindGroupLayout, empty_layout: &wgpu::BindGroupLayout) -> Gi {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("indirect light"), source: wgpu::ShaderSource::Wgsl(GI_SHADER.into()) });
        let storage = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let field = texture(device, "gi field", FIELD, FORMAT, storage);
        let block = texture(device, "gi blocks", FIELD, wgpu::TextureFormat::R32Float, storage);
        let cascades: Vec<wgpu::Texture> = (0..CASCADES).map(|i| texture(device, &format!("gi cascade {i}"), PROBES * 2, FORMAT, storage)).collect();
        let a = texture(device, "gi probes", PROBES, FORMAT, storage);
        let b = texture(device, "gi probes' ways", PROBES, FORMAT, storage);
        let height = texture(device, "gi heights", FIELD, wgpu::TextureFormat::R32Float, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let height_depth = texture(device, "gi heights' depth", FIELD, crate::scene::DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gi params"),
            size: PARAMS * PARAMS_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
        let uniform = wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64) };
        let read = |sample_type: wgpu::TextureSampleType| wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false };
        let float = read(wgpu::TextureSampleType::Float { filterable: false });
        let write = |format: wgpu::TextureFormat| wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format, view_dimension: wgpu::TextureViewDimension::D2 };
        let field_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gi field"),
            entries: &[entry(8, uniform), entry(9, write(FORMAT)), entry(10, float), entry(11, float), entry(12, read(wgpu::TextureSampleType::Uint)), entry(13, write(wgpu::TextureFormat::R32Float))],
        });
        let cascade_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gi cascade"),
            entries: &[entry(8, uniform), entry(14, float), entry(15, float), entry(16, float), entry(17, write(FORMAT))],
        });
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gi resolve"),
            entries: &[entry(8, uniform), entry(14, float), entry(15, float), entry(16, float), entry(18, write(FORMAT)), entry(19, write(FORMAT))],
        });
        let pipeline = |label: &str, groups: &[Option<&wgpu::BindGroupLayout>]| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: groups, immediate_size: 0 });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(label), layout: Some(&layout), module: &shader, entry_point: Some(label), compilation_options: Default::default(), cache: None })
        };
        let clear_pipeline = pipeline("gi_clear", &[Some(empty_layout), Some(empty_layout), Some(&field_layout)]);
        let map_pipeline = pipeline("gi_map", &[Some(globals_layout), Some(ground_layout), Some(&field_layout)]);
        let cascade_pipeline = pipeline("gi_cascade", &[Some(empty_layout), Some(empty_layout), Some(&cascade_layout)]);
        let resolve_pipeline = pipeline("gi_resolve", &[Some(empty_layout), Some(empty_layout), Some(&resolve_layout)]);
        let params_entry = wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &params, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64) }) };
        let (field_view, block_view) = (view(&field), view(&block));
        let cascade_views: Vec<wgpu::TextureView> = cascades.iter().map(view).collect();
        let cascade_binds = (0..CASCADES)
            .map(|i| {
                // (the coarsest has no coarser one: the field stands in, never read)
                let coarser = cascade_views.get(i + 1).unwrap_or(&field_view);
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("gi cascade"),
                    layout: &cascade_layout,
                    entries: &[params_entry.clone(), tex(14, &field_view), tex(15, &block_view), tex(16, coarser), tex(17, &cascade_views[i])],
                })
            })
            .collect();
        let (a_view, b_view) = (view(&a), view(&b));
        let resolve_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gi resolve"),
            layout: &resolve_layout,
            entries: &[params_entry, tex(14, &field_view), tex(15, &block_view), tex(16, &cascade_views[0]), tex(18, &a_view), tex(19, &b_view)],
        });
        let height_view = view(&height);
        Gi { height_view: height_view.clone(), height_depth_view: view(&height_depth), a_view, b_view, params, field_layout, field_entries: [field_view, height_view, block_view], clear_pipeline, map_pipeline, cascade_pipeline, resolve_pipeline, cascade_binds, resolve_bind }
    }

    /// The probes as the lighting reads them.
    pub(crate) fn views(&self) -> (&wgpu::TextureView, &wgpu::TextureView) {
        (&self.a_view, &self.b_view)
    }

    /// The field passes' bind group for a scene target (what its last picture showed
    /// shining, and the looks that say where).
    pub(crate) fn target_bind(&self, device: &wgpu::Device, emit: &wgpu::TextureView, looks: &wgpu::TextureView) -> wgpu::BindGroup {
        let [field, height, block] = &self.field_entries;
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gi field"),
            layout: &self.field_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.params, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64) }) },
                tex(9, field),
                tex(10, height),
                tex(11, emit),
                tex(12, looks),
                tex(13, block),
            ],
        })
    }

    /// Writes the frame's parameters: the region, and each cascade's probes, directions,
    /// spacing, step and interval (the finest's interval two probe spacings long, each
    /// coarser one's four times its finer's; the steps never longer than a wall made a texel
    /// wider each way, so none steps over one).
    pub(crate) fn prepare(&self, queue: &wgpu::Queue, region: &Region) {
        let (t, s0) = (region.texel(), region.spacing());
        let region_v = [region.origin.x, region.origin.y, t, FIELD as f32];
        let heights = [0.0, 0.0, BOUNCE, SCREEN];
        let unit = 2.0 * s0;
        let reach = |i: usize| if i == 0 { 0.0 } else { unit * 4f32.powi(i as i32 - 1) };
        let mut bytes = vec![0u8; (PARAMS * PARAMS_STRIDE) as usize];
        let mut put = |k: usize, p: Params| bytes[k * PARAMS_STRIDE as usize..k * PARAMS_STRIDE as usize + std::mem::size_of::<Params>()].copy_from_slice(bytemuck::bytes_of(&p));
        put(0, Params { region: region_v, heights, ..Default::default() });
        for (i, &step) in STEP_TEXELS.iter().enumerate() {
            let probes = (PROBES >> i) as f32;
            let dirs = (4u32 << (2 * i)) as f32;
            let (coarser, coarser_dirs) = if i + 1 < CASCADES { ((PROBES >> (i + 1)) as f32, (4u32 << (2 * (i + 1))) as f32) } else { (0.0, 0.0) };
            let k = 2f32.powi(i as i32);
            put(1 + i, Params { region: region_v, probes: [probes, dirs, s0 * k, t * step], reach: [reach(i), reach(i + 1), coarser, coarser_dirs], heights });
        }
        put(1 + CASCADES, Params { region: region_v, probes: [PROBES as f32, 4.0, s0, t], heights, ..Default::default() });
        queue.write_buffer(&self.params, 0, &bytes);
    }

    /// Records the frame's compute passes: the field (each world map's texels with its
    /// placement and ground, by its offset in the places buffer), the cascades coarsest
    /// first, the probes.
    pub(crate) fn record(&self, encoder: &mut wgpu::CommandEncoder, globals_bind: &wgpu::BindGroup, empty_bind: &wgpu::BindGroup, target_bind: &wgpu::BindGroup, worlds: &[(u32, &wgpu::BindGroup)]) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("indirect light"), timestamp_writes: None });
        let groups = FIELD / 8;
        pass.set_pipeline(&self.clear_pipeline);
        pass.set_bind_group(0, empty_bind, &[]);
        pass.set_bind_group(1, empty_bind, &[]);
        pass.set_bind_group(2, target_bind, &[0]);
        pass.dispatch_workgroups(groups, groups, 1);
        pass.set_pipeline(&self.map_pipeline);
        for &(at, ground) in worlds {
            pass.set_bind_group(0, globals_bind, &[at]);
            pass.set_bind_group(1, ground, &[]);
            pass.dispatch_workgroups(groups, groups, 1);
        }
        pass.set_pipeline(&self.cascade_pipeline);
        pass.set_bind_group(0, empty_bind, &[]);
        pass.set_bind_group(1, empty_bind, &[]);
        for i in (0..CASCADES).rev() {
            pass.set_bind_group(2, &self.cascade_binds[i], &[((1 + i) as u64 * PARAMS_STRIDE) as u32]);
            pass.dispatch_workgroups(PROBES * 2 / 8, PROBES * 2 / 8, 1);
        }
        pass.set_pipeline(&self.resolve_pipeline);
        pass.set_bind_group(2, &self.resolve_bind, &[((1 + CASCADES) as u64 * PARAMS_STRIDE) as u32]);
        pass.dispatch_workgroups(PROBES / 8, PROBES / 8, 1);
    }
}

/// A texture view as a bind group's entry.
fn tex(binding: u32, v: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(v) }
}

fn texture(device: &wgpu::Device, label: &str, size: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compute shaders parse and hold as wgpu reads them, with the scene's own functions
    /// they call: the four passes, and the scene's entry points along with them.
    #[test]
    fn the_compute_shaders_hold() {
        let module = naga::front::wgsl::parse_str(GI_SHADER).unwrap_or_else(|e| panic!("{}", e.emit_to_string(GI_SHADER)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{}", e.emit_to_string(GI_SHADER)));
        let compute: Vec<&str> = module.entry_points.iter().filter(|e| e.stage == naga::ShaderStage::Compute).map(|e| e.name.as_str()).collect();
        assert_eq!(compute, ["gi_clear", "gi_map", "gi_cascade", "gi_resolve"]);
        assert!(module.entry_points.iter().any(|e| e.name == "fs_ground"));
    }

    /// The region sits round where the camera looks, on the probes' grid, and its height
    /// pass maps it onto the field: the north-west corner to the first texel, x east and z
    /// south, the depth from the eye down.
    #[test]
    fn the_region_lies_round_the_cameras_target_on_the_probes_grid() {
        let eye = Vec3::new(20.0, 30.0, 50.0);
        let view_proj = Mat4::perspective_rh(1.0, 1.5, 0.1, 400.0) * Mat4::look_at_rh(eye, Vec3::new(20.0, 0.0, 20.0), Vec3::Y);
        let r = Region::around(&view_proj, eye, 64.0);
        assert_eq!(r.spacing(), 0.5);
        assert_eq!(r.texel(), 0.25);
        let centre = r.origin + Vec2::splat(32.0);
        assert!((centre - Vec2::new(20.0, 20.0)).length() < 0.75, "{centre}");
        assert!((r.origin / 0.5 - (r.origin / 0.5).round()).length() < 1e-4, "on the grid: {}", r.origin);
        // a little further: the same origin until a spacing is crossed
        let moved = Region::around(&(view_proj * Mat4::from_translation(Vec3::new(-0.2, 0.0, 0.0))), eye + Vec3::new(0.2, 0.0, 0.0), 64.0);
        assert!((moved.origin - r.origin).length() <= 0.5 + 1e-4);
        // looking up: the camera's own foot
        let up = Mat4::perspective_rh(1.0, 1.5, 0.1, 400.0) * Mat4::look_at_rh(eye, eye + Vec3::new(0.0, 1.0, -1.0), Vec3::Y);
        let r2 = Region::around(&up, eye, 64.0);
        assert!((r2.origin + Vec2::splat(32.0) - Vec2::new(20.0, 50.0)).length() < 0.75);
        let top = r.top_view_proj();
        let project = |p: Vec3| {
            let c = top * p.extend(1.0);
            c.truncate() / c.w
        };
        let nw = project(Vec3::new(r.origin.x, 0.0, r.origin.y));
        assert!((nw - Vec3::new(-1.0, 1.0, EYE / FAR)).length() < 1e-4, "{nw}");
        let se = project(Vec3::new(r.origin.x + 64.0, 5.0, r.origin.y + 64.0));
        assert!((se - Vec3::new(1.0, -1.0, (EYE - 5.0) / FAR)).length() < 1e-4, "{se}");
    }
}
