using System.Runtime.CompilerServices;
using r2CsRuntime.Internal;

namespace r2CsRuntime;

/// <summary>
/// Represents a Rust reference (<c>&amp;T</c> or <c>&amp;mut T</c>) that may
/// point into either a heap object or a stack location.
///
/// <para><strong>Memory model</strong></para>
/// <list type="table">
///   <listheader><term>Case</term><description>Encoding</description></listheader>
///   <item>
///     <term>Heap field</term>
///     <description>
///       <see cref="Target"/> = the owning managed object;<br/>
///       <see cref="Offset"/> = byte offset from the object's first data byte
///       to the field (computed with <see cref="Unsafe.ByteOffset{T}"/>).
///     </description>
///   </item>
///   <item>
///     <term>Stack variable</term>
///     <description>
///       <see cref="Target"/> = <c>null</c>;<br/>
///       <see cref="Offset"/> = absolute (pinned) stack address.
///       Valid only while the originating stack frame is live and the
///       variable has not moved.  The compiler enforces that stack refs do
///       not cross <c>await</c> points, are not closure-captured, and do
///       not escape the function via a return value.
///     </description>
///   </item>
/// </list>
///
/// <para>GC safety: when <see cref="Target"/> is non-null the GC tracks the
/// object reference and updates it on collection; <see cref="Offset"/> is a
/// relative byte distance that is independent of the object's location, so
/// both fields remain valid after a GC compaction.</para>
///
/// <para>Mutability is not encoded in this struct; the Rust type system
/// enforces the <c>&amp;T</c> vs <c>&amp;mut T</c> distinction before
/// code generation.</para>
/// </summary>
/// <typeparam name="T">The referent type.</typeparam>
public readonly struct Ref<T>
{
    /// <summary>
    /// The owning heap object, or <c>null</c> for a stack reference.
    /// </summary>
    public readonly object? Target;

    /// <summary>
    /// For heap refs: byte offset from the object's first data byte to the
    /// referenced field.<br/>
    /// For stack refs: absolute address of the value on the stack.
    /// </summary>
    public readonly nint Offset;

    /// <summary>Constructs a heap reference.</summary>
    public Ref(object target, nint offset)
    {
        Target = target;
        Offset = offset;
    }

    /// <summary>Constructs a stack reference (target = null, offset = address).</summary>
    public Ref(nint stackAddress)
    {
        Target = null;
        Offset = stackAddress;
    }

    /// <summary>
    /// Returns a managed <c>ref T</c> to the referenced location.
    /// Both reads and writes go through the same managed reference.
    /// </summary>
    public unsafe ref T AsRef()
    {
        if (Target == null)
        {
            // Stack reference: Offset is an absolute pointer.
            // CS8500: pointer to a managed type is intentional — stack refs are only
            // created for locals whose lifetime is provably contained within the
            // current stack frame (no await crossing, no closure capture, no escape).
#pragma warning disable CS8500
            return ref *(T*)Offset;
#pragma warning restore CS8500
        }

        // Heap reference: reconstruct a managed ref from the object + byte offset.
        ref byte dataStart = ref RawData.GetDataRef(Target!);
        return ref Unsafe.As<byte, T>(ref Unsafe.AddByteOffset(ref dataStart, Offset));
    }

    /// <summary>Reads the referenced value.</summary>
    public T Get() => AsRef();

    /// <summary>Writes a value to the referenced location.</summary>
    public void Set(T value) => AsRef() = value;
}

