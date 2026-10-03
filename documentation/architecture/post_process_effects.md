# Post-Process Effects

Screen-space visual effects applied after tonemapping, each as a system in the `Core2d`
schedule. Each effect runs once per camera frame regardless of how many instances are active.

Effects attach their dedicated components to mark the cameras that should render them.
These components also store each camera's parameters for GPU extraction.

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

Effect instances                    Shared instance Resource (extracted)
  └── gathered into a Resource          └── uploaded to GPU storage buffer
      ── ExtractResourcePlugin ─────►      once per frame, shared by cameras
```

The main-world `update` system fills the component with the camera's own world position
and orthographic size. Per-camera data is kept slim; shared effect data (e.g. all active
ripple entries) lives in a separate `Resource` extracted via `ExtractResourcePlugin` and
uploaded to a GPU storage buffer once per frame.

## Pass Registration & Ordering

Each effect plugin registers its pass in `Core2d` under its own `SystemSet`:

```rust
render_app.add_systems(Core2d, my_effect_pass.in_set(MyEffectPostProcessSet));
```

- **Pass sets:** defined in `visuals/src/post_process.rs`.
- **Pass order:** defined by `PostProcessOrderingPlugin` in `visuals_internal/src/post_process.rs`.

To add a pass: define its set, register the pass in that set, then add the set to `PostProcessOrderingPlugin` at the intended position.

## Executing a Pass

- `ViewQuery` includes `DynamicUniformIndex<MyEffect>` to select cameras with that effect and locate each camera's data in the uniform buffer.
- `ViewTarget::post_process_write()` supplies the source and destination textures for the fullscreen pass.
- `ExtractedCamera` provides the HDR check. The pipeline expects an `Rgba16Float` framebuffer; a camera without HDR would otherwise fail with a format mismatch.

## World ↔ UV Projection

Camera projection parameters are stored in the uniform struct itself. No view-matrix
bindings needed.

```wgsl
// UV (0..1, Y=0 at top) → world XY
fn uv_to_world(uv: vec2<f32>) -> vec2<f32> {
    let centered = uv - vec2<f32>(0.5, 0.5);
    // UV Y is inverted relative to world Y
    return camera.world_pos + centered * camera.viewport_size * vec2<f32>(1.0, -1.0);
}

// World XY → UV
fn world_to_uv(world: vec2<f32>) -> vec2<f32> {
    let centered = (world - camera.world_pos) / camera.viewport_size;
    return centered * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5, 0.5);
}
```

## WGSL Gotchas

**Shader import path** — the fullscreen vertex output lives at:
```wgsl
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
```

**Sampling in non-uniform control flow** — `textureSample` requires uniform control flow
(all SIMD lanes must reach the call, or none). Post-process shaders often branch or loop per
fragment. Use mip level 0 explicitly:
```wgsl
textureSampleLevel(screen_texture, screen_sampler, uv, 0.0)
```

Two less obvious ways to encounter the same error:
- Naga validates unreachable code. An early unconditional `return` does not prevent validation
  of a later `textureSample` in non-uniform control flow.
- A `return` inside a `for` loop makes later `textureSample` calls non-uniform across fragments.
  If the intent is to skip one iteration, use `continue` instead.

**GPU struct alignment** — uniform array elements need stride ≥ 16 bytes, aligned to 16.
For unbounded instance data, prefer a `var<storage, read>` buffer over uniform arrays.
Avoid `vec3<f32>` in GPU structs (implicit padding to 16 bytes).

**`arrayLength` is not an active-instance count** — Bevy's `StorageBuffer<T>` retains its
largest capacity, and the binding covers the whole buffer. `arrayLength(&buf)` can therefore
include stale entries when the active count falls. Pass the active count in a uniform and use
that as the loop bound.

## Existing Effects

| Effect | Component | Shader |
|--------|-----------|--------|
| Ripple displacement | `RipplePostProcess` in `weaponry_internal/src/ripple_post_process.rs` | `assets/shaders/weaponry/ripple_post_process.wgsl` |
| Force field dome | `ForceFieldPostProcess` in `weaponry_internal/src/force_field_post_process.rs` | `assets/shaders/weaponry/force_field_post_process.wgsl` |
| Quantum field anomaly | `QuantumFieldPostProcess` in `map_objects_internal/src/quantum_field_post_process.rs` | `assets/shaders/quantum_field/post_process.wgsl` |

Pass order (in the `Core2d` schedule): `Tonemapping → Ripple → ForceField → QuantumField → Upscaling`.
