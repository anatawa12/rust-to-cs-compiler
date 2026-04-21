using System.Runtime.CompilerServices;
using r2CsRuntime.Internal;

namespace r2CsRuntime;

/// <summary>
/// Represents a Rust slice reference (<c>&amp;[T]</c> or <c>&amp;mut [T]</c>).
///
/// <para><strong>Memory model</strong></para>
/// <c>LenRef&lt;T&gt; = (object? Target, nint Offset, nint Length)</c>
/// <list type="table">
///   <listheader><term>Case</term><description>Encoding</description></listheader>
///   <item>
///     <term>Heap slice</term>
///     <description>
///       <see cref="Target"/> = owning managed object;<br/>
///       <see cref="Offset"/> = byte offset from object data start to the
///       first element;<br/>
///       <see cref="Length"/> = number of elements.
///     </description>
///   </item>
///   <item>
///     <term>Stack / inline slice</term>
///     <description>
///       <see cref="Target"/> = <c>null</c>;<br/>
///       <see cref="Offset"/> = absolute address of the first element;<br/>
///       <see cref="Length"/> = number of elements.
///     </description>
///   </item>
/// </list>
///
/// Slices where the last field of a struct is a slice (<c>[T]</c> unsized
/// tail) follow the same encoding with <see cref="Target"/> pointing to the
/// containing struct object.
/// </summary>
/// <typeparam name="T">The element type.</typeparam>
public readonly struct LenRef<T>
{
    /// <summary>Owning heap object, or <c>null</c> for a stack slice.</summary>
    public readonly object? Target;

    /// <summary>
    /// Byte offset from the object's first data byte to element [0], or the
    /// absolute stack address of element [0] when <see cref="Target"/> is <c>null</c>.
    /// </summary>
    public readonly nint Offset;

    /// <summary>Number of elements in the slice.</summary>
    public readonly nint Length;

    /// <summary>Constructs a heap slice reference.</summary>
    public LenRef(object target, nint offset, nint length)
    {
        Target = target;
        Offset = offset;
        Length = length;
    }

    /// <summary>Constructs a stack slice reference.</summary>
    public LenRef(nint stackAddress, nint length)
    {
        Target = null;
        Offset = stackAddress;
        Length = length;
    }

    /// <summary>
    /// Returns a managed <c>ref T</c> to element at position <paramref name="index"/>.
    /// No bounds checking is performed.
    /// </summary>
    public unsafe ref T GetElement(nint index)
    {
        nint elementOffset = Offset + index * Unsafe.SizeOf<T>();

        if (Target == null)
        {
#pragma warning disable CS8500
            return ref *(T*)elementOffset;
#pragma warning restore CS8500
        }

        ref byte dataStart = ref RawData.GetDataRef(Target!);
        return ref Unsafe.As<byte, T>(ref Unsafe.AddByteOffset(ref dataStart, elementOffset));
    }

    /// <summary>
    /// Returns a <see cref="Ref{T}"/> to element at position <paramref name="index"/>.
    /// </summary>
    public unsafe Ref<T> GetElementRef(nint index)
    {
        nint elementOffset = Offset + index * Unsafe.SizeOf<T>();
        if (Target == null)
            return new Ref<T>(elementOffset);
        return new Ref<T>(Target!, elementOffset);
    }

    /// <summary>
    /// Returns a sub-slice (no copy).  Equivalent to <c>&amp;slice[start..end]</c>.
    /// </summary>
    public LenRef<T> Slice(nint start, nint length)
    {
        nint newOffset = Offset + start * Unsafe.SizeOf<T>();
        if (Target == null)
            return new LenRef<T>(newOffset, length);
        return new LenRef<T>(Target!, newOffset, length);
    }
    // Note: AsSpan() is intentionally omitted. MemoryMarshal.CreateSpan is not
    // available in the netstandard2.0 compile-time surface even though it exists
    // in System.Memory 4.5.5 at runtime (the SDK uses the framework reference
    // assembly which does not expose it). Generated C# code accesses elements
    // via GetElement(index) rather than Span<T>.
}
