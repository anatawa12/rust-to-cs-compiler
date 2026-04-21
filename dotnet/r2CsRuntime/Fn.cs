namespace r2CsRuntime;

/// <summary>
/// Corresponds to Rust's <c>FnOnce</c> trait.
/// The call conceptually consumes <c>self</c>; the compiler guarantees the
/// object is not used again after <c>m_call_once</c> is invoked.
///
/// <para>Design note (C# 10 limitation): Rust's <c>FnOnce::call_once</c>
/// takes <c>self</c> by value. In C# 10 we cannot express a consuming
/// <c>this</c> in an interface. The consuming contract is enforced by the
/// Rust-side type checker during compilation, not re-checked in C#.</para>
/// </summary>
/// <typeparam name="TSelf">The implementing type (F-bounded polymorphism).</typeparam>
/// <typeparam name="TArgs">Argument tuple type.</typeparam>
/// <typeparam name="TReturn">Return type.</typeparam>
public interface t_FnOnce<TSelf, TArgs, TReturn>
    where TSelf : t_FnOnce<TSelf, TArgs, TReturn>
{
    TReturn m_call_once(TArgs args);
}

/// <summary>
/// Corresponds to Rust's <c>FnMut</c> trait.
/// The call may mutate captured variables; for class closures <c>this</c>
/// is already a reference so mutation works naturally.
///
/// <para>Design note (C# 10 limitation): Rust's <c>FnMut::call_mut</c>
/// takes <c>&amp;mut self</c>. A <c>ref TSelf</c> parameter cannot be
/// expressed as an instance method in C# 10. For struct implementors, the
/// compiler must call <c>m_call_mut</c> directly on the concrete type
/// (not through the interface) to avoid boxing. For class closures the
/// distinction is irrelevant because classes are already reference types.</para>
/// </summary>
/// <typeparam name="TSelf">The implementing type (F-bounded polymorphism).</typeparam>
/// <typeparam name="TArgs">Argument tuple type.</typeparam>
/// <typeparam name="TReturn">Return type.</typeparam>
public interface t_FnMut<TSelf, TArgs, TReturn> : t_FnOnce<TSelf, TArgs, TReturn>
    where TSelf : t_FnMut<TSelf, TArgs, TReturn>
{
    TReturn m_call_mut(TArgs args);

    // t_FnOnce.m_call_once delegates to m_call_mut.
    // Default interface methods require .NET Core 3.0+; netstandard2.0 does not
    // support them, so implementors must explicitly provide both methods.
    // The compiler always generates both.
}

/// <summary>
/// Corresponds to Rust's <c>Fn</c> trait.
/// The call only reads captured variables; the closure may be called
/// concurrently from multiple callers.
///
/// <para>Design note (C# 10 limitation): Rust's <c>Fn::call</c> takes
/// <c>&amp;self</c> (shared reference). C# 10 cannot express an <c>in TSelf</c>
/// constraint on an instance interface method. The read-only contract is
/// enforced by the Rust type checker, not by C#.</para>
/// </summary>
/// <typeparam name="TSelf">The implementing type (F-bounded polymorphism).</typeparam>
/// <typeparam name="TArgs">Argument tuple type.</typeparam>
/// <typeparam name="TReturn">Return type.</typeparam>
public interface t_Fn<TSelf, TArgs, TReturn> : t_FnMut<TSelf, TArgs, TReturn>
    where TSelf : t_Fn<TSelf, TArgs, TReturn>
{
    TReturn m_call(TArgs args);

    // Implementors must also provide m_call_mut (which calls m_call) and
    // m_call_once (which calls m_call) because netstandard2.0 does not
    // support default interface methods.
}
