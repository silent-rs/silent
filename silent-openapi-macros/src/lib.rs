use convert_case::Casing;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::Token;
use syn::punctuated::Punctuated;
use syn::{
    Expr, ExprLit, FnArg, ItemFn, Lit, Meta, Result as SynResult, parse::Parse, parse::ParseStream,
    parse_macro_input,
};

#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    struct MetaArgs(Punctuated<Meta, Token![,]>);
    impl Parse for MetaArgs {
        fn parse(input: ParseStream) -> SynResult<Self> {
            Ok(MetaArgs(Punctuated::parse_terminated(input)?))
        }
    }
    let MetaArgs(args) = parse_macro_input!(attr as MetaArgs);
    let mut summary_arg: Option<String> = None;
    let mut description_arg: Option<String> = None;
    for meta in args {
        if let Meta::NameValue(nv) = meta {
            if nv.path.is_ident("summary") {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) = nv.value
                {
                    summary_arg = Some(s.value());
                }
            } else if nv.path.is_ident("description") {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) = nv.value
                {
                    description_arg = Some(s.value());
                }
            }
        }
    }

    let input = parse_macro_input!(item as ItemFn);
    let vis = &input.vis;
    let sig = input.sig.clone();
    let attrs = &input.attrs;
    let block = &input.block;
    let name = &sig.ident;

    // 收集文档注释作为默认 summary/description
    let mut doc_lines: Vec<String> = Vec::new();
    for a in attrs.iter() {
        if a.path().is_ident("doc") {
            let _ = a.parse_nested_meta(|meta| {
                let lit: syn::LitStr = meta.value()?.parse()?;
                let v = lit.value();
                doc_lines.push(v.trim().to_string());
                Ok(())
            });
        }
    }
    let (def_summary, def_description) = if !doc_lines.is_empty() {
        let mut it = doc_lines.into_iter().filter(|s| !s.is_empty());
        if let Some(first) = it.next() {
            let rest = it.collect::<Vec<_>>().join("\n");
            (Some(first), if rest.is_empty() { None } else { Some(rest) })
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let summary = summary_arg.or(def_summary);
    let description = description_arg.or(def_description);

    // 真实处理函数改名
    let impl_name = format_ident!("{}_impl", name);
    // 生成实现函数签名（重命名）
    let mut impl_sig = sig.clone();
    impl_sig.ident = impl_name.clone();

    // 端点类型 + 常量（实现与原 `.get(get_xxx)` 风格兼容）
    let ep_ty = format_ident!(
        "{}Endpoint",
        name.to_string().to_case(convert_case::Case::UpperCamel)
    );
    let sum_tokens = if let Some(s) = &summary {
        let lit = syn::LitStr::new(&s, proc_macro2::Span::call_site());
        quote!(Some(#lit))
    } else {
        quote!(None)
    };
    let desc_tokens = if let Some(s) = &description {
        let lit = syn::LitStr::new(&s, proc_macro2::Span::call_site());
        quote!(Some(#lit))
    } else {
        quote!(None)
    };

    // 解析返回类型 Ok(T) -> ResponseMeta
    let ret_meta = {
        match &sig.output {
            syn::ReturnType::Type(_, ty) => {
                if let syn::Type::Path(tp) = ty.as_ref() {
                    if let Some(seg) = tp.path.segments.last() {
                        if seg.ident == "Result" || seg.ident == "SilentResult" {
                            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                                if let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first() {
                                    match ok_ty {
                                        syn::Type::Path(tpath) => {
                                            if let Some(id) = tpath.path.segments.last() {
                                                if id.ident == "Response" {
                                                    quote!(None)
                                                } else if id.ident == "String" {
                                                    quote!(Some(::silent_openapi::doc::ResponseMeta::TextPlain))
                                                } else {
                                                    let tn = id.ident.to_string();
                                                    quote!(Some(::silent_openapi::doc::ResponseMeta::Json { type_name: #tn }))
                                                }
                                            } else {
                                                quote!(None)
                                            }
                                        }
                                        syn::Type::Reference(r) => {
                                            if let syn::Type::Path(tp2) = r.elem.as_ref() {
                                                if let Some(id) = tp2.path.segments.last() {
                                                    if id.ident == "str" {
                                                        quote!(Some(::silent_openapi::doc::ResponseMeta::TextPlain))
                                                    } else {
                                                        let tn = id.ident.to_string();
                                                        quote!(Some(::silent_openapi::doc::ResponseMeta::Json { type_name: #tn }))
                                                    }
                                                } else {
                                                    quote!(None)
                                                }
                                            } else {
                                                quote!(None)
                                            }
                                        }
                                        _ => quote!(None),
                                    }
                                } else {
                                    quote!(None)
                                }
                            } else {
                                quote!(None)
                            }
                        } else {
                            quote!(None)
                        }
                    } else {
                        quote!(None)
                    }
                } else {
                    quote!(None)
                }
            }
            _ => quote!(None),
        }
    };

    // 根据函数参数形态生成 IntoRouteHandler 实现
    let inputs = sig.inputs.clone().into_iter().collect::<Vec<_>>();
    let impls = if inputs.len() == 1 {
        match &inputs[0] {
            FnArg::Typed(pat_ty) => {
                let ty = &pat_ty.ty;
                // 简单规则：类型标识名为 Request 则认为是 Request 形态
                let is_request = matches!(
                    &**ty,
                    syn::Type::Path(tp) if tp.path.segments.last().map(|s| s.ident == "Request").unwrap_or(false)
                );
                if is_request {
                    quote! {
                        impl ::silent::prelude::IntoRouteHandler<::silent::Request> for #ep_ty {
                            fn into_handler(self) -> std::sync::Arc<dyn ::silent::Handler> {
                                let handler = std::sync::Arc::new(::silent::HandlerWrapper::new(#impl_name));
                                let ptr = std::sync::Arc::as_ptr(&handler) as *const () as usize;
                                ::silent_openapi::doc::register_doc_by_ptr(
                                    ptr,
                                    #sum_tokens,
                                    #desc_tokens,
                                );
                                if let Some(meta) = #ret_meta { ::silent_openapi::doc::register_response_by_ptr(ptr, meta); }
                                handler
                            }
                        }
                    }
                } else {
                    // 单萃取器参数
                    quote! {
                        impl ::silent::prelude::IntoRouteHandler<#ty> for #ep_ty {
                            fn into_handler(self) -> std::sync::Arc<dyn ::silent::Handler> {
                                let adapted = ::silent::extractor::handler_from_extractor::<#ty, _, _, _>(#impl_name);
                                let handler = std::sync::Arc::new(::silent::HandlerWrapper::new(adapted));
                                let ptr = std::sync::Arc::as_ptr(&handler) as *const () as usize;
                                ::silent_openapi::doc::register_doc_by_ptr(
                                    ptr,
                                    #sum_tokens,
                                    #desc_tokens,
                                );
                                if let Some(meta) = #ret_meta { ::silent_openapi::doc::register_response_by_ptr(ptr, meta); }
                                handler
                            }
                        }
                    }
                }
            }
            _ => quote! {},
        }
    } else if inputs.len() == 2 {
        match (&inputs[0], &inputs[1]) {
            (FnArg::Typed(first), FnArg::Typed(second)) => {
                let ty1 = &first.ty;
                let ty2 = &second.ty;
                // 期望形态： (Request, Args)
                let is_request_first = matches!(
                    &**ty1,
                    syn::Type::Path(tp) if tp.path.segments.last().map(|s| s.ident == "Request").unwrap_or(false)
                );
                if is_request_first {
                    quote! {
                        impl ::silent::prelude::IntoRouteHandler<(::silent::Request, #ty2)> for #ep_ty {
                            fn into_handler(self) -> std::sync::Arc<dyn ::silent::Handler> {
                                let adapted = ::silent::extractor::handler_from_extractor_with_request::<#ty2, _, _, _>(#impl_name);
                                let handler = std::sync::Arc::new(::silent::HandlerWrapper::new(adapted));
                                let ptr = std::sync::Arc::as_ptr(&handler) as *const () as usize;
                                ::silent_openapi::doc::register_doc_by_ptr(
                                    ptr,
                                    #sum_tokens,
                                    #desc_tokens,
                                );
                                if let Some(meta) = #ret_meta { ::silent_openapi::doc::register_response_by_ptr(ptr, meta); }
                                handler
                            }
                        }
                    }
                } else {
                    quote! {}
                }
            }
            _ => quote! {},
        }
    } else {
        quote! {}
    };

    let code = quote! {
        // 原函数体改名为实现函数
        #(#attrs)*
        #impl_sig #block

        // 端点类型（零尺寸） + 常量，同名以保留 `.get(get_xxx)` 调用方式
        pub struct #ep_ty;
        #[allow(non_upper_case_globals)]
        #vis const #name: #ep_ty = #ep_ty;

        #impls
    };

    code.into()
}
