using System.Runtime.CompilerServices;
using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for <see cref="DynRef{TVtable}"/>.
///
/// Demonstrates the dyn-dispatch pattern:
/// <list type="bullet">
///   <item><c>T_Greet</c> — the <c>T_</c>-prefixed trait interface.</item>
///   <item><c>S_Greet_for_s_Point</c> — the <c>S_</c>-prefixed vtable struct.</item>
///   <item><c>DynRef&lt;T_Greet&gt;</c> — the fat pointer (Target+Offset+Vtable = 3 ptrs).</item>
///   <item>Method call: <c>r.Vtable.m_greet(r)</c> — pass the full DynRef as self.</item>
/// </list>
/// </summary>
public class DynRefTests
{
    // ── Trait interface (T_ prefix) ───────────────────────────────────────

    private interface T_Greet
    {
        string m_greet(DynRef<T_Greet> self);
    }

    // ── Concrete data ─────────────────────────────────────────────────────

    private sealed class s_Point_container
    {
        public int f_x;
        public int f_y;
    }

    // ── Vtable struct (S_ prefix, one per impl) ───────────────────────────

    private struct S_Greet_for_s_Point : T_Greet
    {
        public string m_greet(DynRef<T_Greet> self)
        {
            ref int x = ref self.AsConcreteRef<int>();
            return $"({x})";
        }
    }

    // ── Tests ──────────────────────────────────────────────────────────────

    [Fact]
    public void DynRef_HeapDispatch_CallsVtableMethod()
    {
        var obj = new s_Point_container { f_x = 3, f_y = 4 };
        var r   = RefHelper.FromHeapField(obj, ref obj.f_x);
        var dyn = new DynRef<T_Greet>(obj, r.Offset, new S_Greet_for_s_Point());

        string result = dyn.Vtable.m_greet(dyn);
        Assert.Equal("(3)", result);
    }

    [Fact]
    public void DynRef_AsVoidRef_HasSameTargetAndOffset()
    {
        var obj = new s_Point_container { f_x = 1, f_y = 2 };
        var r   = RefHelper.FromHeapField(obj, ref obj.f_x);
        var dyn = new DynRef<T_Greet>(obj, r.Offset, new S_Greet_for_s_Point());

        var voidRef = dyn.AsVoidRef();
        Assert.Same(obj, voidRef.Target);
        Assert.Equal(r.Offset, voidRef.Offset);
    }

    [Fact]
    public void DynRef_AsConcreteRef_AliasesField()
    {
        var obj = new s_Point_container { f_x = 7, f_y = 8 };
        var r   = RefHelper.FromHeapField(obj, ref obj.f_x);
        var dyn = new DynRef<T_Greet>(obj, r.Offset, new S_Greet_for_s_Point());

        ref int concrete = ref dyn.AsConcreteRef<int>();
        Assert.Equal(7, concrete);
        concrete = 99;
        Assert.Equal(99, obj.f_x);
    }

    [Fact]
    public void DynRef_IsThreePointerSizes()
    {
        // DynRef<T_Greet>: Target(ptr) + Offset(nint) + Vtable(interface ref = ptr)
        // = exactly 3 * pointer-size.
        int expected = 3 * IntPtr.Size;
        int actual   = Unsafe.SizeOf<DynRef<T_Greet>>();
        Assert.Equal(expected, actual);
    }
}
