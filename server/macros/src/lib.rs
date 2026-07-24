//! Procedural macros for `odoo_ls_server`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Ident, Path, Type, parse_macro_input, parse_quote};

/// Derives `From<K> for Self` for every variant's K type.
///
/// ```ignore
/// #[derive(From)]
/// pub enum SourceFileKey {
///     File(FileKey),        // From<FileKey> for SourceFileKey
///     CsvFile(CsvFileKey),  // From<CsvFileKey> for SourceFileKey
/// }
/// ```
#[proc_macro_derive(From)]
pub fn derive_from_inner(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(expand_from_inner(&input))
}

fn expand_from_inner(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let variants = inner_types(input)?;
    let name = &input.ident;

    let impls = variants.iter().map(|(variant, inner)| {
        quote! {
            impl ::core::convert::From<#inner> for #name {
                fn from(inner: #inner) -> Self {
                    #name::#variant(inner)
                }
            }
        }
    });
    Ok(quote!(#(#impls)*))
}

/// Derives `From<Self> for SymbolKey`.
///
/// Requires every inner type to implement `Into<SymbolKey>`.
///
/// ```ignore
/// #[derive(IntoSymbolKey)]
/// pub enum SourceFileKey {
///     File(FileKey),
///     CsvFile(CsvFileKey),
/// }
/// ```
/// Equivalent to `#[derive(IntoKey)] #[into_key(SymbolKey)]`.
#[proc_macro_derive(IntoSymbolKey)]
pub fn derive_into_symbol_key(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(expand_into(&input, &[symbol_key_path()]))
}

/// Derives `From<Self> for Target`, for each Target named by the `into_key` attribute.
///
/// ```ignore
/// #[derive(IntoKey)]
/// #[into_key(XmlId)]
/// pub enum XmlDataKey { .. }   // From<XmlDataKey> for XmlId
/// ```
///
/// Requires every inner type to implement `Into<Target>`.
/// 
/// Takes several targets, e.g. `#[into_key(A, B)]`
#[proc_macro_derive(IntoKey, attributes(into_key))]
pub fn derive_into_key(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(into_key_targets(&input).and_then(|targets| expand_into(&input, &targets)))
}

fn expand_into(input: &DeriveInput, targets: &[Path]) -> syn::Result<TokenStream2> {
    let variants: Vec<&Ident> = inner_types(input)?.into_iter().map(|(v, _)| v).collect();
    let name = &input.ident;

    let impls = targets.iter().map(|target| {
        quote! {
            impl ::core::convert::From<#name> for #target {
                fn from(value: #name) -> Self {
                    match value {
                        #( #name::#variants(inner) => inner.into(), )*
                    }
                }
            }
        }
    });
    Ok(quote!(#(#impls)*))
}

fn into_key_targets(input: &DeriveInput) -> syn::Result<Vec<Path>> {
    let mut targets = Vec::new();
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("into_key")) {
        attr.parse_nested_meta(|meta| {
            targets.push(meta.path.clone());
            Ok(())
        })?;
    }

    if targets.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "missing target: `#[into_key(SomeKey)]`",
        ));
    }
    Ok(targets)
}

/// Derives `TryFrom<SymbolKey> for Self`, with `Error = SymbolKey`.
/// 
/// Requires every variant's name and inner type to match those in `SymbolKey`.
///
/// ```ignore
/// #[derive(TryFromSymbolKey)]
/// pub enum SourceFileKey {
///     File(FileKey),   // matches SymbolKey::File(FileKey)
/// }
/// ```
#[proc_macro_derive(TryFromSymbolKey)]
pub fn derive_try_from_symbol_key(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(expand_try_from_symbol_key(&input))
}

