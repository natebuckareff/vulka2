# Vulkan Pipeline Optimizer Plan (Slang + Vulkan 1.3)

This document summarizes the current plan for a Vulkan pipeline/layout optimizer built around **Slang reflection** and a **draw DAG**, and incorporates additional Vulkan-specific notes/pitfalls that affect correctness and pipeline count.

---

## Goals

* Treat a *shader invocation* as the combination of:

  * **Slang-compiled shader module** (SPIR-V, with entry points per stage)
  * **fixed-function pipeline state** (raster/depth/blend/etc)
  * **pipeline layout** (descriptor sets + push constants)
  * **dynamic rendering attachment *formats*** (external to shader)
* From a stream/DAG of draw operations, infer:

  * the **minimum set of Vulkan objects** required (`VkPipeline`, `VkPipelineLayout`, `VkDescriptorSetLayout`)
  * the **binding plan** (when to `vkCmdBindPipeline`, `vkCmdBindDescriptorSets`, push constants, vertex buffers)
* Keep the system cache-friendly (stable hashing, minimal layout permutations).

---

## High-level mental model

* In Vulkan, the *executable-like object* is the **pipeline** (`VkPipeline`).
* **Shader modules** are inputs to pipeline creation; they are not standalone linked programs.
* The **pipeline layout** defines how descriptors/push constants are bound.
* With **dynamic rendering**, you avoid render passes/framebuffers, but pipelines still depend on the **attachment formats** used for rendering.

---

## Core assumptions

* **Vulkan 1.3 minimum**
* **Dynamic rendering** (no classic `VkRenderPass`/`VkFramebuffer`)
* **Descriptor indexing + bindless**

  * Use Slang `DescriptorHandle<T>` + `-bindless-space-index`
  * Prefer “few stable descriptor sets” over per-shader layouts
* **Buffer device address (BDA)** for most buffers (avoid descriptors where reasonable)
* Prefer **dynamic state** where supported to reduce pipeline variants

---

## Planned work: `crates/slang`

### Reflection + inference targets

* Infer push constant ranges from shader
* Infer all descriptor sets and bindings from shader
* Infer vertex attribute layouts from shader (VS stage)
* Infer render target attachment **layout** from shader (*outputs only, no formats*)
* Create inferred `VkDescriptorSetLayout` definitions
* Create inferred `VkPipelineLayout` definitions
* Create inferred `VkVertexInputBindingDescription` / attribute descriptions
* Create inferred “render attachment layout” (**sans formats**)
* Track std140 vs std430 byte layouts
* **Specialization constants** (planned; see below)

### `SlangLayout`

Create a root-level `SlangLayout` type that can be queried for:

* Per-stage layouts (VS/FS/CS/etc)
* Shader-global layouts (descriptor sets, push constants)
* Reflection-driven metadata needed for hashing/caching

All layout-ish types must implement **`Eq` + `Hash`** and be deterministic.

---

## Procedural macros + generated structs

### Intended user-facing ergonomics

Proc macros to generate Rust structs for:

* Push constants
* Uniform buffers
* Vertex structs (assume interleaved vertex format)

Provide `bytemuck` helpers:

* Convert slices of generated structs to bytes (`&[T] -> &[u8]`)
* Make push constant packing and buffer uploads easy/cheap

### Open question

> How do we generate structs via macros for vertices, push constants, uniforms, etc?

One idea:

* An `include_shader!()`-style macro:

  * reads shader source at compile time
  * compiles with Slang (or loads precompiled artifact)
  * emits a `'static SlangShader` + generated Rust bindings (types + layout metadata)

This approach should be designed with **reproducible builds** in mind (e.g., caching compiled output or requiring precompiled SPIR-V for release builds).

---

## `SlangShader` output type

The output of a compiled, reflected, inferred Slang shader module should be:

```rust
struct SlangShader { /* ... */ }

impl SlangShader {
    pub fn layout(&self) -> &SlangLayout { /* ... */ }
}
```

The user can then construct:

* Descriptor set layouts
* Pipeline layouts
* Vertex input descriptions
* Attachment layout declarations (locations, count, types) from the shader

---

## Draw operations + the DAG

### What a draw operation needs to capture

* Which pipeline “shader+state combo” it requires
* What resources it binds:

  * descriptor set(s) / bindless table handles
  * push constant bytes
  * vertex/index buffers
* External render graph information:

  * which color/depth attachments are used **and their formats**
  * load/store ops and transitions (tracked by render graph)

### DAG / scheduling intent

* A draw DAG expresses ordering constraints:

  * resource hazards (write→read, write→write)
  * subpass-like dependencies (e.g., depth prepass then shading)
  * blending/transparent ordering constraints
* A scheduler can then reorder draws **within legal freedom** to reduce:

  * `vkCmdBindPipeline` count
  * `vkCmdBindDescriptorSets` count
  * push constant updates
  * vertex buffer rebinding

---

## Important Vulkan notes to incorporate

### 1) Attachment formats cannot be inferred from the shader

