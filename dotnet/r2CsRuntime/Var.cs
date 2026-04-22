namespace r2CsRuntime;

/// <summary>
/// Heap-allocated wrapper for a Rust local variable whose address must survive
/// beyond its stack frame.
///
/// The Rust compiler analysis uses this type whenever a local satisfies any of:
/// <list type="bullet">
///   <item>its borrow is captured by a closure</item>
///   <item>its borrow escapes the function via a return value</item>
///   <item>the local lives inside an <c>async fn</c> body (async state-machine
///         fields are already on the heap, so their addresses are heap refs)</item>
/// </list>
///
/// The field <see cref="f_value"/> is public so that the generated C# code can
/// take a managed reference (<c>ref f_value</c>) and convert it to a
/// <see cref="Ref{T}"/> via <see cref="RefHelper.FromHeapField{TOwner,T}"/>.
/// </summary>
/// <typeparam name="T">The type of the wrapped variable.</typeparam>
public sealed class Var<T>
{
    public T f_value;

    /// <summary>Initialises the wrapper with a specific value.</summary>
    public Var(T value) => f_value = value;

    /// <summary>Initialises the wrapper with <c>default(T)</c>.</summary>
    public Var() => f_value = default!;
}
