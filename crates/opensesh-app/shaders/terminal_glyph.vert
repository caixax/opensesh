#version 440
// Terminal glyph atlas material (cpp/terminal_render.cpp, ADR 0013): textured quads for glyphs,
// decorations and the cursor. `cargo xtask shaders` compiles it with `qsb -b`: the scene graph
// merges the row nodes into one batch, and the batchable rewrite adds an input at location 7, so
// this shader must not use that location.

layout(location = 0) in vec4 vertexCoord;
layout(location = 1) in vec2 textureCoord;
layout(location = 2) in vec4 vertexColor;
layout(location = 3) in float glyphKind;

layout(location = 0) out vec2 vTexCoord;
layout(location = 1) out vec4 vColor;
layout(location = 2) out float vKind;

layout(std140, binding = 0) uniform buf {
    mat4 modelViewMatrix;
    mat4 projectionMatrix;
    float qt_Opacity;
    float dpr;
};

void main()
{
    vTexCoord = textureCoord;
    // Vertex colors are premultiplied.
    vColor = vertexColor * qt_Opacity;
    vKind = glyphKind;
    // Snap every corner to the device pixel grid, as Qt's own native text shader does
    // (textmask.vert): glyphs are rasterized at device resolution and must map 1:1 to pixels.
    vec4 xformed = modelViewMatrix * vertexCoord;
    gl_Position = projectionMatrix * vec4(floor(xformed.xyz * dpr + 0.5) / dpr, xformed.w);
}
