# Rendering

The renderer supports a growing set of world visuals: sprites, procedural surfaces, transient
effects, and effects that distort the scene beneath them. Its design separates an effect's
appearance from the machinery that draws it. Most effects provide instance data and shader
functions; the renderer handles shared materials, batching, scene snapshots and distortion.

Three decisions shape the system:

- **Compose the world in bands.** Effects can act on the scene below a band, with full-screen
  work shared by the effects in that band.
- **Keep appearance in the domain.** A tower or projectile owns its visual state and shader look.
  Lanes provide reusable drawing mechanisms.
- **Share world data at its source.** A domain uploads the data its shaders and other consumers
  need. Effects read that shared copy rather than maintaining separate versions of the same grid.

## How the world becomes a frame

Draw order uses two coordinates: a **draw level** and a **layer**. Draw levels run from
`Underground` to `Airborne`. All visuals at one level draw before any visual at the next level.
Layers order visuals within a level, such as scorch marks below units or projectiles above them.
They can also keep instances of the same visual together for batching.

A **band** groups layers that share a scene snapshot and distortion pass. The bands are Ground,
Surface, Air, and Placement. Ground and Surface layers support selected terrain levels; Air
and Placement layers draw only at `Airborne`. A band has one composition boundary, before its
lowest depth across all draw levels. For example, the Air boundary lets an effect act on the
terrain beneath it while leaving aerial units and projectiles to draw afterward.

Each visual declares a `ZDepth` layer and a supported `DrawLevel`. `ZDepth` defines the supported
pairs and gives each pair a separate depth range, with stable entity offsets to reduce ties.
World-space depth is enforced independently of parent transforms, so a visual child can
follow its parent without inheriting the parent's render layer. The exact depth calculation
and transform rules belong to the [`ZDepth` API](../../game_core/src/z_depth.rs).

A camera renders the frame in this order:

1. Opaque and alpha-masked world content.
2. Transparent world content, sorted by draw level and then layer. At a band's composition
   boundary, the renderer performs a **cut** if effects need displacement or a scene snapshot.
   Each band has at most one cut, even when its layers appear at several draw levels.
3. Bloom and tonemapping of the composed world.
4. Screen UI, followed by final output scaling.

World-space indicators belong to the world bands. Screen UI draws afterward, so world
distortion does not affect it.

Only transparent content participates in banded ordering. A custom world material that needs
this ordering must use the transparent phase; an opaque material draws before all bands,
regardless of its layer.

## Cuts: effects that act on the scene

A cut processes the image accumulated before a band's lowest depth. It has three stages,
each performed only when needed:

1. **Displace.** Effects contribute sampling offsets and brightness gains to a shared buffer.
   Contributions add together, and one full-screen pass applies their sum to the scene.
2. **Capture.** If a visual needs to sample the scene, the renderer preserves
   that image as the band's **beneath snapshot**. Drawing continues on a copy.
3. **Overlay.** Effects draw over the scene in layer order, with access to the snapshot.

Ordinary transparent items, including **paint** visuals, draw at their assigned depth.
Paint visuals can request access to their band's snapshot.

### What a snapshot means

The snapshot contains the scene at the band's cut, after that cut's displacement. It stays
fixed while the band's overlays and paint visuals draw. All draw levels in a band use the same
snapshot. Raising a visual's draw level does not add intervening terrain or earlier overlays
to that snapshot.

This gives effects a predictable composition boundary. For example, a force field before the
air band refracts the ground and surface, then adds its shell light as an overlay. The shell
does not refract itself, and aerial units drawn afterward remain undistorted. Overlapping force
fields contribute to the same displacement operation. Each field's look reads the shared field
index to draw within the area its field owns, and to draw a seam between neighbours.

### Why cuts belong to bands

Sampling at each effect's exact depth would require a snapshot whenever a reader interleaves
with other content. Sharing a cut across a band bounds full-screen work by the bands that need
it, rather than by the number of effects. Displacement contributions shade their own quads;
their combined result needs one full-screen application per affected band. A snapshot adds one
full-screen copy, shared by all readers in that band and camera.

This choice has visible consequences:

- An effect sees the scene captured at its band's cut. Changing its layer or draw level within
  that band does not change the snapshot. Sampling later content requires a later cut.
- Displacement within one cut is additive, not a sequence of independent refractions.
  Sequential distortion requires separate composition boundaries.
- The accumulated image no longer separates its objects. To keep an object undistorted, draw
  it after the cut. Selectively exempting lower content would require separate render targets
  and later composition.

## Describing a visual

A **visual kind** pairs a component containing instance data with a WESL **look module** that
defines its appearance. Each entity carrying that component is an instance. A **lane** supplies
the rendering mechanism, allowing the same kind to combine several operations:

| Part | Purpose | Position in the frame |
|---|---|---|
| `PAINT` | Draw colour from the instance's appearance data | At the instance's layer and depth |
| `DISPLACE` | Offset and brighten samples of the accumulated scene | At the cut before its band |
| `OVERLAY` | Add light or redraw the scene using its snapshot | After displacement, before the band's paint |

