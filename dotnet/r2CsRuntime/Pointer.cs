using System.Runtime.CompilerServices;
using r2CsRuntime.Internal;

namespace r2CsRuntime;

/// <summary>
/// Represents a Rust raw thin pointer (<c>*const T</c> or <c>*mut T</c>).
///
/// Internal representation is identical to <see cref="Ref{T}"/> —
/// <c>(object? Target, nint Offset)</c> — but the type is distinct to
/// reflect the semantic difference: raw pointers have no borrow-checking
/// guarantees and all dereferences are implicitly unsafe.
///
/// <para>Restriction: only <em>strict-provenance</em> operations are
/// permitted. The following Rust APIs are not supported because they would
/// require exposing integer addresses to user code:
/// <c>expose_addr</c>, <c>with_addr</c>.</para>
/// </summary>
/// <typeparam name="T">The pointee type.</typeparam>
public readonly struct Pointer<T>
{
    public readonly object? Target;
    public readonly nint Offset;

    /// <summary>Constructs a heap pointer.</summary>
    public Pointer(object target, nint offset)
    {
        Target = target;
        Offset = offset;
    }

    /// <summary>Constructs a stack / absolute-address pointer.</summary>
    public Pointer(nint address)
    {
        Target = null;
        Offset = address;
    }

    /// <summary>
    /// Dereferences the pointer. The caller is responsible for validity.
    /// </summary>
    public unsafe ref T Deref()
    {
        if (Target == null)
        {
#pragma warning disable CS8500
            return ref *(T*)Offset;
#pragma warning restore CS8500
        }
        ref byte dataStart = ref RawData.GetDataRef(Target!);
        return ref Unsafe.As<byte, T>(ref Unsafe.AddByteOffset(ref dataStart, Offset));
    }

    /// <summary>Converts this raw pointer to a <see cref="Ref{T}"/>.</summary>
    public Ref<T> AsRef() => Target == null ? new Ref<T>(Offset) : new Ref<T>(Target!, Offset);

    /// <summary>
    /// Advances the pointer by <paramref name="count"/> elements.
    /// Equivalent to Rust's <c>ptr.add(count)</c>.
    /// </summary>
    public Pointer<T> Add(nint count)
    {
        nint newOffset = Offset + count * Unsafe.SizeOf<T>();
        return Target == null ? new Pointer<T>(newOffset) : new Pointer<T>(Target!, newOffset);
    }

    /// <summary>
    /// Returns whether the pointer is null (i.e., <see cref="Target"/> is null
    /// and <see cref="Offset"/> is zero).
    /// </summary>
    public bool IsNull() => Target == null && Offset == 0;

    /// <summary>A null pointer constant.</summary>
    public static Pointer<T> Null => new Pointer<T>(0);
}

/// <summary>
/// Represents a Rust raw fat pointer to a slice
/// (<c>*const [T]</c> or <c>*mut [T]</c>).
///
/// Internal representation is identical to <see cref="LenRef{T}"/>.
/// </summary>
/// <typeparam name="T">The element type.</typeparam>
public readonly struct LenPointer<T>
{
    public readonly object? Target;
    public readonly nint Offset;
    public readonly nint Length;

    public LenPointer(object target, nint offset, nint length)
    {
        Target = target;
        Offset = offset;
        Length = length;
    }

    public LenPointer(nint address, nint length)
    {
        Target = null;
        Offset = address;
        Length = length;
    }

    /// <summary>Dereferences element at <paramref name="index"/>.</summary>
    public unsafe ref T Deref(nint index)
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

    /// <summary>Converts this raw pointer to a <see cref="LenRef{T}"/>.</summary>
    public LenRef<T> AsRef() =>
        Target == null
            ? new LenRef<T>(Offset, Length)
            : new LenRef<T>(Target!, Offset, Length);

    /// <summary>A null pointer constant.</summary>
    public static LenPointer<T> Null => new LenPointer<T>(0, 0);
    // Note: AsSpan() is intentionally omitted — see LenRef<T> for the same
    // rationale (MemoryMarshal.CreateSpan not exposed by netstandard2.0
    // reference assemblies).
}

/// <summary>
/// Represents a Rust raw fat pointer to a trait object
/// (<c>*const dyn Trait</c> or <c>*mut dyn Trait</c>).
///
/// Internal representation is identical to <see cref="DynRef{TVtable}"/>.
/// </summary>
/// <typeparam name="TVtable">The vtable struct type.</typeparam>
public readonly struct DynPointer<TVtable> where TVtable : struct
{
    public readonly object? Target;
    public readonly nint Offset;
    public readonly TVtable Vtable;

    public DynPointer(object target, nint offset, TVtable vtable)
    {
        Target = target;
        Offset = offset;
        Vtable = vtable;
    }

    public DynPointer(nint address, TVtable vtable)
    {
        Target = null;
        Offset = address;
        Vtable = vtable;
    }

    /// <summary>
    /// Returns the data as an erased <see cref="Pointer{Void}"/>.
    /// </summary>
    public Pointer<Void> AsVoidPointer() =>
        Target == null ? new Pointer<Void>(Offset) : new Pointer<Void>(Target!, Offset);

    /// <summary>Converts this raw pointer to a <see cref="DynRef{TVtable}"/>.</summary>
    public DynRef<TVtable> AsRef() =>
        Target == null
            ? new DynRef<TVtable>(Offset, Vtable)
            : new DynRef<TVtable>(Target!, Offset, Vtable);
}
