using System.Runtime.CompilerServices;
using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for <see cref="DynRef{TVtable}"/>.
///
/// We use a minimal example that mirrors how the compiler would generate
/// vtable dispatch for a trait object:
/// <code>
/// trait Greet { fn greet(&amp;self) -> String; }
/// impl Greet for Point { fn greet(&amp;self) -> String { ... } }
/// let r: &amp;dyn Greet = &amp;my_point;
/// r.greet();
/// </code>
/// </summary>
public class DynRefTests
{
    // ── Trait interface (T_Greet) ─────────────────────────────────────────

    private interface T_Greet
    {
        string m_greet(Ref<Void> self);
    }

    // ── Concrete type ─────────────────────────────────────────────────────

    private sealed class s_Point_container
    {
        public int f_x;
        public int f_y;
    }

    // ── Vtable struct (S_Greet_for_s_Point) ───────────────────────────────

    private readonly struct S_Greet_for_s_Point : T_Greet
    {
        public string m_greet(Ref<Void> self)
        {
            // Cast the erased void ref to the concrete type.
            ref int x = ref Unsafe.As<Void, int>(ref self.AsRef());
            return $"({x})";
        }
    }

    // ── Tests ──────────────────────────────────────────────────────────────

    [Fact]
    public void DynRef_HeapDispatch_CallsVtableMethod()
    {
        var obj = new s_Point_container { f_x = 3, f_y = 4 };
        var r = RefHelper.FromHeapField(obj, ref obj.f_x);
        var vtable = new S_Greet_for_s_Point();
        var dyn = new DynRef<S_Greet_for_s_Point>(obj, r.Offset, vtable);

        string result = dyn.Vtable.m_greet(dyn.AsVoidRef());
        Assert.Equal("(3)", result);
    }

    [Fact]
    public void DynRef_AsVoidRef_HasSameTargetAndOffset()
    {
        var obj = new s_Point_container { f_x = 1, f_y = 2 };
        var r = RefHelper.FromHeapField(obj, ref obj.f_x);
        var dyn = new DynRef<S_Greet_for_s_Point>(obj, r.Offset, new S_Greet_for_s_Point());

        var voidRef = dyn.AsVoidRef();
        Assert.Same(obj, voidRef.Target);
        Assert.Equal(r.Offset, voidRef.Offset);
    }

    [Fact]
    public void DynRef_AsConcreteRef_AliasesField()
    {
        var obj = new s_Point_container { f_x = 7, f_y = 8 };
        var r = RefHelper.FromHeapField(obj, ref obj.f_x);
        var dyn = new DynRef<S_Greet_for_s_Point>(obj, r.Offset, default);

        ref int concrete = ref dyn.AsConcreteRef<int>();
        Assert.Equal(7, concrete);
        concrete = 99;
        Assert.Equal(99, obj.f_x);
    }

    [Fact]
    public void DynRef_VtableStruct_HasMinimalSize()
    {
        // Design intent: vtable structs should be zero-sized so that
        // DynRef<TVtable> has no allocation overhead.
        //
        // DESIGN ISSUE FOUND: C# structs always have a minimum size of 1 byte
        // (even empty structs), unlike Rust where zero-sized types are truly
        // zero-sized.  Unsafe.SizeOf<S_Greet_for_s_Point>() returns 1, not 0.
        // This means DynRef<TVtable> is 1 byte larger than intended.
        //
        // Mitigation: The JIT still devirtualises calls through the vtable
        // struct (because it can inline default(S_Greet_for_s_Point)) so the
        // "zero call cost" goal is still achieved, even though the struct
        // itself occupies 1 byte in memory.
        //
        // Update: StructLayout(LayoutKind.Sequential, Size = 0) can force a
        // zero-size layout in .NET 8+, but this is not available for all
        // target frameworks. See design-v1.md notes section.
        Assert.True(Unsafe.SizeOf<S_Greet_for_s_Point>() <= 1,
            $"Vtable struct should be at most 1 byte; got {Unsafe.SizeOf<S_Greet_for_s_Point>()}");
    }
}
