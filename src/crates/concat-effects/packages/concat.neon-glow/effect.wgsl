struct Params { glow: f32, speed: f32 }

// Where the picture's colour steps - read against the colour two pixels
// to the right and the colour two pixels above, the two distances added -
// is an edge, and an edge is lit with a neon that turns through the hues,
// one channel a third of a turn behind the next, at a tenth of `speed`
// turns a second. The light comes in by `glow` hundredths of the step and
// is added to the picture's own without clipping it: light past white
// carries on past white, as what went in brighter should. The alpha is
// the picture's own throughout: a title's clear margins stay clear.
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = sample(uv);
    let t = texel() * 2.0;
    let c_r = sample(uv + vec2<f32>(t.x, 0.0));
    let c_u = sample(uv + vec2<f32>(0.0, t.y));
    let edge = length(c.rgb - c_r.rgb) + length(c.rgb - c_u.rgb);
    let hue = fract(frame.time * params.speed * 0.1);
    let neon_col = vec3<f32>(
        0.5 + 0.5 * sin(hue * 6.283),
        0.5 + 0.5 * sin(hue * 6.283 + 2.094),
        0.5 + 0.5 * sin(hue * 6.283 + 4.188)
    );
    let glow_val = edge * (params.glow * 0.03) * neon_col;
    return vec4<f32>(c.rgb + glow_val, c.a);
}
