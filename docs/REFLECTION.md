# Slang reflection notes (voxels2)

This document explains how the `crates/slang` reflection dump works and how to
interpret its output. The goal is to make field-level sizes/offsets/bindings
obvious for push constants, descriptor sets, and other struct types.

## What the tool prints

The `slang` tool loads a shader module, links entry points, and queries Slang's
reflection API to print:

- Entry points and stages.
- Global parameters and their layouts (including push constants).
- Descriptor set ranges (per scope).
- Entry-point parameter layouts (including stage inputs/outputs).

The reflection traversal uses:

- `global_params_var_layout()` for the global scope.
- `entry_point.var_layout()` for entry-point scopes.

These are recommended by the Slang docs because they account for any implicit
wrapping (e.g., constant buffers or parameter blocks) that can change offsets.

## Layout units (why `size=1`)

Slang reports sizes/offsets in **layout units**. A layout unit is represented
by `ParameterCategory`, and there can be multiple units in play at once:

- `Uniform` (bytes)
- `PushConstantBuffer` (push-constant bindings)
- `DescriptorTableSlot` (descriptor slots/bindings)
- Other resource categories

So `size=1` does **not** mean 1 byte. It means "size 1 in this layout unit."

Example: a `ConstantBuffer<PushConstants>` parameter might show:

```
layout[PushConstantBuffer]: size=1 stride=1 align=1
```

That means the *container* consumes one push-constant binding. The byte size of
the data is found in the **element layout** under `layout[Uniform]`, e.g.:

```
type_layout: name=PushConstants kind=Struct parameter_category=Uniform
  layout[Uniform]: size=96 stride=96 align=16
```

When you see `size=1`, always check which `layout[...]` unit it is tied to.

## Container vs element (single-element containers)

Types like `ConstantBuffer<T>` or `ParameterBlock<T>` behave like single-element
containers. Slang exposes:

- `container_offsets`: where the container binding lives
- `element_offsets`: where the element `T` lives relative to the container
- `type_layout` for the element type, which contains field offsets/sizes

This is why `PushConstants` fields show up under:

```
param[0] -> ConstantBuffer<PushConstants>
  container_offsets: ...
  element_offsets: ...
  type_layout: PushConstants
    fields: ...
```

## How to read the output

- `param[...]` lines show the top-level parameters (global scope).
- `parameter_layouts` expands each parameter with type layout and field details.
- `global_params_layout` shows the global scope as a grouped struct, which may
  include implicit container wrapping.
- `entry_layout[...]` shows entry-point scopes (inputs/outputs) and their
  layouts.
- `fields` under a struct show per-field offsets/sizes/alignments for each
  layout unit.

If a type is only referenced by pointer (e.g., `Vertex*` inside a struct),
Slang will not automatically expand the pointee type. If you need those fields,
look up the type explicitly by name via `reflection.find_type_by_name("Vertex")`
and query its layout directly.

## Related docs

See `~/docs/slang-docs-md/09-reflection.md` for the authoritative reflection
model, especially:

- "Layout Units"
- "Single-Element Containers"
- "Container and Element"
