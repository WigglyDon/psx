#version 300 es
precision highp float;
in vec4 vcolor;
in vec2 vuv;
in vec2 vlocal;
flat in vec4 vshape;
flat in vec2 vsurface;
uniform sampler2D glyphs;
uniform vec4 panelClip;
uniform float panelRadius;
out vec4 outputColor;
float roundedBox(vec2 p, vec2 halfSize, float radius) {
    vec2 q = abs(p) - halfSize + radius;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}
void main() {
    float coverage = 1.0;
    float light = 1.0;
    if (vsurface.x > 0.5) {
        // Signed distance to a rounded rectangle also describes circles and
        // rotated capsules. Derivatives keep the coverage ramp one physical
        // pixel wide at every density, orientation and custom control size.
        float d;
        if (vsurface.x > 1.5) {
            // Rounded molded cross, including the four inside corners.
            float a = roundedBox(vlocal, vshape.xy * vec2(0.34, 1.0), vshape.z);
            float b = roundedBox(vlocal, vshape.xy * vec2(1.0, 0.34), vshape.z);
            float k = max(vshape.z * 2.0, 0.001);
            float h = max(k - abs(a - b), 0.0) / k;
            d = min(a, b) - h * h * k * 0.25;
        } else {
            d = roundedBox(vlocal, vshape.xy, vshape.z);
        }
        if (vshape.w > 0.0) {
            d = abs(d + vshape.w * 0.5) - vshape.w * 0.5;
        }
        coverage = clamp(0.5 - d / max(fwidth(d), 0.0001), 0.0, 1.0);
        light += vsurface.y * clamp(-vlocal.y / max(vshape.y, 0.001), -1.0, 1.0);
    } else if (vuv.x >= 0.0) {
        coverage = texture(glyphs, vuv).r;
    }
    if (panelRadius > 0.0) {
        float d = roundedBox(gl_FragCoord.xy - panelClip.xy - panelClip.zw * 0.5,
            panelClip.zw * 0.5, panelRadius);
        coverage = min(coverage, clamp(0.5 - d / max(fwidth(d), 0.0001), 0.0, 1.0));
    }
    outputColor = vec4(vcolor.rgb * light, vcolor.a * coverage);
}
