//! RustScope proc-macro attributes.
//!
//! - `#[profile]`     — instrument a function (timing, memory, stack, CPU)
//! - `#[benchmark]`   — statistical micro-benchmark (N iterations, percentiles)
//! - `#[profile_all]` — instrument every fn in a `mod` or `impl` block

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TS2;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ItemMod, ItemImpl};

// ─── #[profile] ──────────────────────────────────────────────────────────────

/// Instrument a single function.
///
/// For **sync** functions: records wall-clock time, heap, stack frame size,
/// CPU counters (Linux hw-counters feature), call depth, recursion depth.
///
/// For **async** functions: wraps the future so the `ProfileGuard` is entered
/// on each poll and exited on each yield — measuring **active CPU time only**,
/// not suspension time. This gives the same semantics as Phase 2's tracing Layer
/// without requiring `#[tracing::instrument]`.
///
/// # Options
/// - `name = "custom"` — override the recorded function name
/// - `budget_ns = 5000` — register a per-call latency budget (fires SLO callback)
///
/// # Examples
/// ```rust,ignore
/// #[profile]
/// fn my_fn(x: u32) -> u32 { x * 2 }
///
/// #[profile(name = "custom_label")]
/// async fn fetch(url: &str) -> String { ... }
///
/// #[profile(budget_ns = 1_000_000)]
/// fn handler(req: Request) -> Response { ... }
/// ```
#[proc_macro_attribute]
pub fn profile(args: TokenStream, input: TokenStream) -> TokenStream {
    let args2 = TS2::from(args);
    let func = parse_macro_input!(input as ItemFn);
    TokenStream::from(emit_profile_fn(args2, func))
}

/// Instrument a function AND run it as a statistical micro-benchmark.
///
/// # Options
/// - `iters = N`   — iteration count (default 1000)
/// - `warmup = N`  — warmup iterations before measurement (default 100)
/// - `name = "x"`  — override label
///
/// # Example
/// ```rust,ignore
/// #[benchmark(iters = 5000, warmup = 500)]
/// fn sort_bench() {
///     let mut v: Vec<u32> = (0..1000).rev().collect();
///     v.sort_unstable();
///     std::hint::black_box(v);
/// }
/// ```
#[proc_macro_attribute]
pub fn benchmark(args: TokenStream, input: TokenStream) -> TokenStream {
    let args2 = TS2::from(args);
    let func = parse_macro_input!(input as ItemFn);
    TokenStream::from(emit_benchmark_fn(args2, func))
}

/// Instrument every `fn` inside a `mod` or `impl` block.
///
/// # Example
/// ```rust,ignore
/// #[profile_all]
/// mod math {
///     pub fn add(a: i32, b: i32) -> i32 { a + b }
///     pub fn mul(a: i32, b: i32) -> i32 { a * b }
/// }
///
/// #[profile_all]
/// impl MyCache {
///     pub fn get(&self, key: &str) -> Option<&str> { ... }
///     pub fn insert(&mut self, key: String, val: String) { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn profile_all(_args: TokenStream, input: TokenStream) -> TokenStream {
    if let Ok(item_mod) = syn::parse::<ItemMod>(input.clone()) {
        return TokenStream::from(instrument_mod(item_mod));
    }
    if let Ok(item_impl) = syn::parse::<ItemImpl>(input.clone()) {
        return TokenStream::from(instrument_impl(item_impl));
    }
    input
}

// ─── code generation ─────────────────────────────────────────────────────────

