# Post-Process Effects

Screen-space visual effects applied after tonemapping via a custom ViewNode render graph
pass. Each effect runs once per camera frame regardless of how many instances are active,
and works on any camera without special pairing.

## Architecture

Each effect consists of:

1. A **pure-data component** (no mesh, no material) holding effect state and per-camera
   projection parameters, which gets extracted to the render world each frame.
2. A **ViewNode** that runs in the render graph and dispatches the fullscreen pass.
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

## Render Graph

The node is registered in the `Core2d` graph between `Node2d::Tonemapping` and
`Node2d::EndMainPassPostProcessing`:

```rust
render_app
    .add_render_graph_node::<ViewNodeRunner<MyEffectNode>>(Core2d, MyEffectLabel)
    .add_render_graph_edges(Core2d, (
        Node2d::Tonemapping,
        MyEffectLabel,
        Node2d::EndMainPassPostProcessing,
    ));
```

`ViewTarget::post_process_write()` provides a `(source, destination)` texture pair.
The node's `ViewQuery` must include `DynamicUniformIndex<MyEffect>` so the node only
fires for cameras that carry the component. The index is the per-camera byte offset into
the shared `DynamicUniformBuffer`.

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

4. Insert the component on the relevant cameras via `On<Add, CameraMarker>` observers.

5. Implement `ViewNode`:
   ```rust
   impl ViewNode for MyEffectNode {
       type ViewQuery = (
           &'static ViewTarget,
           &'static DynamicUniformIndex<MyEffect>,
       );
       fn run<'w>(..., (view_target, settings_index): QueryItem<'w, 'w, Self::ViewQuery>, ...) {
           // bind group + render pass …
           render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
       }
   }
   ```

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

## Existing Effects

| Effect | Component | Shader |
|--------|-----------|--------|
| Ripple displacement | `RipplePostProcess` in `weaponry/ripple_post_process.rs` | `shaders/ripple_post_process.wgsl` |