fn expand_try_from_symbol_key(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let variants: Vec<&Ident> = inner_types(input)?.into_iter().map(|(v, _)| v).collect();
    let name = &input.ident;
    let symbol_key = symbol_key_path();

    Ok(quote! {
        impl ::core::convert::TryFrom<#symbol_key> for #name {
            type Error = #symbol_key;

            #[allow(unreachable_patterns)]
            fn try_from(key: #symbol_key) -> ::core::result::Result<Self, Self::Error> {
                match key {
                    #( #symbol_key::#variants(k) => ::core::result::Result::Ok(#name::#variants(k)), )*
                    _ => ::core::result::Result::Err(key),
                }
            }
        }
    })
}

fn symbol_key_path() -> Path {
    parse_quote!(crate::core::symbols::symbol_keys::SymbolKey)
}

/// Derives `KeyValidator<Self> for SymbolTable`.
/// 
/// Requires `SymbolTable: KeyValidator<Key>` for every inner `Key` type.
///
/// ```ignore
/// #[derive(Validator)]
/// pub enum ModelSymbolKey {
///     Class(ClassKey),
///     XmlRecord(XmlRecordKey),
/// }
/// ```
/// Note the impl lands on `SymbolTable`, not on the enum.
#[proc_macro_derive(Validator)]
pub fn derive_validator(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(expand_validator(&input))
}

fn expand_validator(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let variants: Vec<&Ident> = inner_types(input)?.into_iter().map(|(v, _)| v).collect();
    let name = &input.ident;
    let key_validator = quote!(crate::core::symbols::symbol_keys::KeyValidator);
    let symbol_table = quote!(crate::core::symbols::storage::SymbolTable);

    Ok(quote! {
        impl #key_validator<#name> for #symbol_table {
            fn is_key_valid(&self, key: #name) -> bool {
                match key {
                    #( #name::#variants(k) => self.is_key_valid(k), )*
                }
            }
        }
    })
}

/// Convenience derive macro for an enum whose variants are a subset of [`SymbolKey`]'s.
/// 
/// Derives [`From`] + [`IntoSymbolKey`] + [`TryFromSymbolKey`] + [`Validator`] at once.
///
/// ```ignore
/// #[derive(Debug, Clone, Copy, SymbolKeySubset)]
/// pub enum DataFileKey {
///     XmlFile(XmlFileKey),
///     CsvFile(CsvFileKey),
/// }
/// ```
///
/// Inherits the requirements of the 4 derive-macros above, notably:
///
/// **Every variant's name and inner type must match those in `SymbolKey`**.
/// 
/// Notes:
/// - combining it with any of its four parts is a conflicting impl.
/// - [`Validator`] is the one part landing on `SymbolTable` instead of on the
///   enum, so a hand-written `KeyValidator<Self> for SymbolTable` conflicts too.
#[proc_macro_derive(SymbolKeySubset)]
pub fn derive_symbol_key_subset(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    finish(expand_symbol_key_subset(&input))
}

fn expand_symbol_key_subset(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let from_inner = expand_from_inner(input)?;
    let into_symbol_key = expand_into(input, &[symbol_key_path()])?;
    let try_from_symbol_key = expand_try_from_symbol_key(input)?;
    let validate_inner = expand_validator(input)?;

    Ok(quote!(#from_inner #into_symbol_key #try_from_symbol_key #validate_inner))
}

/// Each variant paired with its inner type. Rejects anything but a non-empty
/// enum of one-value tuple variants.
fn inner_types(input: &DeriveInput) -> syn::Result<Vec<(&Ident, &Type)>> {
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "can only be derived for enums",
        ));
    };

    let mut variants = Vec::new();
    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                variant,
                "every variant must hold exactly one value, e.g. `XmlRecord(XmlRecordKey)`",
            ));
        };
        if fields.unnamed.len() != 1 {
            return Err(syn::Error::new_spanned(
                variant,
                "every variant must hold exactly one value",
            ));
        }
        variants.push((&variant.ident, &fields.unnamed[0].ty));
    }

    if variants.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "enum must have at least one variant",
        ));
    }
    Ok(variants)
}

fn finish(expanded: syn::Result<TokenStream2>) -> TokenStream {
    match expanded {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
