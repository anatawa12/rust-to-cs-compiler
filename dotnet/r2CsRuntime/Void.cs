namespace r2CsRuntime;

/// <summary>
/// Zero-sized unit type, equivalent to Rust's <c>()</c>.
///
/// Used as the erased "self" pointer type in dyn-trait vtable dispatch:
/// a <see cref="Ref{T}">Ref&lt;Void&gt;</see> carries the opaque data pointer
/// that is then reinterpret-cast inside vtable method implementations to the
/// concrete <c>s_</c> struct type.
/// </summary>
public readonly struct Void { }
