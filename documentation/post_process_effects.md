# Post-Process Effects

Screen-space visual effects applied after tonemapping, each as a system in the `Core2d`
schedule. Each effect runs once per camera frame regardless of how many instances are active,
and works on any camera carrying the effect component without special pairing.

## Architecture

Each effect consists of:

1. A **pure-data component** (no mesh, no material) holding effect state and per-camera
   projection parameters, which gets extracted to the render world each frame.
2. A **render-pass system** that runs in the `Core2d` schedule and dispatches the fullscreen pass.
3. A **WGSL shader** for the pixel manipulation.

## Data Flow

```
Main World                           Render World
─────────────────────────────────────────────────────
Camera entity                        Camera entity (extracted)
  ├── MyEffect component               ├── MyEffect component
  │   filled by Update system          │   copied by ExtractComponentPlugin
  │   (effect data + camera params)    ├── DynamicUniformIndex<MyEffect>
  │                                    │   offset into DynamicUniformBuffer,
  └── Projection::Orthographic         │   added by UniformComponentPlugin
      → camera_world_pos /             └── ViewTarget
        viewport_world_size                ping-pong screen textures
        packed into MyEffect
```

The main-world `update` system fills the component with the camera's own world position
and orthographic size. Per-camera data is kept slim; shared effect data (e.g. all active
ripple entries) lives in a separate `Resource` extracted via `ExtractResourcePlugin` and
uploaded to a GPU storage buffer once per frame.

## Pass Registration & Ordering

Each effect plugin registers **only its own pass system** in the `Core2d` schedule, bound to a
shared `SystemSet` — it does *not* declare its own ordering:

```rust
render_app.add_systems(Core2d, my_effect_pass.in_set(MyEffectPostProcessSet));
```

The `SystemSet` types and the order of *all* post-process passes live in one place,
`lib-core/src/post_processing.rs`: the shared sets plus `PostProcessOrderingPlugin`, which pins the
whole chain with `configure_sets(Core2d, …)`. Every set is placed `.in_set(Core2dSystems::PostProcess)`
and `.after(...)` the previous one, yielding
`Tonemapping → Ripple → ForceField → QuantumField → Upscaling`. That plugin is added **last** in
`main.rs`, after every effect plugin, so each set is already populated when the ordering is applied.
To add a new pass: define its set in `lib_core::post_processing`, add your system to it in your
plugin, and splice the set into `PostProcessOrderingPlugin`'s chain at the position you want.

`ViewTarget::post_process_write()` provides a `(source, destination)` texture pair. The pass
system's `ViewQuery` includes `DynamicUniformIndex<MyEffect>` so it only fires for cameras carrying
the component (the index is the per-camera byte offset into the shared `DynamicUniformBuffer`), and
`ExtractedCamera` so the pass can assert the camera is HDR — the pipelines are built only for the
`Rgba16Float` (HDR) framebuffer, so a non-HDR `PostProcessCamera` would panic with an actionable
message rather than a cryptic format mismatch.

## World ↔ UV Projection

Camera projection parameters are stored in the uniform struct itself. No view-matrix
bindings needed.

```wgsl
// UV (0..1, Y=0 at top) → world XY
fn uv_to_world(uv: vec2<f32>) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    // UV Y is inverted relative to world Y
    return camera_world_pos + centered * viewport_world_size * vec2<f32>(1.0, -1.0);
}

// World XY → UV
fn world_to_uv(world: vec2<f32>) -> vec2<f32> {
    let centered = (world - camera_world_pos) / viewport_world_size;
    return centered * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5, 0.5);
}
```

## Adding a New Effect

1. Define the component with all derives:
   ```rust
   #[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
   pub struct MyEffect {
       camera_world_pos: Vec2,
       viewport_world_size: Vec2,
       // effect-specific data …
   }
   ```

