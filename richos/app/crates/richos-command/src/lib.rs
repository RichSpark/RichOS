//! **`#[richos_command::command]`: a window command whose body never blocks one of Tauri's
//! async workers.** Written in place of `#[tauri::command(async)]`, with the body unchanged.
//!
//! # What was wrong, measured
//!
//! Tauri 2.11.5 runs a `#[tauri::command(async)]` SYNC body inside `async_runtime::spawn`:
//! `(async)` selects `body_async` (`tauri-macros-2.6.3/src/command/wrapper.rs:249`, its kind
//! string `"sync_threadpool"` at `:264`), which at `:389-393` emits
//! `resolver.respond_async_serialized(async move { let result = $path(args); … })`, and
//! `tauri-2.11.5/src/ipc/mod.rs:329` spawns that future on the async runtime. So the body runs on
//! one of the runtime's worker threads — one per core — and a body that blocks (a child process,
//! a wait on the spine's mutex, a slow file or lock) holds that worker for as long as it blocks.
//! Every other command of the window, however quick, waits for a free worker.
//!
//! In the real app (test VM, 4 cores, escalation esc-20261011T062202Z-883703bd): closing
//! Preferences after choosing Fable left `config.json` at `rich_model=opus` at +2 s, +10 s and
//! about +68 s in three runs while the interview card's "Not now" was waiting
//! (`a window command waited 15093 ms for the spine` at `decline_onboarding`), and a `sample`
//! showed 3 of the 4 `tokio-rt-worker` threads inside `richos_voice::stt::time_decode` ->
//! `std::process::Command::output`. The save's own work is microseconds; it queued.
//!
//! # What this does
//!
//! It emits the same `#[tauri::command(async)]` (plus any arguments given here), over a body
//! that runs under `tokio::task::block_in_place`:
//!
//! - a sync body runs as `block_in_place(move || -> R { body })`;
//! - an async body runs as `block_in_place(|| Handle::current().block_on(async move { body }))`.
//!
//! `block_in_place` is tokio's own way to block from a worker: it hands the worker's core, with
//! every task queued on it, to another thread before the closure runs, so the runtime keeps all
//! its workers serving other commands while this one waits. It is used rather than
//! `spawn_blocking` because it does not require `'static`: a command's `State<'_, T>` and its
//! other borrowed arguments stay exactly as they are, and so does every body.
//!
//! **Only on the multi-threaded runtime**, which is the one Tauri runs commands on.
//! `block_in_place` panics on a current-thread runtime, so anywhere else (a unit test calling a
//! command function directly, an example's own runtime) the body simply runs where it is, as it
//! always did.
//!
//! The emitted code names only `::tauri` and `::tokio`, so a command in a file an example
//! compiles by `#[path]` (`examples/output_files_probe.rs`) expands there as well.
//!
//! The tests below check the expansion; the shell's `src/command_rule_tests.rs` checks it
//! through Tauri's real dispatch and refuses a bare `#[tauri::command]` anywhere in the shell.
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::{ItemFn, ReturnType, Type};

/// See the crate documentation. Arguments are passed through to `#[tauri::command(async, …)]`.
#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(attr.into(), item.into()).into()
}

