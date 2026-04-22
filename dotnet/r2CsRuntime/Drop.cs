namespace r2CsRuntime;

/// <summary>
/// Corresponds to Rust's <c>Drop</c> trait.
///
/// Types that have custom cleanup logic implement this interface.
/// The generated code normally invokes drop directly on the concrete type
/// (no interface dispatch, no boxing) inside a <c>finally</c> block:
/// <code>
/// try { /* body */ }
/// finally { if (x_should_drop) x.m_drop(); }
/// </code>
///
/// This interface is only dispatched through when the concrete type is
/// not statically known, e.g. when dropping through a generic
/// <c>where T : t_Drop&lt;T&gt;</c> bound.
///
/// <para>Design note: calling this interface method on a <em>struct</em>
/// through the interface requires boxing (C# limitation). The generated
/// code avoids this by always calling <c>m_drop()</c> on the concrete
/// type directly. Boxing is therefore never needed in practice.</para>
/// </summary>
/// <typeparam name="TSelf">The implementing type (F-bounded polymorphism).</typeparam>
public interface t_Drop<TSelf> where TSelf : t_Drop<TSelf>
{
    /// <summary>
    /// Executes the custom drop logic. Corresponds to <c>Drop::drop(&amp;mut self)</c>.
    /// For struct implementors, <c>this</c> is an implicit <c>ref</c> within the
    /// instance method, so mutation is allowed.
    /// </summary>
    void m_drop();
}
