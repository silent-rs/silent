use convert_case::Casing;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::Token;
use syn::punctuated::Punctuated;
use syn::{
    Expr, ExprLit, FnArg, ItemFn, Lit, Meta, Result as SynResult, parse::Parse, parse::ParseStream,
};

fn endpoint_impl(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    struct MetaArgs(Punctuated<Meta, Token![,]>);
    impl Parse for MetaArgs {
        fn parse(input: ParseStream) -> SynResult<Self> {
            Ok(MetaArgs(Punctuated::parse_terminated(input)?))
        }
    }
    let MetaArgs(args) = syn::parse2::<MetaArgs>(attr).expect("parse attr");
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

    let input: ItemFn = syn::parse2(item).expect("parse item fn");
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

    // 为自定义 Ok(T) 注册 ToSchema 完整 schema
    let ret_schema_register = {
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
                                                if id.ident == "Response" || id.ident == "String" {
                                                    quote!()
                                                } else {
                                                    let ty = ok_ty.clone();
                                                    quote!(::silent_openapi::doc::register_schema_for::<#ty>();)
                                                }
                                            } else {
                                                quote!()
                                            }
                                        }
                                        syn::Type::Reference(r) => {
                                            if let syn::Type::Path(tp2) = r.elem.as_ref() {
                                                if let Some(id) = tp2.path.segments.last() {
                                                    if id.ident == "str" {
                                                        quote!()
                                                    } else {
                                                        let inner = tp2.clone();
                                                        quote!(::silent_openapi::doc::register_schema_for::<#inner>();)
                                                    }
                                                } else {
                                                    quote!()
                                                }
                                            } else {
                                                quote!()
                                            }
                                        }
                                        _ => quote!(),
                                    }
                                } else {
                                    quote!()
                                }
                            } else {
                                quote!()
                            }
                        } else {
                            quote!()
                        }
                    } else {
                        quote!()
                    }
                } else {
                    quote!()
                }
            }
            _ => quote!(),
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
                                #ret_schema_register
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
                                #ret_schema_register
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
                                #ret_schema_register
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

    code
}

#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    endpoint_impl(attr.into(), item.into()).into()
}

#[cfg(test)]
mod tests {
    use quote::quote;

    fn render(ts: proc_macro2::TokenStream) -> String {
        ts.to_string()
    }

    #[test]
    fn generates_endpoint_type_and_const_for_request_sig() {
        let attr = quote!(summary = "hello", description = "world");
        let item = quote!(
            async fn get_hello(_req: ::silent::Request) -> ::silent::Result<::silent::Response> {
                unimplemented!()
            }
        );
        let out = super::endpoint_impl(attr, item);
        let s = render(out);
        assert!(s.contains("struct GetHelloEndpoint"));
        assert!(s.contains("const get_hello"));
    }

    #[test]
    fn generates_into_route_handler_for_extractor_sig() {
        let attr = quote!();
        let item = quote!(
            async fn get_user(_id: Path<u64>) -> ::silent::Result<::silent::Response> {
                unimplemented!()
            }
        );
        let out = super::endpoint_impl(attr, item);
        let s = render(out);
        // 生成的端点常量与 IntoRouteHandler 实现
        assert!(s.contains("struct GetUserEndpoint"));
        assert!(s.contains("const get_user"));
        assert!(s.contains("IntoRouteHandler"));
        assert!(s.contains("GetUserEndpoint"));
    }

    #[test]
    fn registers_response_meta_for_string() {
        let attr = quote!();
        let item = quote!(
            async fn ping(_req: ::silent::Request) -> ::silent::Result<String> {
                unimplemented!()
            }
        );
        let out = super::endpoint_impl(attr, item);
        let s = render(out);
        // 生成文本响应的注册调用
        assert!(s.contains("ResponseMeta :: TextPlain"));
    }
}