An overlay can blend with or replace scene colour. It is useful when an effect must redraw
what it sees beneath itself, as a quantum field does. Paint is appropriate when the visual needs
ordinary depth ordering, including paint that samples the band's snapshot.

The domain decides when an instance exists, which layer it uses, and how gameplay changes its
parameters. Its look module determines the pixels. The lane handles instance buffers, shared
materials and shader entry points. This keeps rendering infrastructure reusable while leaving
appearance and gameplay relationships with the feature that owns them.

The [`LaneVisual` API](../../visuals/src/lane.rs) defines the shader contract and available options.
Its component data and shader instance structure must agree on layout. Adding a visual should
normally require a kind, its look module and plugin registration; a new rendering mechanism is
needed only when the existing parts cannot express its composition.

### Choosing how to draw

**Use a lane for repeated entity visuals.** Per-instance differences such as colour, seed,
intensity, size or start time belong in instance data. Paint shares a mesh and material among
instances of a kind while retaining each entity's transform, depth and Bevy visibility handling.
Effects such as an affliction can also be drawn inside their host's look by passing effect
parameters into that instance.

**Use a map canvas for continuous grid surfaces.** A canvas draws one quad over the map and
looks up cell data in the shader. This suits walls, ore and grid overlays, where appearance
depends on neighbouring cells and boundaries. The domain still owns the entities and gameplay
state; the canvas is their visual representation. To draw other visuals between parts of a surface,
use a separate quad for each depth. Walls use two passes: contact shadows, tunnel recesses, tunnel
interiors and lower ramp halves draw below wisps; wall tops and upper ramp halves draw above them.

**Use a custom material when instances need different rendering configuration.** Different
textures, shader definitions or blend modes may justify independent materials. A single
placement preview also gains little from instance batching. Custom materials can reuse look
modules and shared world data without using a lane.

## Data ownership and shader inputs

Visual data has five scopes:

| Scope | Meaning | Example |
|---|---|---|
| Instance | Values that differ between instances | Charge, radius, seed, start time |
| Kind | Resources shared by a visual kind's paint instances | A sprite texture used to mask a glow |
| World | Data uploaded by the domain that owns it | Obstacle cells or wall styles |
| Beneath | A camera's scene snapshot at a band boundary | The image a refracting field samples |
| Shader resources | Static GPU assets bound in the shared group alongside world data | Tileable noise textures |

Choose the scope by ownership and reuse. A gameplay grid needed by several effects should have
one shared GPU representation maintained by its owner. A presentation buffer used only to
highlight one overlay can remain local to that overlay. Values that do not vary at runtime can
be shared shader code or constants.

Shared data is bound by the rendering passes. Lane shaders can import its helpers without
carrying buffer handles in each material. Custom world and UI materials opt into the same
mechanism. During world rendering, it also supplies the band's beneath snapshot as per-camera
frame data. Screen UI shares world data but does not receive a band's snapshot. Static shader
resources such as the noise textures live in the same bind group; no gameplay domain owns or
updates them.

The CPU remains authoritative for gameplay state. GPU copies let shaders interpret that state
visually: for example, a scorch on a wall top can mask itself against current obstacle cells,
so the mask follows walls built or destroyed during the effect. The scorch does not need its
own obstacle snapshot or upload system.

The owning domain is responsible for the shader-facing layout and for making valid data
available before it is drawn. A buffer's existence alone does not guarantee that its contents
are ready for a consumer. Binding numbers, layouts and upload contracts are maintained with
the [shared-data API](../../visuals/src/shared_gpu.rs) and the corresponding shader modules.

### Noise

[`common/noise.wesl`](../../assets/shaders/common/noise.wesl) is the default source of continuous
noise for effects and surfaces. It samples tileable lattice textures bound in the shared GPU group:
value noise uses one hardware-filtered texture fetch per octave (instead of 8 corner hashes in 3D
or 4 in 2D), and 2D gradient noise fetches the four cell corners with two `textureGather` calls per
octave. The 2D textures have 1024² cells and the 3D texture has 128³ cells, large enough to avoid
visible tiling.

Each value-noise texture is single-channel (`R8Unorm`): packing additional lattices into extra
channels would share the same integer lattice planes, producing axis-aligned seams when one field
domain-warps another. Independent noise fields instead sample the same texture at offset
coordinates.

The KTX2 textures are generated by `lab-server noise` in `dwd-shader-lab`. Regenerating a texture
changes the lattice for every effect that samples it.

Because texture noise lives in the shared GPU group, passes that do not bind the group (such as the
opaque pass) cannot use `common/noise.wesl`.
[`common/value_noise.wesl`](../../assets/shaders/common/value_noise.wesl) and
[`common/gradient_noise.wesl`](../../assets/shaders/common/gradient_noise.wesl) compute equivalent
noise procedurally from coordinate hashes; they serve both as the reference implementation and as a
fallback for pipelines without the shared group.

### Time and temporary visuals

Choose a clock according to the visual's intended pause behaviour. Game time stops while the
game is paused; Bevy's shader clock continues. A gameplay-timed blast should age on game time,
while an ambient or UI animation may continue during a pause.

