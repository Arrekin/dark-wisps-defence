import bevy_sprite_render::mesh2d::{
    functions::get_world_from_local,
    vertex_output::VertexOutput,
    view_bindings::{globals, view},
};
import package::shaders::void_sea::look::{
    CLING, DROP_HEIGHT, DROP_OUT, DROP_SIZE, FOAM_CORE_COLOR, SEA_SHADOW, glass_crossing, glass_face,
    sea_surface, spray_drops,
};
import package::shaders::void_sea::waves::{
    WATER_HEIGHT, glass_distance, map_size, wall_coordinate, wall_lift,
};

// The void sea around the map. Its surface stands `WATER_HEIGHT` above the map, held back by invisible
// glass along the map's edge. A slight perspective from each camera lifts the surface away from the
// view's centre. From inside the map that opens a view of the water's face through the glass; from out
// at sea the surface overhangs the map's near edge.
//
// The frame mesh has eight vertices: 0..3 trace the view's corners and 4..7 the part of the map that
// the sea cannot cover, both counter-clockwise from the bottom left. The vertex shader places them for
// each view, so the sea shades only the pixels it can cover.

// Height of the perspective's eye above the map, per world unit of the view's height. Every camera
// sees the same picture at its own scale.
const CAMERA_HEIGHT_PER_VIEW_HEIGHT: f32 = 0.72;
// Lowest eye height, so the highest spray stays well below the eye in small views.
const MIN_CAMERA_HEIGHT: f32 = 500.0;
// The highest anything of the sea reaches above the map: the spray's top.
const HIGHEST: f32 = WATER_HEIGHT + 1.4 * DROP_HEIGHT;
// Distance from the glass within which water climbing it raises the surface.
const MOUND_WIDTH: f32 = 30.0;

struct SeaVertex {
    @builtin(instance_index) instance_index: u32,
    // x holds the vertex's number in the frame.
    @location(0) position: vec3<f32>,
};

// The view's height in world units.
fn view_height() -> f32 {
    return 2.0 / view.clip_from_world[1][1];
}

fn camera_height() -> f32 {
    return max(CAMERA_HEIGHT_PER_VIEW_HEIGHT * view_height(), MIN_CAMERA_HEIGHT);
}

@vertex
fn vertex(in: SeaVertex) -> VertexOutput {
    let screen_low = (view.world_from_clip * vec4<f32>(-1.0, -1.0, 0.0, 1.0)).xy;
    let screen_high = (view.world_from_clip * vec4<f32>(1.0, 1.0, 0.0, 1.0)).xy;
    var low = min(screen_low, screen_high);
    var high = max(screen_low, screen_high);
    let index = u32(in.position.x + 0.5);
    if index >= 4u {
        // Ground points whose line of sight reaches the highest spray only above the map. Scaled from
        // the map about the camera, so the region shrinks toward the far side when the camera is out
        // at sea.
        let camera = view.world_position.xy;
        let eye = camera_height();
        let reach = eye / (eye - HIGHEST);
        let hole_low = max(vec2<f32>(0.0), camera - camera * reach);
        let hole_high = min(map_size(), camera + (map_size() - camera) * reach);
        // Clamped to the view, and collapsed onto its low corner when empty.
        low = clamp(hole_low, low, high);
        high = clamp(hole_high, low, high);
    }
    let corner = index % 4u;
    let at = vec2<f32>(f32(corner == 1u || corner == 2u), f32(corner >= 2u));
    let world = vec3<f32>(mix(low, high, at), get_world_from_local(in.instance_index)[3].z);

    var out: VertexOutput;
    out.position = view.clip_from_world * vec4<f32>(world, 1.0);
    out.world_position = vec4<f32>(world, 1.0);
    out.instance_index = in.instance_index;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.world_position.xy;
    let t = globals.time;
    let camera = view.world_position.xy;
    let eye = camera_height();
    let pixel = view_height() / view.viewport.w;
    let ground_distance = glass_distance(p);

    // The surface point the line of sight meets. Water climbing the glass forms a mound against it that
    // falls off away from the glass. Near the glass, a few steps find where the line of sight meets it.
    var surface = camera + (p - camera) * (eye - WATER_HEIGHT) / eye;
    var d = glass_distance(surface);
    if d < 5.0 * MOUND_WIDTH {
        for (var i = 0; i < 3; i++) {
            let height = WATER_HEIGHT + wall_lift(wall_coordinate(surface), t) * exp(-max(d, 0.0) / MOUND_WIDTH);
            surface = camera + (p - camera) * (eye - height) / eye;
            d = glass_distance(surface);
        }
    }
    let sea_height = WATER_HEIGHT + wall_lift(wall_coordinate(p), t);

    // The surface and the glass crossing are evaluated everywhere so their screen-space derivatives stay
    // defined; the glass face is then shaded only where it shows.
    let on_surface = sea_surface(surface, max(d, 0.0), t, fwidth(surface.x));
    let crossing = glass_crossing(p, surface, sea_height);
    let below_width = fwidth(crossing.below_surface);
    let along_width = fwidth(crossing.wall.x);
    var color = on_surface;
    if d < 0.0 {
        color = glass_face(crossing, below_width, along_width, t);
    }

    // Outside the map the sea covers the ground. Over the map it shows only where the surface overhangs it.
    let surface_pixel = pixel * (eye - WATER_HEIGHT) / eye;
    let alpha = max(smoothstep(-pixel, 0.0, ground_distance), smoothstep(-surface_pixel, 0.0, d));

    // Spray stays near the glass, on either side of it. The highest drops are carried furthest from the
    // camera by the perspective, over the map when the camera is out at sea.
    let highest_lift = eye / (eye - HIGHEST);
    let spray_reach = CLING + DROP_OUT + (highest_lift - 1.0) * 1.1 * length(p - camera) + 2.0 * DROP_SIZE * highest_lift;
    if abs(ground_distance) < spray_reach {
        let spray = spray_drops(p, camera, eye, t, pixel);
        if d >= 0.0 {
            color *= 1.0 - spray.shadow * SEA_SHADOW * (1.0 - spray.cover);
        }
        if alpha <= 0.0 {
            color = FOAM_CORE_COLOR;
        }
        return vec4<f32>(mix(color, FOAM_CORE_COLOR, spray.cover), max(alpha, spray.cover));
    }
    return vec4<f32>(color, alpha);
}
