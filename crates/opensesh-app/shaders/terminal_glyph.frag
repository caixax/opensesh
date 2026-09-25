#version 440
// Terminal glyph atlas material (cpp/terminal_render.cpp, ADR 0013). The atlas holds white
// coverage masks (text, decorations, a solid block) and premultiplied color glyphs (emoji).

layout(location = 0) in vec2 vTexCoord;
layout(location = 1) in vec4 vColor;
layout(location = 2) in float vKind;

layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 modelViewMatrix;
    mat4 projectionMatrix;
    float qt_Opacity;
    float dpr;
};

layout(binding = 1) uniform sampler2D atlas;

void main()
{
    vec4 t = texture(atlas, vTexCoord);
    // kind 0: coverage mask tinted with the vertex color; kind 1: color glyph, only faded by the
    // vertex alpha (hidden text, opacity).
    fragColor = mix(vColor * t.a, t * vColor.a, vKind);
}
