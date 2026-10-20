#version 300 es
layout(location=0) in vec2 position;
layout(location=1) in vec4 color;
layout(location=2) in vec2 uv;
layout(location=3) in vec2 local;
layout(location=4) in vec4 shape;
layout(location=5) in vec2 surface;
out vec4 vcolor;
out vec2 vuv;
out vec2 vlocal;
flat out vec4 vshape;
flat out vec2 vsurface;
void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    vcolor = color;
    vuv = uv;
    vlocal = local;
    vshape = shape;
    vsurface = surface;
}
