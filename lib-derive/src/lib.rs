use proc_macro::TokenStream;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, FieldsUnnamed, parse_macro_input};

mod log_tags;

/// Derives the SSS trait (Send + Sync + 'static) for structs and enums.
///
/// # Example
/// ```ignore
/// #[derive(SSS)]
/// struct MyComponent;
///
/// // Generates:
/// // impl SSS for MyComponent {}
/// ```
#[proc_macro_derive(SSS)]
pub fn derive_sss(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics SSS for #name #ty_generics #where_clause {}
    };

    TokenStream::from(expanded)
}

/// Derives the Property trait for structs containing a single f32 field.
///
/// Supported shapes:
/// - structs with exactly one unnamed field, e.g. `struct Temperature(f32);`
/// - structs with exactly one named field, e.g. `struct Temperature { value: f32 }`
///
/// The derived implementation generates the full `Property` API:
/// - `fn get(&self) -> f32`
/// - `fn set(&mut self, value: f32)`
/// - `fn new(value: f32) -> Self`
///
/// # Example
/// ```ignore
/// #[derive(Property)]
/// struct Temperature(f32);
///
/// // Generates:
/// // impl Property for Temperature {
/// //     fn get(&self) -> f32 { self.0 }
/// //     fn set(&mut self, value: f32) { self.0 = value }
/// //     fn new(value: f32) -> Self { Self(value) }
/// // }
/// ```
#[proc_macro_derive(Property)]
pub fn derive_property(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Generate the implementation based on the struct's fields
    let property_impl = match generate_property_impl(&input.data) {
        Ok(impl_block) => impl_block,
        Err(error) => return error.to_compile_error().into(),
    };

    let expanded = quote! {
        impl #impl_generics Property for #name #ty_generics #where_clause {
            #property_impl
        }
    };

    TokenStream::from(expanded)
}

fn generate_property_impl(data: &Data) -> syn::Result<TokenStream2> {
    match data {
        Data::Struct(data_struct) => {
            match &data_struct.fields {
                Fields::Unnamed(FieldsUnnamed { unnamed, .. }) if unnamed.len() == 1 => {
                    // Single unnamed field (newtype pattern)
                    Ok(quote! {
                        fn get(&self) -> f32 {
                            self.0
                        }

                        fn set(&mut self, value: f32) {
                            self.0 = value;
                        }

                        fn new(value: f32) -> Self {
                            Self(value)
                        }
                    })
                }
                Fields::Named(fields) if fields.named.len() == 1 => {
                    // Single named field
                    let field = fields.named.first().unwrap();
                    let field_name = field.ident.as_ref().unwrap();
                    Ok(quote! {
                        fn get(&self) -> f32 {
                            self.#field_name
                        }

                        fn set(&mut self, value: f32) {
                            self.#field_name = value;
                        }

                        fn new(value: f32) -> Self {
                            Self { #field_name: value }
                        }
                    })
                }
                _ => Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "Property derive only supports structs with exactly one field"
                ))
            }
        }
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "Property derive only supports structs"
        ))
    }
}

/// Derives `MomentKind` for a moment marker struct, inferring the `KIND`
/// persistence tag from the type name. The type must start with `Moment`;
/// the prefix is stripped and the remainder is converted to snake_case.
///
/// # Example
/// ```ignore
/// #[derive(Component, Default, MomentKind)]
/// pub struct MomentGameStart;
///
/// // Generates:
/// // impl MomentKind for MomentGameStart {
/// //     const KIND: &'static str = "game_start";
/// // }
/// ```
#[proc_macro_derive(MomentKind)]
pub fn derive_moment_kind(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let kind = match moment_kind_from_ident(name) {
        Ok(kind) => kind,
        Err(error) => return error.to_compile_error().into(),
    };

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics MomentKind for #name #ty_generics #where_clause {
            const KIND: &'static str = #kind;
        }
    };

    TokenStream::from(expanded)
}

/// Strip the `Moment` prefix and snake_case the rest: `MomentGameStart` →
/// `game_start`, `MomentObjectiveSatisfied` → `objective_satisfied`.
fn moment_kind_from_ident(ident: &syn::Ident) -> syn::Result<String> {
    let name = ident.to_string();
    let rest = name.strip_prefix("Moment").ok_or_else(|| {
        syn::Error::new(
            ident.span(),
            "MomentKind derive requires the type name to start with `Moment`",
        )
    })?;
    Ok(to_snake_case(rest))
}