/// The whole transformation, on `proc_macro2` tokens so the tests below can call it.
fn expand(attr: Tokens, item: Tokens) -> Tokens {
    let mut function: ItemFn = match syn::parse2(item) {
        Ok(function) => function,
        Err(error) => return error.to_compile_error(),
    };
    let block = &function.block;
    let on_a_worker = quote! {
        ::tokio::runtime::Handle::try_current()
            .is_ok_and(|h| h.runtime_flavor() == ::tokio::runtime::RuntimeFlavor::MultiThread)
    };
    let body = if function.sig.asyncness.is_some() {
        quote!({
            let __richos_body = async move #block;
            if #on_a_worker {
                ::tokio::task::block_in_place(move || ::tokio::runtime::Handle::current().block_on(__richos_body))
            } else {
                __richos_body.await
            }
        })
    } else {
        // The closure is told the function's own return type, so `?` and early `return` in the
        // body mean exactly what they meant before. `impl Trait` cannot be written on a closure,
        // so that one case is left to inference, as is `()` (no `?` can apply to it).
        let ret = match &function.sig.output {
            ReturnType::Default => quote!(),
            ReturnType::Type(_, ty) if matches!(**ty, Type::ImplTrait(_)) => quote!(),
            ReturnType::Type(_, ty) => quote!(-> #ty),
        };
        quote!({
            let __richos_body = move || #ret #block;
            if #on_a_worker {
                ::tokio::task::block_in_place(__richos_body)
            } else {
                __richos_body()
            }
        })
    };
    function.block = match syn::parse2(body) {
        Ok(block) => Box::new(block),
        Err(error) => return error.to_compile_error(),
    };
    let args = if attr.is_empty() { quote!(async) } else { quote!(async, #attr) };
    quote!(
        #[::tauri::command(#args)]
        #function
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The expansion as one line with single spaces, which is how `TokenStream`'s `Display`
    /// spaces tokens; the assertions below are written in that spacing.
    fn expanded(attr: Tokens, item: Tokens) -> String {
        expand(attr, item).to_string().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// A sync body runs inside `block_in_place`, as a closure told the function's return type,
    /// under the `(async)` Tauri attribute; the signature and the doc comment are untouched.
    #[test]
    fn a_sync_body_runs_under_block_in_place_with_its_return_type() {
        let out = expanded(quote!(), quote! {
            /// Saves it.
            fn save(state: State<'_, Saved>, model: String) -> Result<String, String> { Ok(model) }
        });
        assert!(out.starts_with("# [:: tauri :: command (async)]"), "{out}");
        assert!(out.contains("Saves it."), "the doc comment is kept: {out}");
        assert!(out.contains("fn save (state : State < '_ , Saved > , model : String) -> Result < String , String >"), "{out}");
        assert!(out.contains("let __richos_body = move | | -> Result < String , String > { Ok (model) }"), "{out}");
        assert!(out.contains(":: tokio :: task :: block_in_place (__richos_body)"), "{out}");
        assert!(out.contains("RuntimeFlavor :: MultiThread"), "only on the multi-threaded runtime: {out}");
        assert!(out.contains("__richos_body ()"), "elsewhere it runs where it is: {out}");
    }

    /// An async body is driven by `Handle::block_on` inside `block_in_place`, so blocking work
    /// between its awaits does not hold a worker either.
    #[test]
    fn an_async_body_is_driven_inside_block_in_place() {
        let out = expanded(quote!(), quote! {
            async fn fetch(app: AppHandle) -> Result<(), String> { work(&app).await }
        });
        assert!(out.contains("async fn fetch (app : AppHandle) -> Result < () , String >"), "{out}");
        assert!(out.contains("let __richos_body = async move { work (& app) . await }"), "{out}");
        assert!(
            out.contains(":: tokio :: task :: block_in_place (move | | :: tokio :: runtime :: Handle :: current () . block_on (__richos_body))"),
            "{out}"
        );
        assert!(out.contains("__richos_body . await"), "{out}");
    }

    /// No return type and `impl Trait` are left to inference; nothing is written on the closure.
    #[test]
    fn unit_and_impl_trait_returns_are_not_written_on_the_closure() {
        let unit = expanded(quote!(), quote! { fn tick(app: AppHandle) { app.go(); } });
        assert!(unit.contains("let __richos_body = move | | { app . go () ; }"), "{unit}");
        let opaque = expanded(quote!(), quote! { fn it() -> impl Serialize { 1 } });
        assert!(opaque.contains("let __richos_body = move | | { 1 }"), "{opaque}");
    }

    /// Arguments given to this attribute reach Tauri's, after `async`.
    #[test]
    fn arguments_pass_through_to_the_tauri_attribute() {
        let out = expanded(quote!(rename_all = "snake_case"), quote! { fn f() {} });
        assert!(out.starts_with("# [:: tauri :: command (async , rename_all = \"snake_case\")]"), "{out}");
    }

    /// Something that is not a function is refused at compile time, not passed on.
    #[test]
    fn a_non_function_is_a_compile_error() {
        let out = expanded(quote!(), quote! { struct S; });
        assert!(out.contains("compile_error"), "{out}");
    }
}