Both clocks wrap to zero every `CLOCK_WRAP_PERIOD` seconds to preserve `f32` precision over
long sessions:

- **Elapsed time and ages.** Store start times as a [`ClockInstant`](../../visuals/src/clock.rs),
  which pairs the clock's wrap count (`epoch`) with the wrapped seconds. Elapsed time between
  two instants remains continuous across wraps, so long-lived or finished visuals never jump or
  replay at the wrap boundary. Temporary world visuals pair a start instant and duration with a
  `TimeBound` that despawns the entity. `ClockInstant::shader_now` stamps Bevy's shader clock;
  `ClockInstant::at` converts an elapsed time such as `GameClock::now()`. Shaders read both
  clocks through [`common/clock.wesl`](../../assets/shaders/common/clock.wesl) via the shared
  GPU group.
- **Seamless periodic animation.** A looping animation has no seam at the wrap when an integer
  number of cycles fits into `CLOCK_WRAP_PERIOD`. `common/clock.wesl` provides `wrap_period`
  and `wrap_rate` to quantize periods and rates to the wrap period, and `wrap_index` to fold
  per-cycle indices modulo the cycle count per wrap.
- **Drifting noise.** The `*_drift` functions in
  [`common/noise.wesl`](../../assets/shaders/common/noise.wesl) fold completed wrap periods
  (`shader_clock_epoch`) into the texture's tile period before adding `velocity * time`, so noise
  scrolls continuously across the wrap boundary at any velocity. Pass `globals.time` (plus any
  static phase offset such as a per-instance seed) and a `velocity` to `*_drift` rather than
  folding time into `p` manually. Because `*_drift` uses `shader_clock_epoch`, it is only valid for
  Bevy's shader clock; effects that pause with the game clock should pass elapsed game time
  (`game_time_since`) into a static noise function's sample coordinates instead. For procedural
  hash noise, [`common/value_noise.wesl`](../../assets/shaders/common/value_noise.wesl) quantizes
  drift velocity so the lattice repeats once per wrap, while non-periodic hash noise (such as
  [`common/gradient_noise.wesl`](../../assets/shaders/common/gradient_noise.wesl)) crossfades into
  a wrap-shifted sample (`time - CLOCK_WRAP_PERIOD`) using `wrap_fade` and `wrap_mix`.

## Performance tradeoffs

Paint lanes share material state, but transparent ordering still determines batching.
Bevy can merge adjacent compatible instances; content sorted between them splits the draw.
A dedicated layer can improve batching for a kind with many instances, provided that layer also
gives the intended visual order. Each paint entity still incurs extraction, visibility, queueing
and sorting work.

Paint buffers retain stable slots and upload on change. This avoids reshuffling instances when
others are removed, at the cost of retaining capacity after the population shrinks and uploading
the whole buffer when an instance changes. Shader-driven animation reduces those changes.
Cut instance lists are rebuilt each frame and draw as instanced quads, so their collection and
upload cost follows the number of gathered instances.

Draw count is only part of the cost. Large translucent quads can shade the same screen area
many times, and cuts process full camera targets. Keep geometry close to the visible effect
and introduce composition boundaries for a visual need, rather than giving each effect its own
cut.

## Integration with Bevy

The game uses its own camera render schedule to place cuts inside transparent world rendering.
It reuses Bevy's public passes where possible and adapts the transparent and UI paths to provide
the composition and shared data described above. Bevy itself is not forked.

Owning the schedule also means owning integration with engine changes. Systems added to Bevy's
default schedule do not automatically run for the game's cameras. Compatibility notes and the
set of adapted passes live with the
[render schedule](../../visuals_internal/src/render_schedule.rs), where they can be reviewed
during upgrades.

The world renders to HDR targets before bloom and tonemapping. The main camera's bloom adds
light from colour above 1.0, so luminous effects should output HDR colour. Cuts preserve this
range, allowing distorted surfaces and overlay light to participate in bloom together.
The cameras disable MSAA: most visual edges come from textures or procedural alpha inside
quads, while the cut pipeline requires single-sample textures.

Two implementation limits matter when extending the renderer:

- The opaque and alpha-masked pass does not bind shared world data. Supporting consumers there
  requires adapting that pass as well as declaring the shader input.
- Bevy can batch transparent items across a band boundary. Such a batch draws with its first
  item, so content above the cut can draw early and receive that cut's distortion. The
  boundary lookup currently does not split those batches.

## Batched painting (planned)

A further painting mode is designed for large populations of short-lived visuals, such as
sparks or per-cell heat. It is not implemented and has no current consumer.

The intended API keeps the kind's instance data and look module, with a flag selecting the
batched mode. It accepts entities or a per-frame list of instances and draws once per kind per
layer, alongside other transparent content at that layer's depth. Instances use quads and draw
in a stable buffer order: entity index, then insertion order for submitted instances.

This mode trades individual depth, arbitrary meshes and Bevy's per-entity visibility handling
for lower extraction, queueing and sorting overhead. The lane chooses which instances to
include and rebuilds their buffers every frame, even when their data has not changed.
It becomes useful when that reduction in per-entity work outweighs repeated uploads, or when
visual instances do not need entities at all.
