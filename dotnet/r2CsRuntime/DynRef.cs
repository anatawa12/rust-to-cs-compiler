using System.Runtime.CompilerServices;
using r2CsRuntime.Internal;

namespace r2CsRuntime;

/// <summary>
/// Represents a Rust fat reference to a trait object (<c>&amp;dyn Trait</c>
/// or <c>&amp;mut dyn Trait</c>).
///
/// <para><strong>Memory model</strong></para>
/// <c>DynRef&lt;TVtable&gt; = (object? Target, nint Offset, TVtable Vtable)</c>
///
/// <list type="table">
///   <listheader><term>Field</term><description>Meaning</description></listheader>
///   <item>
///     <term><see cref="Target"/> / <see cref="Offset"/></term>
///     <description>Same as <see cref="Ref{T}"/> — identifies the data location.</description>
///   </item>
///   <item>
///     <term><see cref="Vtable"/></term>
///     <description>
///       A reference to a <c>T_</c>-prefixed trait interface instance (the vtable).
///       Each concrete <c>S_</c>-prefixed vtable struct implements the <c>T_</c>
///       interface and is stored here (boxed as the interface type).
///       This makes <c>DynRef&lt;TVtable&gt;</c> exactly 3 pointer-sizes:
///       Target (ptr) + Offset (ptr/nint) + Vtable (interface ref = ptr).
///     </description>
///   </item>
/// </list>
///
/// <para><strong>Dispatch pattern</strong></para>
/// T_ interface methods receive the full <c>DynRef&lt;T_Trait&gt;</c> as their
/// self parameter, because the caller only holds the fat pointer and does not
/// know the concrete underlying type.  The vtable method uses
/// <see cref="AsConcreteRef{T}"/> to recover the concrete data reference.
/// <code>
/// // Trait interface (T_ prefix):
/// interface T_Greet { string m_greet(DynRef&lt;T_Greet&gt; self); }
///
/// // Vtable struct (S_ prefix), one per impl:
/// struct S_Greet_for_s_Point : T_Greet {
///     string T_Greet.m_greet(DynRef&lt;T_Greet&gt; self) {
///         ref s_Point p = ref self.AsConcreteRef&lt;s_Point&gt;();
///         return p.m_greet_impl();
///     }
/// }
///
/// // Creating and calling:
/// DynRef&lt;T_Greet&gt; r = new(obj, offset, new S_Greet_for_s_Point());
/// string result = r.Vtable.m_greet(r);   // pass the full DynRef as self
/// </code>
/// </summary>
/// <typeparam name="TVtable">
/// The <c>T_</c>-prefixed trait interface type (e.g. <c>T_Greet</c>).
/// </typeparam>
public readonly struct DynRef<TVtable>
{
    /// <summary>Owning heap object, or <c>null</c> for a stack reference.</summary>
    public readonly object? Target;

    /// <summary>Byte offset (heap) or absolute address (stack) of the data.</summary>
    public readonly nint Offset;

    /// <summary>
    /// Reference to the vtable interface instance (an <c>S_</c>-prefixed struct
    /// boxed as a <c>T_</c>-prefixed interface).
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
    /// Vtable method implementations reinterpret-cast this to their concrete type.
    /// </summary>
    public Ref<Void> AsVoidRef() =>
        Target == null
            ? new Ref<Void>(Offset)
            : new Ref<Void>(Target!, Offset);

    /// <summary>
    /// Reinterprets the data as a <see cref="Ref{T}"/> to a specific concrete type.
    /// Used inside vtable method bodies.
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
