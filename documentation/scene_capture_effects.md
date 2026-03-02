# Scene Capture Effects

In-world visual effects that transform the rendered scene at their world-space position. Unlike screen-space post-processing, these effects exist as meshes in the world, so they respect camera zoom and pan automatically.

## How It Works

A secondary camera renders the scene to an off-screen texture before the main camera runs. Effect meshes then sample that texture, apply a transformation, and composite the result into the main render. Because the projection matrices are kept identical, any world position can be mapped to the correct texel in the captured image.

## Render Layer Layout

| Layer | Seen by | Contains |
|-------|---------|----------|
| 0 (default) | Both cameras | All normal game entities |
| 1 | Main camera only | In-world effect meshes |

The capture camera defaults to layer 0. The main camera is given `RenderLayers::from_layers(&[0, 1])`. Effect meshes are placed on `RenderLayers::layer(1)`. This ensures effect meshes are never captured in the scene texture, preventing feedback loops.

## Infrastructure (`lib_core::camera`)

**`SceneTexture`** — `Resource<Handle<Image>>` pointing to the off-screen render target. Created at startup and replaced on every `WindowResized` event to match the physical window size. Any effect material that needs the pre-rendered scene clones this handle.

**`SceneCaptureCamera`** — marker on the secondary camera (`order: -1`). Its transform and projection are synced to `MainCamera` every frame by `sync_with_main`. Its render target is updated by `update_texture` alongside `SceneTexture`.

## World-to-UV Projection

To map any world-space position to a UV coordinate in the scene texture, use `clip_from_world` from the view bindings:

```wgsl
#import bevy_sprite::mesh2d_view_bindings::view

let clip = view.clip_from_world * world_position;
let ndc  = clip.xy / clip.w;
let uv   = ndc * vec2<f32>(0.5, -0.5) + 0.5;
// NDC y flips because NDC +1 = top but UV v=0 = top
```

This projection respects the camera's current zoom and pan, so the sampled position always matches the correct pixel of the pre-rendered scene.

## Adding a New Effect

1. **Material**: define a `Material2d` with `scene_texture: Handle<Image>` at group 2 bindings 0/1.
2. **Spawn**: inject `Res<SceneTexture>` and clone `scene_texture.0` into the material at spawn time.
3. **Layer**: assign `RenderLayers::layer(1)` to the effect mesh entity.
4. **Shader**: use the world-to-UV formula above to sample the scene texture. Return `alpha = 0.0` outside the effect region and `alpha = 1.0` inside.
5. **Alpha mode**: use `AlphaMode2d::Blend` so multiple overlapping instances of the same effect compose independently rather than overwriting each other.

### Seamless boundaries

When using displacement (sampling at an offset from the fragment's actual position), the displacement must reach zero at the boundary of the effect region. This makes the transition to the transparent exterior invisible, since at zero displacement the sampled pixel equals what the normal render would show there.