/// <summary>
/// Static helpers for creating <see cref="Ref{T}"/> values.
/// These are used by generated code and by tests; they are not needed at
/// the Rust source level.
/// </summary>
public static class RefHelper
{
    /// <summary>
    /// Creates a <see cref="Ref{T}"/> that points to <paramref name="field"/>
    /// inside the heap object <paramref name="owner"/>.
    ///
    /// <para>Example (generated code pattern):
    /// <code>
    /// var myVar = new s_MyStruct { f_x = 1 };
    /// var r = RefHelper.FromHeapField(myVar, ref myVar.f_x);
    /// // r.AsRef() == ref myVar.f_x
    /// </code>
    /// </para>
    /// </summary>
    /// <typeparam name="TOwner">Heap object type (must be a reference type).</typeparam>
    /// <typeparam name="T">Field type.</typeparam>
    /// <param name="owner">The owning object.</param>
    /// <param name="field">A managed reference to the specific field within <paramref name="owner"/>.</param>
    public static Ref<T> FromHeapField<TOwner, T>(TOwner owner, ref T field)
        where TOwner : class
    {
        ref byte dataStart = ref RawData.GetDataRef(owner);
        nint offset = Unsafe.ByteOffset(
            ref dataStart,
            ref Unsafe.As<T, byte>(ref field));
        return new Ref<T>(owner, offset);
    }

    /// <summary>
    /// Creates a <see cref="Ref{T}"/> that points to the
    /// <see cref="Var{T}.f_value"/> field of a <see cref="Var{T}"/>.
    /// This is the most common way stack-borrow-escaped variables are accessed.
    /// </summary>
    public static Ref<T> FromVar<T>(Var<T> var)
        => FromHeapField(var, ref var.f_value);

    /// <summary>
    /// Creates a <see cref="LenRef{T}"/> pointing to <paramref name="firstElement"/>
    /// inside the heap object <paramref name="owner"/>, with the given
    /// <paramref name="length"/>.
    ///
    /// <para>Use <see cref="FromHeapSlice{TOwner,T}"/> for plain slices (<c>&amp;[T]</c>)
    /// and this method for DST-struct fat references where <typeparamref name="TDst"/>
    /// is the outer struct type.</para>
    /// </summary>
    public static LenRef<TDst> FromHeapLenRef<TOwner, TDst>(TOwner owner, ref TDst startField, nint length)
        where TOwner : class
    {
        ref byte dataStart = ref RawData.GetDataRef(owner);
        nint offset = Unsafe.ByteOffset(
            ref dataStart,
            ref Unsafe.As<TDst, byte>(ref startField));
        return new LenRef<TDst>(owner, offset, length);
    }

    /// <summary>
    /// Creates a <c>LenRef&lt;Slice&lt;T&gt;&gt;</c> that represents a <c>&amp;[T]</c>
    /// slice starting at <paramref name="firstElement"/> inside the heap object
    /// <paramref name="owner"/>.
    ///
    /// <para>Typically used when taking a slice of a heap-allocated array field
    /// or an inline array field in a struct.</para>
    ///
    /// <para>Example (generated code pattern):
    /// <code>
    /// var r = RefHelper.FromHeapSlice(obj, ref obj.f_arr_0, length: 3);
    /// r.GetElement(1) = 42;
    /// </code>
    /// </para>
    /// </summary>
    public static LenRef<Slice<T>> FromHeapSlice<TOwner, T>(TOwner owner, ref T firstElement, nint length)
        where TOwner : class
    {
        ref byte dataStart = ref RawData.GetDataRef(owner);
        nint offset = Unsafe.ByteOffset(
            ref dataStart,
            ref Unsafe.As<T, byte>(ref firstElement));
        return new LenRef<Slice<T>>(owner, offset, length);
    }

    /// <summary>
    /// Creates a stack <see cref="Ref{T}"/> from a pinned pointer.
    /// <para><strong>Unsafe:</strong> the caller must ensure the variable's
    /// address remains valid for the lifetime of the returned ref.
    /// Use a <c>fixed</c> block or <c>Unsafe.AsPointer</c> to obtain the
    /// pointer.</para>
    /// </summary>
    // CS8500: void* is used instead of T* to avoid the managed-type-pointer
    // warning; the caller is responsible for passing a correctly-typed address.
    public static unsafe Ref<T> FromStackPointer<T>(void* ptr)
        => new Ref<T>((nint)ptr);
}