Shaders can tell you:

* which color outputs exist (locations)
* whether depth output is written

But shader reflection **cannot** infer:

* the actual `VkFormat` of color/depth attachments
* MSAA sample count

With dynamic rendering, graphics pipelines still bake in attachment formats via `VkPipelineRenderingCreateInfo`.

**Action:** include rendering formats/sample count in the pipeline cache key.

---

### 2) Pipeline layouts: prefer stability and compatibility

Generating a unique `VkPipelineLayout` for every shader *works*, but tends to:

* increase pipeline permutations
* force frequent descriptor rebinding when switching pipelines

**Recommendation:** design a small number of “layout tiers”, e.g.:

* **GraphicsLayout**:

  * set0 = bindless tables
  * set1 = material/per-draw params (optional)
  * push constants for fast indices/params
* **ComputeLayout**:

  * similar, but compute-focused set1/push layout

Then map shaders into those layouts even if some bindings are unused.

---

### 3) Vertex input reflection may increase pipeline variants

In core Vulkan, vertex input state is part of the pipeline.

If vertex formats vary widely, “infer vertex layout per mesh” can cause **pipeline explosion**.

Mitigation options:

* standardize vertex formats
* keep a small number of vertex layouts
* use dynamic vertex input state if available/allowed on target GPUs (extension-dependent)

---

### 4) Specialization constants must be part of pipeline keys (when used)

A **specialization constant** is a shader constant whose value is supplied at **pipeline creation time**.

Changing specialization values => *different `VkPipeline`*, even with the same SPIR-V.

**Action items:**

* reflect specialization constants from Slang (`[SpecializationConstant]` / `vk::constant_id`)
* include specialization values (or a hash blob) in the pipeline cache key
* treat them as “shader variants without recompilation”

---

### 5) Descriptor indexing flags/features matter

If the engine relies on:

* runtime arrays
* partially-bound descriptors
* update-after-bind
* variable descriptor count

then those behaviors must be included in the set layout construction and enabled at device creation.

**Action:** treat descriptor-layout flags + relevant feature toggles as part of layout identity/hashing.

---

## Pipeline cache key design (suggested)

A minimal-but-correct graphics pipeline key should include:

### Shader identity

* per-stage: `(shaderModuleId, entryPointName)`
* (optional) per-stage SPIR-V hash for safety

### Pipeline layout identity

* hash/ID of the `VkPipelineLayout` (descriptor set layouts + push constant ranges)

### Rendering identity (dynamic rendering)

* `colorAttachmentCount`
* array of `colorAttachmentFormats[i]`
* `depthAttachmentFormat` / `stencilAttachmentFormat`
* MSAA sample count

### Fixed-function state identity

* input assembly topology
* rasterization state (cull mode, front face, polygon mode)
* depth/stencil state (compare op, write enable, etc)
* blend state (per attachment)
* sample state, alpha-to-coverage, etc
* vertex input state (bindings/attributes), unless made dynamic

### Specialization constants (per stage)

* hash of specialization constant values blob (or canonicalized map from `constant_id -> bytes`)

---

## Proposed implementation steps

### Phase 1: Deterministic reflection + hashing

* Implement `SlangLayout` types that are stable and `Hash`able
* Encode descriptor sets, push constants, vertex inputs, shader outputs
* Serialize to a canonical form (for hashing + debugging)

### Phase 2: Vulkan object builders

* `SlangLayout -> VkDescriptorSetLayoutCreateInfo`
* `SlangLayout -> VkPipelineLayoutCreateInfo`
* `SlangLayout + fixed state + rendering formats -> VkGraphicsPipelineCreateInfo`

### Phase 3: Pipeline + layout caching

* Global caches:

  * `DescriptorSetLayoutCache`
  * `PipelineLayoutCache`
  * `GraphicsPipelineCache`
  * `ComputePipelineCache`
* Lazy creation on first use

### Phase 4: Draw DAG scheduling / binding minimization

* Define draw nodes with:

  * pipeline key
  * resource bindings
  * attachment formats
  * dependency edges
* Topologically sort with heuristics to reduce state changes

### Phase 5: Specialization constants + variants

* Reflect specialization constants
* Add specialization blob hashing to pipeline keys
* Provide user-level API for supplying specialization values

---

## Debugging / validation recommendations

* Turn on validation layers early (especially descriptor indexing + dynamic rendering)
* Emit debug labels that include:

  * pipeline key hash
  * layout hash
  * shader entrypoints
  * attachment formats
* Log pipeline cache misses with a clear “why new pipeline?” diff

---

## Notes / open questions

* How aggressively should layouts be unified across shaders (one “global” layout vs per-material layouts)?
* How should vertex layout inference interact with mesh formats (standardization strategy)?
* Do we want to support optional “extra bindings” in layouts for compatibility?
* How do we handle multi-draw batching (same pipeline + varying push constants/handles)?
* What is the fallback when Slang reflection cannot fully infer a layout (manual override)?

---

