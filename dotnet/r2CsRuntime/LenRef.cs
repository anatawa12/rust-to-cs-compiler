namespace r2CsRuntime;

/// <summary>
/// Represents a Rust fat reference to an unsized type
/// (<c>&amp;T</c> or <c>&amp;mut T</c> where <c>T: ?Sized</c>).
///
/// <para><strong>Memory model</strong></para>
/// <c>LenRef&lt;T&gt; = (object? Target, nint Offset, nint Length)</c>
///
/// <list type="table">
///   <listheader><term>Case</term><description>Encoding</description></listheader>
///   <item>
///     <term>Heap</term>
///     <description>
///       <see cref="Target"/> = owning managed object;<br/>
///       <see cref="Offset"/> = byte offset from object data start to the
///       beginning of the unsized value;<br/>
///       <see cref="Length"/> = metadata (e.g. number of slice elements, or
///       number of tail elements in a DST struct).
///     </description>
///   </item>
///   <item>
///     <term>Stack / inline</term>
///     <description>
///       <see cref="Target"/> = <c>null</c>;<br/>
///       <see cref="Offset"/> = absolute address of the beginning of the value;<br/>
///       <see cref="Length"/> = same metadata.
///     </description>
///   </item>
/// </list>
///
/// <para><strong>Usage</strong></para>
/// <list type="bullet">
///   <item>
///     For a Rust <c>&amp;[T]</c> (slice), use <c>LenRef&lt;<see cref="Slice{T}"/>&gt;</c>.
///     Element access is provided by <see cref="SliceExt"/> extension methods.
///   </item>
///   <item>
///     For a DST struct <c>struct A(Fields, [T])</c>, use <c>LenRef&lt;s_A&gt;</c>
///     where <see cref="Length"/> is the number of tail elements.
///   </item>
/// </list>
/// </summary>
/// <typeparam name="T">
/// The unsized type being referenced.  Use <see cref="Slice{T}"/> for plain slices.
/// </typeparam>
public readonly struct LenRef<T>
{
    /// <summary>Owning heap object, or <c>null</c> for a stack/inline reference.</summary>
    public readonly object? Target;

    /// <summary>
    /// Byte offset from the object's first data byte to the referenced location,
    /// or the absolute stack address when <see cref="Target"/> is <c>null</c>.
    /// </summary>
    public readonly nint Offset;

    /// <summary>
    /// Slice length or DST tail element count associated with this fat reference.
    /// </summary>
    public readonly nint Length;

    /// <summary>Constructs a heap fat reference.</summary>
    public LenRef(object target, nint offset, nint length)
    {
        Target = target;
        Offset = offset;
        Length = length;
    }

    /// <summary>Constructs a stack/inline fat reference.</summary>
    public LenRef(nint stackAddress, nint length)
    {
        Target = null;
        Offset = stackAddress;
        Length = length;
    }
}
