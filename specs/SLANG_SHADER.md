## shader compilation and linking

the `slang` crate in this repo is a wrapper around [slang-rs](https://github.com/FloatyMonkey/slang-rs). it's responsible for:
- compiling and linking slang shaders
- providing reflection information to the renderer for pipeline creation

```rust
struct SlangCompilerBuilder {
    fn new() -> Self;

    // finds the capability in the global session and sets it on the compiler
    // settings, otherwise errors
    fn capability(mut self, name: &str) -> Result<Self>;

    fn bindless_space_index(mut self, index: i32) -> Self;
    fn optimization(mut self, ...) -> Self;
    fn matrix_layout_row(mut self, ...) -> Self;

    // adds a search path for resolving modules
    fn search_path<P: AsRef<Path>>(mut self, P) -> Self;


    // instantiates a compiler instance
    fn build(self) -> SlangCompiler;
}
```

hardcode vulkan_use_entry_point_name(true)
hardcode format spirv
hardcode profile spirv_1_6 (this is guaranteed by min vk 1.3)

NOTE: we use term "entrypoint", while slang-rs uses "entry_point"

all of these `SlangCompilerBuilder` option values need to be incorporated into calculated `SlangProgramKey`s

```rust
// sort components by modules first, then entrypoints
// sort modules by some module-unique identifier
// sort entrypoints by name

struct SlangCompiler {
    // stages the module for compilation and returns a SlangModule for entrypoint selection
    fn load_module(&mut self, path) -> Result<SlangModule>;

    // returns a linker builder
    fn linker(&mut self) -> SlangLinker;
}

// link components
// pull in the modules for each entrypoint so progression looks like:
// [] start with empty component set
// [moduleA, epA_V, epA_F] add two entrypoints, pull in module
// [moduleA, moduleB, epA_V, epA_F, epB_F] add one entrypoint, pull in module
// sort components by modules first, then entrypoints
// sort modules by some module-unique identifier (module id)
// sort entrypoints by (stage, name, module id)
//
// components are sorted into a canonical ordering for deterministic layouts and
// stable cache key hashing
struct SlangLinker {
    // adds the module *and* all of its entrypoints
    fn add_module(mut self, module: &SlangModule) -> Self;

    // adds all entrypoints for a specific stage from a module. also adds the
    // module to the component list if it wasn't already
    fn add_stage(mut self, module: &SlangModule, stage) -> Self;

    // adds all entrypoints, regardless of stage, in a module. also adds the
    // module to the component list if it wasn't already
    fn add_all_entrypoints(mut self, module: &SlangModule) -> Self;

    // adds a single entrypoint. also adds the parent module to the component
    // list if it wasn't already
    fn add_entrypoint(mut self, entrypoint: SlangEntrypoint) -> Self;

    // defer calling any slang compilation functions _until link() is called_ so
    // that we can compute the `SlangProgramKey` for the current compiler+linker
    // state and potentially reuse a previously compiled artifact
    fn link(self) -> Result<SlangProgram>;
}
```

given the same set of modules + entrypoints, the `slang` crate defines a canonical composition ordering. layout/binding ordering follows this policy, not caller insertion order

```rust
struct SlangModule {
    fn module_id(&self) -> ModuleId;
    fn entrypoints(&self) -> impl Iterator<Item = SlangEntrypoint>;
    fn entrypoint(&self, stage, name: &str) -> Result<SlangEntrypoint>;
}

// bump this whenever we need to release a new version and invalidate old cache
// entries
const SLANG_CACHE_KEY_VERSION: u8 = 1;

// copyable handle to the entrypoint
struct SlangEntrypoint {
    name: CompactString, // own the name for simplicity

    fn module_id(&self) -> ModuleId;
    fn stage(&self) -> ShaderStage;
    fn name(&self) -> &str
}

// can assume there is only ever one target so always layout(0)
// implies always entry_point_code(e, 0) for some entrypoint
struct SlangProgram {
    // the entire program and all of its compiled spirv blobs are cached
    fn cache_key(&self) -> SlangProgramKey;

    // get the program layout
    fn layout(&self) -> SlangLayout;

    // get the entrypoints in this program
    fn entrypoints(&self) -> impl Iterator<Item = SlangEntrypoint>;

    // get the program bytecode
    fn code(&self, entrypoint: SlangEntrypoint) -> SpirvCode
}
```

the `SlangCompiler` must maintain an internal "program cache" so that when the same compiler options, modules, and endpoints are requested, it can compute the `SlangProgramKey` and look up an existing `SlangProgram` in the program cache

that cache should internally hold items like this (example):
```rust
struct SlangProgramCache {
    layouts: HashMap<SlangProgramKey, SlangLayout>,
    blobs: HashMap<SpirvCodeKey, SpirvCode>,
}
```

```rust
impl Clone        for SpirvCode { ... }
impl AsRef<[u32]> for SpirvCode { ... } // dereference to 32-bit words

// just an owned blob of spirv bytes
// guaranteed to be 4-byte aligned
impl SpirvCode {
    fn cache_key() -> SpirvCodeKey;
    fn len_words() -> usize;
    fn len_bytes() -> usize;
}

// hash key calculated from:
// - slang compiler version
// - global version number
// - compiler options (the non-hardcoded ones)
// - all linked components (their sorted order confers stable identity)
//   - modules -> module source blake3 hash
//   - entrypoints -> parent module hash + entrypoint name + stage
impl Clone     for SlangProgramKey { ... }
impl Copy      for SlangProgramKey { ... }
impl PartialEq for SlangProgramKey { ... }
impl Eq        for SlangProgramKey { ... }
impl Hash      for SlangProgramKey { ... }
```

when calculating the hash of a module, need to use reflection to iterate over all the files that were included as dependencies, and also calculate their hashes

```rust
// hash key calculated from:
// - the parent program's cache key
// - the entrypoint stage + name
impl Clone     for SpirvCodeKey { ... }
impl Copy      for SpirvCodeKey { ... }
impl PartialEq for SpirvCodeKey { ... }
impl Eq        for SpirvCodeKey { ... }

fn example() {
    let mut compiler = SlangCompilerBuilder::new()
        .capability("SPV_EXT_physical_storage_buffer")?
        .capability("SPV_EXT_descriptor_indexing")?
        .optimization(OptimizationLevel::High)
        .matrix_layout_row(true)
        .search_path("./shaders")
        .build()?;

    let module1 = compiler.load_module("./shaders/shader1.slang")?;
    let module2 = compiler.load_module("./shaders/shader2.slang")?;
    let module3 = compiler.load_module("./shaders/lib.slang")?;

    let program = compiler.linker()
        .add_all_entrypoints(&module1)
        .add_entrypoint(module2.entrypoint("main")?)
        .add_module(module3)
        .link()?;

    // slang compiler is not called internally until the `link()` call. until
    // that point, we're just building up the cache key for the current program,
    // and then checking if a cached version already exists or not. if one is
    // already cached, we can just return that instead

    program.layout(); // -> used to generate pipelines
    program.code();   // -> returns 4-byte aligned spir-v code

    // example of how this would be used from an engine with vulkan builder
    // functions
    let pipeline = {
        let mut shader_modules = vec![];
        for entrypoint in program.entrypoints() {
            let code = program.code(entrypoint);
            // -> VkShaderModule
            shader_modules = build_vulkan_shader_module(code)?;
        }
        // -> VkPipelineLayout + set layouts + push constants etc
        build_vulkan_pipeline(program.layout(), shader_modules)?
    };
}
```

## shader layout

TODO