2. Register extraction and uniform upload in the plugin:
   ```rust
   app.add_plugins((
       ExtractComponentPlugin::<MyEffect>::default(),
       UniformComponentPlugin::<MyEffect>::default(),
   ));
   ```

3. Add an `Update` system that fills each camera's component from `(&Transform, &Projection)`.

4. Insert the component on `PostProcessCamera` entities via an `On<Add, PostProcessCamera>` observer.

5. Add the render-pass **system** (runs in `Core2d`, bound to your set):
   ```rust
   fn my_effect_pass(
       view: ViewQuery<(
           &ViewTarget,
           &DynamicUniformIndex<MyEffect>,
           &MyEffect,
           &ExtractedCamera,
       )>,
       pipeline_res: Res<MyEffectPipeline>,
       pipeline_cache: Res<PipelineCache>,
       mut ctx: RenderContext,
   ) {
       let (view_target, settings_index, settings, camera) = view.into_inner();
       assert!(camera.hdr, "my effect post-process requires an HDR camera");
       // fetch pipeline, build bind group, run the fullscreen pass …
       render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
   }
   ```
   Register it with `render_app.add_systems(Core2d, my_effect_pass.in_set(MyEffectPostProcessSet))`
   and build the pipeline in a `RenderStartup` system.

6. Write the WGSL shader (see gotchas below).

## WGSL Gotchas

**Shader import path** — the fullscreen vertex output lives at:
```wgsl
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
```
`fullscreen_vertex_output` does NOT exist. The error will manifest as
`Pipeline could not be compiled because the following shader could not be loaded` —
persistent, no WGSL error text.

**`textureSample` vs `textureSampleLevel`** — `textureSample` requires uniform control
flow (all SIMD lanes must reach the call, or none). Post-process shaders commonly have
per-fragment branches or loops with `continue`, which breaks this guarantee. Use:
```wgsl
textureSampleLevel(screen_texture, screen_sampler, uv, 0.0)
```
Post-process effects always want mip level 0 (full resolution), so this is always correct.

**Dead code validation** — Naga validates unreachable code paths. An early unconditional
`return` that makes a `textureSample` call unreachable will still produce a compile error
if that call is in non-uniform control flow. Keep shaders structurally clean.

**`return` inside `for` loops** — a `return` inside a loop causes different fragments to
exit the function at different points. Any `textureSample` after the loop is then in
non-uniform control flow. Use `continue` to skip loop iterations instead of `return` to
exit the function early.

**GPU struct alignment** — uniform array elements need stride ≥ 16 bytes, aligned to 16.
For unbounded instance data, prefer a `var<storage, read>` buffer over uniform arrays.
Avoid `vec3<f32>` in GPU structs (implicit padding to 16 bytes).

**Storage buffers grow but never shrink — don't trust `arrayLength`** — Bevy's `StorageBuffer<T>`
keeps the largest capacity it has ever held, and the binding covers the whole buffer. So
`arrayLength(&buf)` returns that high-water capacity, *not* the current element count: after the
count drops, the shader reads stale trailing entries from a previous (larger) frame. Loop on an
explicit count passed in the uniform instead — as the force field's `field_count` and the quantum
field's `collapse_count` do. (Symptom we hit: a scan-collapse disc frozen at a former spot once
the active count dropped from a peak.)

## Existing Effects

| Effect | Component | Shader |
|--------|-----------|--------|
| Ripple displacement | `RipplePostProcess` in `weaponry/ripple_post_process.rs` | `shaders/ripple_post_process.wgsl` |
| Force field dome | `ForceFieldPostProcess` in `weaponry/force_field_post_process.rs` | `shaders/force_field_post_process.wgsl` |
| Quantum field anomaly | `QuantumFieldPostProcess` in `map_objects/quantum_field_post_process.rs` | `shaders/quantum_field_post_process.wgsl` |

Pass order (in the `Core2d` schedule): `Tonemapping → Ripple → ForceField → QuantumField → Upscaling`.
