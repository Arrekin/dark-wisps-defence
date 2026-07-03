use proc_macro::TokenStream;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, FieldsUnnamed, parse_macro_input};

/// Derives the SSS trait (Send + Sync + 'static) for structs and enums.
///
/// # Example
/// ```rust
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
/// For unnamed-field structs, the generated methods access `self.0` and construct
/// with `Self(value)`. For named-field structs, they access the single field by
/// name and construct with `Self { field_name: value }`.
/// 
/// The derive only supports structs with exactly one field. Enums and multi-field
/// structs are rejected.
///
/// # Example
/// ```rust
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
        Err(err) => return err.to_compile_error().into(),
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

