use std::{collections::HashMap, sync::LazyLock};

use proc_macro2::{Ident, TokenStream as TokenStream2};
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use regex::Regex;
use syn::{
    braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token::Brace,
    Attribute,
    LitStr,
    Token,
    Type,
};

const TYPE_CONVERSION: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    HashMap::from([
        ("int8_t", "i8"),
        ("uint8_t", "u8"),
        ("int16_t", "i16"),
        ("uint16_t", "u16"),
        ("int32_t", "i32"),
        ("uint32_t", "u32"),
        ("int", "i32"),
        ("uint", "u32"),
        ("int64_t", "i64"),
        ("uint64_t", "u64"),
        ("float", "f32"),

        #[cfg(not(feature = "glam"))]
        ("float2", "[f32; 2]"),
        #[cfg(not(feature = "glam"))]
        ("float3", "[f32; 3]"),
        #[cfg(not(feature = "glam"))]
        ("float4", "[f32; 4]"),
        #[cfg(not(feature = "glam"))]
        ("float4x4", "[f32; 16]"),

        #[cfg(feature = "glam")]
        ("float2", "glam::Vec2"),
        #[cfg(feature = "glam")]
        ("float3", "glam::Vec3"),
        #[cfg(feature = "glam")]
        ("float4", "glam::Vec4"),
        #[cfg(feature = "glam")]
        ("float4x4", "glam::Mat4"),
    ])
});

struct SlangStructArray {
    slang_structs: Vec<SlangStruct>,
}

struct SlangStruct {
    attrs: Vec<Attribute>,
    _struct_token: Token![struct],
    name: Ident,
    _brace_token: Brace,
    fields: Punctuated<Field, Token![;]>,
}

struct Field {
    ty: Type,
    name: Ident,
    is_pointer: bool,
}

#[derive(Clone, Copy)]
enum LayoutKind {
    None,
    Std430,
    Std140,
}

impl Parse for SlangStructArray {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut slang_structs = Vec::<SlangStruct>::new();
        while input.peek(Token![struct]) || input.peek(Token![#]) {
            slang_structs.push(input.parse()?);
        }

        Ok(SlangStructArray { slang_structs })
    }
}

impl Parse for SlangStruct {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        Ok(SlangStruct {
            attrs: input.call(Attribute::parse_outer)?,
            _struct_token: input.parse()?,
            name: input.parse()?,
            _brace_token: braced!(content in input),
            fields: content.parse_terminated(Field::parse, Token![;])?,
        })
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Type = input.parse()?;
        let is_pointer = input.parse::<Option<Token![*]>>()?.is_some();

        Ok(Field {
            ty,
            name: input.parse()?,
            is_pointer,
        })
    }
}

fn inject_alignment_markers(contents: &str) -> String {
    let mut out = String::with_capacity(contents.len());
    let mut pending_std430 = false;
    let mut pending_std140 = false;

    for line in contents.lines() {
        let mut line_out = line.to_string();
        if let Some(comment_start) = line.find("//") {
            let comment = &line[comment_start..];
            if comment.contains("@std430") {
                pending_std430 = true;
            }
            if comment.contains("@std140") {
                pending_std140 = true;
            }

            line_out = line[..comment_start].trim_end().to_string();
        }

        let trimmed = line_out.trim_start();
        let is_struct = trimmed.starts_with("struct ");
        if is_struct && (pending_std430 || pending_std140) {
            if pending_std430 {
                out.push_str("#[std430]\n");
            }
            if pending_std140 {
                out.push_str("#[std140]\n");
            }
            pending_std430 = false;
            pending_std140 = false;
        }

        if !line_out.is_empty() {
            out.push_str(&line_out);
        }
        out.push('\n');
    }

    out
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

fn convert_type(ty: &Type, is_pointer: bool, layout: LayoutKind) -> Type {
    if is_pointer {
        let path = match layout {
            LayoutKind::Std430 => "crate::std_layout::Std430U64",
            LayoutKind::Std140 => "crate::std_layout::Std140U64",
            LayoutKind::None => "u64",
        };
        return syn::parse(path.parse().unwrap()).unwrap();
    }

    let mut str = ty.to_token_stream().to_string();
    for (key, value) in TYPE_CONVERSION.iter() {
        let mut r = String::from("([^a-zA-Z0-9]|^)");
        r.push_str(*key);
        r.push_str("([^a-zA-Z0-9]|$)");

        let regex = Regex::new(&r).unwrap();
        str = regex.replace_all(&str, *value).to_string();
    }

    syn::parse(str.parse().unwrap()).unwrap()
}

#[proc_macro]
pub fn slang_struct(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SlangStructArray);
    let SlangStructArray { slang_structs } = input;

    let mut ret = proc_macro2::TokenStream::new();
    for slang in slang_structs {
        let SlangStruct {
            attrs,
            _struct_token,
            name,
            _brace_token,
            fields,
        } = slang;

        let layout = if has_attr(&attrs, "std430") {
            LayoutKind::Std430
        } else if has_attr(&attrs, "std140") {
            LayoutKind::Std140
        } else {
            LayoutKind::None
        };

        let names: Vec<Ident> = fields.iter().map(|field| field.name.clone()).collect();
        let types: Vec<Type> = fields
            .iter()
            .map(|field| convert_type(&field.ty, field.is_pointer, layout))
            .collect();

        let mut derives: Vec<TokenStream2> = vec![quote!(Clone), quote!(Copy), quote!(Default)];
        if cfg!(feature = "bytemuck") && matches!(layout, LayoutKind::None) {
            derives.push(quote!(bytemuck::Pod));
            derives.push(quote!(bytemuck::Zeroable));
        }

        if matches!(layout, LayoutKind::Std430) {
            derives.push(quote!(crevice::std430::AsStd430));
        }
        if matches!(layout, LayoutKind::Std140) {
            derives.push(quote!(crevice::std140::AsStd140));
        }

        ret.extend(quote!(#[repr(C)]));
        ret.extend(quote!(#[derive(#(#derives),*)]));

        ret.extend(quote! {
            pub struct #name {
                #(#names: #types),*
            }
        });
    }

    ret.into()
}

#[proc_macro]
pub fn slang_include(input: TokenStream) -> TokenStream {
    let string = parse_macro_input!(input as LitStr).value();
    let file_contents = std::fs::read_to_string(string.as_str()).unwrap();
    let file_contents = inject_alignment_markers(&file_contents);

    slang_struct(file_contents.parse().unwrap())
}
