using System.Runtime.CompilerServices;
using r2CsRuntime.Internal;

namespace r2CsRuntime;

/// <summary>
/// Represents a Rust fat reference to a trait object (<c>&amp;dyn Trait</c>
/// or <c>&amp;mut dyn Trait</c>).
///
/// <para><strong>Memory model</strong></para>
/// <c>DynRef&lt;TVtable&gt; = (object? Target, nint Offset, TVtable Vtable)</c>
/// <list type="table">
///   <listheader><term>Field</term><description>Meaning</description></listheader>
///   <item>
///     <term><see cref="Target"/> / <see cref="Offset"/></term>
///     <description>Same as <see cref="Ref{T}"/> — identifies the data location.</description>
///   </item>
///   <item>
///     <term><see cref="Vtable"/></term>
///     <description>
///       An instance of the <em>trait-static-interface</em> struct (prefixed
///       <c>S_</c> per naming convention) that carries all vtable entries.
///       Because <typeparamref name="TVtable"/> is a zero-sized struct whose
///       methods are monomorphised by the JIT, virtual-dispatch overhead
///       is eliminated ("JIT monopolization").
///     </description>
///   </item>
/// </list>
///
/// <para><strong>Dispatch pattern</strong></para>
/// To call a method through a <c>DynRef</c>, generated code casts the
/// erased <see cref="Ref{Void}"/> data pointer to the concrete type and
/// invokes the vtable method:
/// <code>
/// // vtable method signature (conceptual):
/// void S_Foo_for_s_Bar.m_some_method(Ref&lt;Void&gt; self, ...)
/// {
///     ref s_Bar concrete = ref Unsafe.As&lt;Void, s_Bar&gt;(ref self.AsRef());
///     concrete.m_some_method_impl(...);
/// }
/// </code>
///
/// <para>Design note: <typeparamref name="TVtable"/> is constrained to
/// <c>struct</c> to ensure zero-size allocation and enable JIT devirtualisation.
/// The actual vtable type is the <c>T_</c>-prefixed interface; each
/// <c>S_</c>-prefixed struct for a specific impl carries the method bodies.</para>
/// </summary>
/// <typeparam name="TVtable">
/// The vtable struct type, which implements the <c>T_</c>-prefixed trait interface.
/// </typeparam>
public readonly struct DynRef<TVtable> where TVtable : struct
{
    /// <summary>Owning heap object, or <c>null</c> for a stack reference.</summary>
    public readonly object? Target;

    /// <summary>Byte offset (heap) or absolute address (stack) of the data.</summary>
    public readonly nint Offset;

    /// <summary>
    /// Zero-sized vtable struct that implements the trait's static interface.
    /// Call methods on this field to dispatch trait methods.
    /// </summary>
    public readonly TVtable Vtable;

    /// <summary>Constructs a heap dyn-trait reference.</summary>
    public DynRef(object target, nint offset, TVtable vtable)
    {
        Target = target;
        Offset = offset;
        Vtable = vtable;
    }

    /// <summary>Constructs a stack dyn-trait reference.</summary>
    public DynRef(nint stackAddress, TVtable vtable)
    {
        Target = null;
        Offset = stackAddress;
        Vtable = vtable;
    }

    /// <summary>
    /// Returns the data location as an erased <see cref="Ref{Void}"/>.
    /// Vtable method implementations reinterpret-cast this to their concrete
    /// type with <c>Unsafe.As&lt;Void, s_ConcreteType&gt;(ref dataRef.AsRef())</c>.
    /// </summary>
    public Ref<Void> AsVoidRef() =>
        Target == null
            ? new Ref<Void>(Offset)
            : new Ref<Void>(Target!, Offset);

    /// <summary>
    /// Reinterprets the data as a <see cref="Ref{T}"/> to a specific concrete
    /// type. Used inside vtable method bodies.
    ///
    /// <para><strong>Unsafe:</strong> the caller must guarantee that the
    /// underlying data is actually of type <typeparamref name="T"/>.</para>
    /// </summary>
    public unsafe ref T AsConcreteRef<T>()
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
}