fn emit_profile_fn(args: TS2, func: ItemFn) -> TS2 {
    let fn_name_str = func.sig.ident.to_string();
    let label = extract_str_arg(&args.to_string(), "name")
        .unwrap_or_else(|| fn_name_str.clone());
    let budget_ns: Option<u64> = extract_u64_arg(&args.to_string(), "budget_ns");

    let is_async = func.sig.asyncness.is_some();
    let body = func.block.clone();
    let attrs = func.attrs.clone();
    let vis = func.vis.clone();
    let sig = func.sig.clone();

    // Budget registration code (emitted once per binary via a static initializer)
    let budget_init = if let Some(bns) = budget_ns {
        quote! {
            {
                static __RS_BUDGET_INIT: std::sync::Once = std::sync::Once::new();
                __RS_BUDGET_INIT.call_once(|| {
                    ::rustscope::features::outliers::set_budget(#label, #bns);
                });
            }
        }
    } else {
        quote! {}
    };

    if is_async {
        // For async fns: wrap the future so we enter/exit the ProfileGuard
        // on each poll — this measures active CPU time only, not suspension.
        //
        // The pattern:
        //   let __rs_guard = ProfileGuard::enter(...);
        //   poll the inner future
        //   drop __rs_guard on yield (Poll::Pending)
        //   re-enter __rs_guard on next poll
        //
        // We achieve this by wrapping the future in a struct that holds the guard.
        quote! {
            #(#attrs)*
            #vis #sig {
                #budget_init

                // Wrap the async body in a future that hooks enter/exit around polls
                struct __RsProfiledFuture<F: ::std::future::Future> {
                    inner: F,
                    name: &'static str,
                    file: &'static str,
                    line: u32,
                    module: &'static str,
                }

                impl<F: ::std::future::Future> ::std::future::Future for __RsProfiledFuture<F> {
                    type Output = F::Output;

                    fn poll(
                        self: ::std::pin::Pin<&mut Self>,
                        cx: &mut ::std::task::Context<'_>,
                    ) -> ::std::task::Poll<Self::Output> {
                        // SAFETY: we never move the inner future
                        let this = unsafe { self.get_unchecked_mut() };
                        let _guard = ::rustscope::ProfileGuard::enter(
                            this.name, this.file, this.line, this.module
                        );
                        let inner = unsafe { ::std::pin::Pin::new_unchecked(&mut this.inner) };
                        let result = inner.poll(cx);
                        // _guard drops here — on Poll::Pending this correctly
                        // records only the active poll time
                        result
                    }
                }

                __RsProfiledFuture {
                    inner: async move #body,
                    name: #label,
                    file: file!(),
                    line: line!(),
                    module: module_path!(),
                }
            }
        }
    } else {
        quote! {
            #(#attrs)*
            #vis #sig {
                #budget_init
                let __rs_guard = ::rustscope::ProfileGuard::enter(
                    #label, file!(), line!(), module_path!()
                );
                let __rs_result = (move || #body)();
                drop(__rs_guard);
                __rs_result
            }
        }
    }
}

fn emit_benchmark_fn(args: TS2, func: ItemFn) -> TS2 {
    let fn_name_str = func.sig.ident.to_string();
    let label = extract_str_arg(&args.to_string(), "name")
        .unwrap_or_else(|| fn_name_str.clone());
    let iters: u64  = extract_u64_arg(&args.to_string(), "iters").unwrap_or(1000);
    let warmup: u64 = extract_u64_arg(&args.to_string(), "warmup").unwrap_or(100);

    let attrs = func.attrs.clone();
    let vis   = func.vis.clone();
    let sig   = func.sig.clone();
    let body  = func.block.clone();

    quote! {
        #(#attrs)*
        #vis #sig {
            ::rustscope::run_benchmark(
                #label, file!(), line!(), module_path!(),
                #iters, #warmup,
                || { #body }
            );
        }
    }
}

fn instrument_mod(mut item_mod: ItemMod) -> TS2 {
    if let Some((brace, items)) = item_mod.content.take() {
        let new_items: Vec<syn::Item> = items.into_iter().map(|item| {
            if let syn::Item::Fn(func) = item {
                let ts = emit_profile_fn(quote! {}, func);
                syn::parse2(ts).unwrap_or_else(|_| syn::parse_quote!(fn __rs_err() {}))
            } else {
                item
            }
        }).collect();
        item_mod.content = Some((brace, new_items));
    }
    quote! { #item_mod }
}

fn instrument_impl(mut item_impl: ItemImpl) -> TS2 {
    let new_items: Vec<syn::ImplItem> = item_impl.items.into_iter().map(|item| {
        if let syn::ImplItem::Fn(mut method) = item {
            let label = method.sig.ident.to_string();
            let stmts = method.block.stmts.clone();
            method.block = syn::parse_quote! {{
                let __rs_guard = ::rustscope::ProfileGuard::enter(
                    #label, file!(), line!(), module_path!()
                );
                let __rs_result = (|| { #(#stmts)* })();
                drop(__rs_guard);
                __rs_result
            }};
            syn::ImplItem::Fn(method)
        } else {
            item
        }
    }).collect();
    item_impl.items = new_items;
    quote! { #item_impl }
}

// ─── argument parsers ─────────────────────────────────────────────────────────

fn extract_str_arg(args: &str, key: &str) -> Option<String> {
    let pat = format!("{key} =");
    let pos = args.find(&pat)?;
    let rest = args[pos + pat.len()..].trim();
    Some(rest.trim_matches(|c| c == '"' || c == ' ').to_string())
}

fn extract_u64_arg(args: &str, key: &str) -> Option<u64> {
    let pat = format!("{key} =");
    let pos = args.find(&pat)?;
    let rest = args[pos + pat.len()..].trim();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}