/// Convert `PascalCase` to `snake_case`.
fn to_snake_case(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + input.len() / 2);
    for (index, character) in input.chars().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.extend(character.to_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

/// Derives `From<Entity>` for a struct with a single `entity: Entity` field.
/// Enables `entity_commands.trigger(Event::from)` instead of
/// `entity_commands.trigger(|e| Event { entity: e })`.
#[proc_macro_derive(FromEntity)]
pub fn derive_from_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics From<bevy::prelude::Entity> for #name #ty_generics #where_clause {
            fn from(entity: bevy::prelude::Entity) -> Self {
                Self { entity }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Sets the log tags for a function and expands the log annotations in its body.
///
/// Annotations are named `<level>_<audience>` — level `debug`/`info`/`warn`/`error`,
/// audience `dev`/`player` — and take `format!` arguments. Every log in the function
/// carries the tags given to `#[log_tags]`. Removing the annotations leaves plain Rust
/// that behaves the same apart from logging.
///
/// # Example
/// ```ignore
/// #[log_tags(Tag::MapObjects)]
/// fn spawn_foos(mut commands: Commands, requests: &[FooRequest], anchors: &HashMap<u32, Entity>) {
///     for request in requests {
///         #[warn_dev("Foo request {} has no anchor", request.id)]
///         let Some(&anchor) = anchors.get(&request.id) else { continue };
///
///         let kind = match request.kind.as_str() {
///             "Small" => FooKind::Small,
///             #[warn_dev("Unknown foo kind: {other}")]
///             other => continue,
///         };
///
///         #[debug_dev("Spawned foo on anchor {anchor}")]
///         commands.spawn((Foo(kind), ChildOf(anchor)));
///     }
/// }
/// ```
///
/// # Expansions
///
/// An annotation logs when its code runs to completion:
///
/// - a statement without a block logs after it has run;
/// - a statement with a block logs when that block reaches its end.
///
/// Any early exit before that point — `?`, `return`, `continue`, `break` — leaves without
/// logging. The same rule covers actions and refusals: in a guard (`let-else`, `if !paid
/// { return None; }`) the block is the refusal, so the log reports the refusal.
///
/// A log in a block goes at its end, before its closing exit, so bindings made inside the
/// block are in scope. A value the exit carries — `return x?`, `break 'label x` — is
/// evaluated before the log, so it logs only if that value was produced. `LOG` below stands for
/// `logging::Log::<level>().<audience>().tags([..]).message(format_args!(..))`.
///
/// ```ignore
/// // let-else → end of the else block, before the exit
/// #[warn_dev("..")] let Some(x) = a else { continue };
/// let Some(x) = a else { LOG; continue };
///
/// // if without else → end of the if block, before the exit
/// #[info_player("..")] if !paid { return None; }
/// if !paid { let value = None; LOG; return value; }
///
/// // ... and bindings made inside the branch are usable in the message
/// #[info_player("Finished {item}")] if done { let item = take(); store(item); }
/// if done { let item = take(); store(item); LOG; }
///
/// // match arm → inside that arm
/// #[warn_dev("Unknown kind: {other}")] other => None,
/// other => { let value = None; LOG; value }
///
/// // a branch ending in a value → value evaluated first, logged, then yielded
/// => { ..; compute() }
/// => { ..; let value = compute(); LOG; value }
///
/// // continue / break / return without a value → before the exit
/// #[warn_dev("..")] continue;
/// LOG; continue;
///
/// // return / break with a value → value evaluated first, logged, then exited with
/// #[warn_dev("..")] return fetch()?;
/// let value = fetch()?; LOG; return value;
///
/// // anything else → after the statement; bindings it makes are in scope
/// #[debug_dev("Saving {}", rows.len())] let rows = collect();
/// let rows = collect(); LOG;
///
/// // `?` is not a block: the log fires once the statement succeeds, a failure exits silently.
/// // To log the failure, give it a block: `let Ok(x) = a else { .. }` or a match arm.
/// #[info_dev("Loaded {}", rows.len())] let rows = load()?;
/// let rows = load()?; LOG;
///
/// // escape hatch: a log at a point no statement shape expresses
/// info_dev!("..");
/// LOG;
/// ```
///
/// Rejected at compile time: an annotated `if` with an `else` and an annotated whole
/// `match` (both have more than one branch — annotate the branch or arm instead), more than
/// one annotation on a statement, and an annotation without a message. The `!` forms fail
/// to compile outside a `#[log_tags]` function.
#[proc_macro_attribute]
pub fn log_tags(attribute: TokenStream, item: TokenStream) -> TokenStream {
    log_tags::expand(attribute.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
