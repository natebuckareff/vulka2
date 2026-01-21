# Slang bindless notes (DescriptorHandle<T>)

This doc captures what we learned while testing Slang bindless handles in this repo.
It focuses on SPIR-V/Vulkan behavior.

## Key takeaways

- `DescriptorHandle<T>` (or `Texture2D.Handle`) is plain data in SPIR-V: a `uint2`.
- The handle value is an index into Slang's implicit global bindless descriptor array (heap).
- The implicit heap is *compiler-generated* and is not reported by the Slang reflection API as a normal parameter.
- The heap *does* exist in SPIR-V and has a fixed descriptor set + binding based on bindless options.
- You must set `-bindless-space-index` (or `CompilerOptionName::BindlessSpaceIndex`) for Vulkan/SPRIV to emit the heap.

## Reflection expectations

Observed behavior with `DescriptorHandle<T>` used from push constants:

- Reflection shows `pushConstants` as a `PushConstantBuffer` and includes the handle as a
  field inside the push-constant struct (category `Uniform`, offset reflected).
- Reflection does **not** surface the implicit global bindless descriptor array as
  a parameter, descriptor set, or binding range.
- The implicit heap is visible in SPIR-V as `__slang_resource_heap` with
  `DescriptorSet = <bindless-space-index>` and a descriptor-type-specific binding index.

Implication: for pipeline layout generation, you must synthesize a bindless heap
binding when any `DescriptorHandle<T>` is used, or parse SPIR-V to confirm.

## CPU-side representation of DescriptorHandle<T>

From Slang docs (SPIR-V target):
- `DescriptorHandle<T>` lowers to a `uint2` (8 bytes).
- The handle is *not* `{set,binding}`. It is an index into the global bindless heap.

Usage of x/y components:
- For resource-only handles (e.g. `Texture2D`), only `.x` is used as the index.
- For combined types (e.g. `Sampler2D`), `.x` indexes the resource heap and `.y`
  indexes the sampler heap (split-resource model).

So on the CPU side, treat `DescriptorHandle<T>` as:
- `uint2` (two `u32`), or
- `u64` containing the packed index.

## Implicit bindless heap bindings

From docs:
- When targeting SPIR-V, Slang introduces a global descriptor array for bindless access.
- The descriptor set is controlled by `-bindless-space-index`.
- The *binding index* depends on descriptor type and the bindless option preset.

Slang defaults to `BindlessDescriptorOptions::VkMutable`.

### VkMutable (default) binding map

This is from `vkmutablebindlessbindings` in the docs mirror:

| Descriptor kind            | Vulkan descriptor type                    | Binding index |
|---------------------------|-------------------------------------------|---------------|
| Sampler                   | VK_DESCRIPTOR_TYPE_SAMPLER                | 0             |
| CombinedTextureSampler    | VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER | 1             |
| SampledImage              | VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE          | 2             |
| StorageImage              | VK_DESCRIPTOR_TYPE_STORAGE_IMAGE          | 2             |
| UniformTexelBuffer        | VK_DESCRIPTOR_TYPE_UNIFORM_TEXEL_BUFFER   | 2             |
| StorageTexelBuffer        | VK_DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER   | 2             |
| ConstantBuffer_Read       | VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER         | 2             |
| StorageBuffer_Read        | VK_DESCRIPTOR_TYPE_STORAGE_BUFFER         | 2             |
| StorageBuffer_ReadWrite   | VK_DESCRIPTOR_TYPE_STORAGE_BUFFER         | 2             |
| Unknown                   | Other                                     | 3             |

For `Texture2D` handles, the descriptor kind is `SampledImage`, so binding index = 2.
This matches SPIR-V we disassembled: `__slang_resource_heap` at set 10, binding 2.

### Default (BindlessDescriptorOptions::None) binding map

From `defaultvkbindlessbindings` in the docs mirror (used when overriding options):

| Descriptor kind            | Vulkan descriptor type                    | Binding index |
|---------------------------|-------------------------------------------|---------------|
| Sampler                   | VK_DESCRIPTOR_TYPE_SAMPLER                | 0             |
| CombinedTextureSampler    | VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER | 1             |
| SampledImage              | VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE          | 2             |
| StorageImage              | VK_DESCRIPTOR_TYPE_STORAGE_IMAGE          | 3             |
| UniformTexelBuffer        | VK_DESCRIPTOR_TYPE_UNIFORM_TEXEL_BUFFER   | 4             |
| StorageTexelBuffer        | VK_DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER   | 5             |
| ConstantBuffer_Read       | VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER         | 6             |
| StorageBuffer_Read        | VK_DESCRIPTOR_TYPE_STORAGE_BUFFER         | 7             |
| StorageBuffer_ReadWrite   | VK_DESCRIPTOR_TYPE_STORAGE_BUFFER         | 7             |
| Unknown                   | Other                                     | 8             |

## SPIR-V evidence

When bindless is enabled, SPIR-V contains a global runtime array:

- `OpDecorate %__slang_resource_heap DescriptorSet <bindless-space-index>`
- `OpDecorate %__slang_resource_heap Binding <binding index>`

This is the authoritative source if reflection does not expose the heap.

## References (local docs mirror)

- `~/docs/slang-docs-md/03-convenience-features.md`
  - `DescriptorHandle` explanation
  - bindless heap behavior and `-bindless-space-index`
  - `BindlessDescriptorOptions::VkMutable` (default)
- `~/docs/slang-docs-md/index.e49449dd1c.md`
  - `VkMutableBindlessBindings` enum (default binding indices)
- `~/docs/slang-docs-md/index.fde06eae35.md`
  - `DefaultVkBindlessBindings` enum (when using BindlessDescriptorOptions::None)
- `~/docs/slang-docs-md/command-line-slangc-reference.md` (search for `-bindless-space-index`)

## Notes for this repo

- Our Slang reflection output does *not* show the implicit bindless heap.
- SPIR-V disassembly does show the heap with the expected set/binding.
- For pipeline layout metadata:
  - Add the bindless heap set/binding when any `DescriptorHandle<T>` is used.
  - Use the binding index table above for descriptor type mapping.
  - Or parse SPIR-V for `__slang_resource_heap` to get exact values.

## Non-uniform handles

By default, Slang assumes a `DescriptorHandle<T>` is *dynamically uniform* across
all lanes. If the handle varies per-lane, you must mark it `nonuniform` *right
before* dereferencing it (sampling, load, etc.). See the Slang docs for
`nonuniform()`.

Example:

```
DescriptorHandle<Texture2D> h = ...; // potentially varying
Texture2D tex = nonuniform(h);
float4 c = tex.Sample(sampler, uv);
```

## Overriding bindless mapping

Slang allows you to override the default bindless mapping by providing your own
`getDescriptorFromHandle()` implementation. This can be used to split sampler
and resource heaps into different sets or bindings.

The default SPIR-V behavior uses `BindlessDescriptorOptions::VkMutable` unless
explicitly changed, which influences the binding indices in the table above.

See `~/docs/slang-docs-md/03-convenience-features.md` for the reference code
snippet and details.

## Compiler option placement

`BindlessSpaceIndex` is a global/session option. Apply it via `SessionDesc::options`
(in addition to target options) to ensure Slang honors the value. This matched
our observations with the Rust binding.

## SPIR-V verification tip

If reflection doesn’t show the heap, disassemble SPIR-V and look for:

- `OpName %__slang_resource_heap "__slang_resource_heap"`
- `OpDecorate %__slang_resource_heap DescriptorSet <N>`
- `OpDecorate %__slang_resource_heap Binding <M>`

This is the authoritative source for the implicit bindless heap binding.
