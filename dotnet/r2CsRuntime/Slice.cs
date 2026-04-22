using System.Runtime.CompilerServices;
using r2CsRuntime.Internal;

namespace r2CsRuntime;

/// <summary>
/// Marker type representing the Rust unsized slice type <c>[T]</c>.
///
/// <para>Use <c>LenRef&lt;Slice&lt;T&gt;&gt;</c> to represent <c>&amp;[T]</c> or
/// <c>&amp;mut [T]</c>.  Element access is provided by the
/// <see cref="SliceExt"/> extension class.</para>
/// </summary>
/// <typeparam name="T">The element type.</typeparam>
public readonly struct Slice<T> { }

/// <summary>
/// Extension methods for <c>LenRef&lt;Slice&lt;T&gt;&gt;</c> and
/// <c>LenPointer&lt;Slice&lt;T&gt;&gt;</c>.
///
/// These operations are only meaningful when the fat reference points to a
/// plain slice (<c>[T]</c>), not to an arbitrary DST struct.
/// </summary>
public static class SliceExt
{
    // ── LenRef<Slice<T>> ──────────────────────────────────────────────────

    /// <summary>
    /// Returns a managed <c>ref T</c> to the element at <paramref name="index"/>.
    /// No bounds checking is performed.
    /// </summary>
    public static unsafe ref T GetElement<T>(this LenRef<Slice<T>> slice, nint index)
    {
        nint elementOffset = slice.Offset + index * Unsafe.SizeOf<T>();
        if (slice.Target == null)
        {
#pragma warning disable CS8500
            return ref *(T*)elementOffset;
#pragma warning restore CS8500
        }
        ref byte dataStart = ref RawData.GetDataRef(slice.Target!);
        return ref Unsafe.As<byte, T>(ref Unsafe.AddByteOffset(ref dataStart, elementOffset));
    }

    /// <summary>
    /// Returns a <see cref="Ref{T}"/> to the element at <paramref name="index"/>.
    /// </summary>
    public static unsafe Ref<T> GetElementRef<T>(this LenRef<Slice<T>> slice, nint index)
    {
        nint elementOffset = slice.Offset + index * Unsafe.SizeOf<T>();
        if (slice.Target == null)
            return new Ref<T>(elementOffset);
        return new Ref<T>(slice.Target!, elementOffset);
    }

    /// <summary>
    /// Returns a sub-slice (no copy).  Equivalent to Rust's <c>&amp;slice[start..start+length]</c>.
    /// </summary>
    public static LenRef<Slice<T>> SliceRange<T>(this LenRef<Slice<T>> slice, nint start, nint length)
    {
        nint newOffset = slice.Offset + start * Unsafe.SizeOf<T>();
        if (slice.Target == null)
            return new LenRef<Slice<T>>(newOffset, length);
        return new LenRef<Slice<T>>(slice.Target!, newOffset, length);
    }

    // ── LenPointer<Slice<T>> ──────────────────────────────────────────────

    /// <summary>
    /// Dereferences the element at <paramref name="index"/>.
    /// </summary>
    public static unsafe ref T Deref<T>(this LenPointer<Slice<T>> ptr, nint index)
    {
        nint elementOffset = ptr.Offset + index * Unsafe.SizeOf<T>();
        if (ptr.Target == null)
        {
#pragma warning disable CS8500
            return ref *(T*)elementOffset;
#pragma warning restore CS8500
        }
        ref byte dataStart = ref RawData.GetDataRef(ptr.Target!);
        return ref Unsafe.As<byte, T>(ref Unsafe.AddByteOffset(ref dataStart, elementOffset));
    }

    /// <summary>
    /// Converts the raw slice pointer to a <see cref="LenRef{T}"/> fat reference.
    /// </summary>
    public static LenRef<Slice<T>> AsRef<T>(this LenPointer<Slice<T>> ptr) =>
        ptr.Target == null
            ? new LenRef<Slice<T>>(ptr.Offset, ptr.Length)
            : new LenRef<Slice<T>>(ptr.Target!, ptr.Offset, ptr.Length);
}
