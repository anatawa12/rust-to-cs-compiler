namespace r2CsRuntime;

/// <summary>
/// Thrown when compiled Rust code executes <c>panic!()</c>.
///
/// Maps to Rust's unwinding panic. The generated code wraps scopes in
/// <c>try/finally</c> so that <see cref="t_Drop{TSelf}"/> implementations
/// still run during stack unwinding, matching Rust's Drop ordering guarantee.
///
/// <para>Design note: Rust aborts on a double-panic (panic inside Drop).
/// In C# a second <c>throw</c> inside a <c>finally</c> block discards the
/// original exception. This divergence is intentional and documented in
/// the design as a known non-reproduction.</para>
/// </summary>
public class PanicException : Exception
{
    public PanicException(string message) : base(message) { }

    public PanicException(string message, Exception innerException)
        : base(message, innerException) { }
}
