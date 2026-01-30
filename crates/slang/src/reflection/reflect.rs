use anyhow::{Context, Ok, Result};
use shader_slang as slang;

use crate::reflection::print::PrintObject;

struct State {
    print: PrintObject,
    base_set: usize,
}

impl State {
    fn new(print: PrintObject) -> Self {
        Self { print, base_set: 0 }
    }

    fn increment_set(&mut self, value: usize) {
        self.base_set += value;
    }

    fn clone(&self, print: PrintObject) -> Self {
        Self {
            print,
            base_set: self.base_set,
        }
    }
}

pub fn walk_program(program: &slang::ComponentType) -> Result<()> {
    let program_layout = program.layout(0)?;
    let global_vars = program_layout
        .global_params_var_layout()
        .context("global_params_var_layout failed")?;

    // TODO
    // let layout = ShaderLayout {
    //     push_constants: vec![],
    //     descriptor_sets: vec![],
    //     globals: vec![],
    //     entrypoints: vec![],
    // };

    let print = PrintObject::new(0).object("globals");
    let state = State::new(print);
    print_var_layout(state, global_vars)?;

    Ok(())
}

fn print_var_layout(
    mut state: State,
    var_layout: &slang::reflection::VariableLayout,
) -> Result<()> {
    if let Some(name) = var_layout.name() {
        state.print.value("name", name);
    }

    let categories = var_layout.categories();
    if categories.len() > 0 {
        let print_array = state.print.array("offset");
        for category in categories {
            let value = var_layout.offset(category);
            let unit = format!("{:?}", category);

            if category == slang::ParameterCategory::SubElementRegisterSpace {
                state.increment_set(value);
            }

            let mut print_object = print_array.object();
            print_object.value("value", &value.to_string());
            print_object.value("unit", &unit);
        }
    }

    if let Some(type_layout) = var_layout.type_layout() {
        print_type_layout(state, type_layout, false)?;
    }

    Ok(())
}

fn print_type_layout(
    state: State,
    type_layout: &slang::reflection::TypeLayout,
    is_parameter_block: bool,
) -> Result<()> {
    let mut print = state.print.object("layout_type");

    let kind = type_layout.kind();
    print.value("kind", &format!("{:?}", kind));

    if kind == slang::TypeKind::Struct {
        if let Some(name) = type_layout.name() {
            print.value("name", name);
        }
    }

    let categories = type_layout.categories();
    if categories.len() > 0 {
        let print_array = print.array("size");
        for category in categories {
            let value = type_layout.size(category);
            let unit = format!("{:?}", category);
            let mut print_object = print_array.object();
            print_object.value("value", &value.to_string());
            print_object.value("unit", &unit);
        }
    }

    let alignment = type_layout.alignment(slang::ParameterCategory::Uniform);
    let stride = type_layout.stride(slang::ParameterCategory::Uniform);
    if alignment != 1 || stride != 0 {
        print.value("alignment", &alignment.to_string());
        print.value("stride", &stride.to_string());
    }

    if is_parameter_block {
        let has_implicit_uniform_binding = type_layout.size(slang::ParameterCategory::Uniform) > 0;
        let mut emitted_uniform_binding = false;

        let print = print.array("descriptors");

        let binding_range_count = type_layout.binding_range_count();
        for br in 0..binding_range_count {
            let dr_count = type_layout.binding_range_descriptor_range_count(br);
            if dr_count == 0 {
                continue;
            }

            let set_index = type_layout.binding_range_descriptor_set_index(br);
            let vk_set = state.base_set as i64 + type_layout.descriptor_set_space_offset(set_index);

            if has_implicit_uniform_binding && !emitted_uniform_binding {
                let mut print = print.object();
                print.value("set", &vk_set.to_string());
                print.value("binding", "0");
                emitted_uniform_binding = true;
            }

            let first = type_layout.binding_range_first_descriptor_range_index(br);
            for i in 0..dr_count {
                let range_index = first + i;
                let vk_binding = type_layout
                    .descriptor_set_descriptor_range_index_offset(set_index, range_index);

                let vk_binding_type =
                    type_layout.descriptor_set_descriptor_range_type(set_index, range_index);

                let vk_binding_count = type_layout
                    .descriptor_set_descriptor_range_descriptor_count(set_index, range_index);

                // if vk_binding_count < 0 -> runtime sized

                let mut print = print.object();
                print.value("set", &vk_set.to_string());
                print.value("binding", &vk_binding.to_string());
                print.value("type", &format!("{:?}", vk_binding_type));
                print.value("count", &vk_binding_count.to_string());
            }
        }
    }

    match type_layout.kind() {
        slang::TypeKind::Struct => {
            let fields = type_layout.fields();
            let print = print.array("fields");
            for field in fields {
                let print = print.object();
                let next_state = state.clone(print);
                print_var_layout(next_state, field)?;
            }
        }
        slang::TypeKind::ParameterBlock => {
            if let Some(element_type) = type_layout.element_type_layout() {
                let print = print.object("element layout");
                print_type_layout(state.clone(print), element_type, true)?;
            }
        }
        slang::TypeKind::ConstantBuffer => {
            if let Some(element_type) = type_layout.element_type_layout() {
                let print = print.object("element layout");
                print_type_layout(state.clone(print), element_type, false)?;
            }
        }
        _ => {}
    }

    Ok(())
}
