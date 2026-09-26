# Ligature spike

Shapes sample strings with `QTextLayout` and compares each shaped glyph with the glyph its character gets on its own, then times shaping 10,000 code-like rows. The results and the decision are in [ADR 0015](../../docs/adr/0015-programming-ligatures.md).

```sh
cmake -S spikes/ligatures -B build/ligatures -DCMAKE_PREFIX_PATH=<Qt prefix>
cmake --build build/ligatures --config Release
build/ligatures/ligatures crates/opensesh-app/fonts/JetBrainsMono-Regular.ttf
```

On Windows, put Qt's `bin` folder on `PATH` first. The program runs on the offscreen platform.
