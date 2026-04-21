# MIR vs THIR — IR Choice for Code Generation

## Background

The Rust compiler exposes two structured intermediate representations that are
richer than raw HIR and suitable for code generation:

| IR   | Full name | When available | Preserves |
|------|-----------|---------------|-----------|
| THIR | Typed High-level IR | *during* analysis (before borrow check steals it) | `async`/`await`, `match`, `?`, closures as structured constructs |
| MIR  | Mid-level IR | *after* analysis (always available in `after_analysis`) | Basic blocks + gotos; all high-level constructs desugared |

## Why the transpiler currently uses MIR

### The THIR steal problem

`rustc_driver::Callbacks::after_analysis` is called **after** the full analysis
phase, which includes borrow checking.  The borrow checker calls
`tcx.thir_body(def_id)` internally and then "steals" the result — consuming the
`Steal<Thir>` cell so it cannot be read again.  Any attempt to call
`tcx.thir_body` inside `after_analysis` panics:

```
thread 'rustc' panicked at src/codegen/item.rs:28:26:
attempted to read from stolen value: rustc_middle::thir::Thir<'_>
```

The only reliable source of function bodies available in `after_analysis` is
**MIR** (`tcx.optimized_mir`), which is why the transpiler uses it.

### Workaround considered

It is theoretically possible to register a callback that runs *before* the
borrow checker (e.g. by overriding `after_expansion` and driving analysis
manually).  That approach is fragile, requires duplicating the analysis
pipeline, and would block progress indefinitely — so MIR was chosen as the
pragmatic option.

---

## Implications for async support

This is the critical trade-off.

**THIR** preserves `async fn` and `.await` as syntactic constructs in the IR.
This would allow the transpiler to emit idiomatic C# `async Task<T>` methods
where every Rust `await` maps to a C# `await`.

**MIR** desugars every `async fn` into a **state machine struct** — a generated
type with a `poll` method that contains a large `switch` over the coroutine
state.  Transpiling MIR therefore produces unreadable coroutine boilerplate
instead of clean `async/await` code:

```
// What Rust async fn desugars to in MIR (simplified)
struct FetchData { state: u32, _field0: ..., ... }
impl Future for FetchData {
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Output> { ... }
}
```

This is the **primary downside** of the current approach.

## Recommended future direction

To emit readable `async/await` C#, the transpiler needs access to THIR.
The cleanest path is to use a `rustc_driver` query hook that runs before the
borrow checker steals THIR:

1. Override `after_expansion` or use `after_hir_lowering` to intercept the
   compilation pipeline early.
2. Manually steer analysis phases so that THIR can be read **before** borrow
   checking runs (or run borrow checking in a mode that does not steal THIR).
3. Fall back to MIR for items where THIR is unavailable or insufficient.

Until that work is done, async functions will transpile to state machine code.
All synchronous Rust code is unaffected — MIR and THIR produce equivalent
results for it.

## Current status

* All **synchronous** functions and structs: compiled from MIR — basic blocks,
  gotos, field access, arithmetic, struct construction.
* **async functions**: compiled from MIR — state machine struct emitted.
  The output is correct but verbose and unreadable.
* `match` expressions: compiled from MIR `SwitchInt` — produces `switch` / `if-else`.
