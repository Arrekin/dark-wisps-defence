// Yellow ceiling in `fire_ramp`, reserving white (`> 0.86`) for light and electric effects.
const FIRE_HEAT_CAP: f32 = 0.86;

// Heat → flame colour, red-dominant: deep ember → red → orange → yellow → white.
fn fire_ramp(h: f32) -> vec3<f32> {
    let ember  = vec3<f32>(0.35, 0.01, 0.00);
    let red    = vec3<f32>(0.85, 0.07, 0.02);
    let orange = vec3<f32>(1.00, 0.28, 0.04);
    let yellow = vec3<f32>(1.00, 0.65, 0.18);
    let white  = vec3<f32>(1.00, 0.92, 0.75);
    var c = ember * smoothstep(0.02, 0.18, h);
    c = mix(c, red,    smoothstep(0.18, 0.42, h));
    c = mix(c, orange, smoothstep(0.42, 0.66, h));
    c = mix(c, yellow, smoothstep(0.66, FIRE_HEAT_CAP, h));
    c = mix(c, white,  smoothstep(FIRE_HEAT_CAP, 1.00, h));
    return c;
}
