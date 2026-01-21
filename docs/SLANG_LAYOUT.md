## slang shader layout

in `crates/slang`:
- infer push constant ranges from shader
- infer all desciptor sets and bindings from shader
- infer vertex attribute layouts from shader
- infer render target attachment layouts from shader
- create inferred descriptor layouts
- create inferred pipeline layouts
- create inferred vertex input binding descriptions
- create inferred render attachment layout (everything sans formats)
- keep track of std140 vs std430 byte layouts
- also need to infer specialization values (TODO later?)

root level `SlangLayout` that can be queried for per-stage layouts and
shader-global layouts. all these things need to impl Eq and Hash

proc macros to generate structs for:
- push constants
- uniform buffers
- vertices (always assume interleaving)

bytemuck utils to quicky convert slices of these generated structs into bytes
for easy memcpying into buffers

the output of a compiled, reflected, and inferred slang shader module will be
something like `SlangShader` with a `layout() -> SlangLayout` method. the user
can create descriptor layouts, pipeline layouts, vertex input binding
descriptions, and render attachments using this

OPEN QUESTION: how can we generate structs using macros for vertices, push
constants, uniforms, etc? I think we need some kind of `include_shader!` macro
that reads the shader source and returns a 'static `SlangShader`
