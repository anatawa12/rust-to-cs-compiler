using System.Runtime.CompilerServices;
using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for t_FnOnce / t_FnMut / t_Fn (F-bounded, static dispatch)
/// and T_FnOnce / T_FnMut / T_Fn (dyn dispatch via S_ vtable structs).
///
/// Also demonstrates the expected module-nesting shape: generated code wraps
/// types in <c>partial class mod_&lt;module&gt;</c> to mirror Rust's module
/// hierarchy.
/// </summary>
public class FnTests
{
    // ════════════════════════════════════════════════════════════════════════
    // Simulated generated output for:
    //
    //   mod my_module {
    //       struct Accumulator { counter: i32 }
    //       impl FnMut(i32) -> i32 for Accumulator { ... }
    //       struct Multiplier { factor: i32 }
    //       impl Fn(i32) -> i32 for Multiplier { ... }
    //   }
    // ════════════════════════════════════════════════════════════════════════

    /// <summary>Mirrors <c>mod my_module { ... }</c>.</summary>
    public static partial class mod_my_module
    {
        // ── Accumulator (FnMut) ───────────────────────────────────────────

        public struct s_Accumulator : t_FnMut<s_Accumulator, int, int>
        {
            public int f_counter;

            public int m_call_once(int args) => m_call_mut(args);
            public int m_call_mut(int args) { f_counter += args; return f_counter; }
        }

        /// <summary>Vtable struct for <c>impl FnMut(i32) -&gt; i32 for Accumulator</c>.</summary>
        public struct S_Accumulator : T_FnMut<int, int>
        {
            int T_FnOnce<int, int>.m_call_once(DynRef<T_FnOnce<int, int>> self, int args)
            {
                var vtMut = (T_FnMut<int, int>)self.Vtable;
                var asMut = new DynRef<T_FnMut<int, int>>(self.Target!, self.Offset, vtMut);
                return vtMut.m_call_mut(asMut, args);
            }

            int T_FnMut<int, int>.m_call_mut(DynRef<T_FnMut<int, int>> self, int args)
            {
                ref s_Accumulator acc = ref self.AsConcreteRef<s_Accumulator>();
                acc.f_counter += args;
                return acc.f_counter;
            }
        }

        // ── Multiplier (Fn) ───────────────────────────────────────────────

        public struct s_Multiplier : t_Fn<s_Multiplier, int, int>
        {
            public int f_factor;

            public s_Multiplier(int factor) { f_factor = factor; }
            public int m_call_once(int args) => m_call(args);
            public int m_call_mut(int args)  => m_call(args);
            public int m_call(int args)      => f_factor * args;
        }

        /// <summary>Vtable struct for <c>impl Fn(i32) -&gt; i32 for Multiplier</c>.</summary>
        public struct S_Multiplier : T_Fn<int, int>
        {
            int T_FnOnce<int, int>.m_call_once(DynRef<T_FnOnce<int, int>> self, int args)
            {
                var vtFn = (T_Fn<int, int>)self.Vtable;
                var asFn = new DynRef<T_Fn<int, int>>(self.Target!, self.Offset, vtFn);
                return vtFn.m_call(asFn, args);
            }

            int T_FnMut<int, int>.m_call_mut(DynRef<T_FnMut<int, int>> self, int args)
            {
                var vtFn = (T_Fn<int, int>)self.Vtable;
                var asFn = new DynRef<T_Fn<int, int>>(self.Target!, self.Offset, vtFn);
                return vtFn.m_call(asFn, args);
            }

            int T_Fn<int, int>.m_call(DynRef<T_Fn<int, int>> self, int args)
            {
                ref s_Multiplier m = ref self.AsConcreteRef<s_Multiplier>();
                return m.f_factor * args;
            }
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // t_Fn* tests (static / monomorphised dispatch)
    // ════════════════════════════════════════════════════════════════════════

    [Fact]
    public void FnMut_CallMut_AccumulatesState()
    {
        var acc = new mod_my_module.s_Accumulator();
        Assert.Equal(10, acc.m_call_mut(10));
        Assert.Equal(15, acc.m_call_mut(5));
    }

    [Fact]
    public void FnMut_IsAssignableToFnOnce()
    {
        t_FnOnce<mod_my_module.s_Accumulator, int, int> asOnce = new mod_my_module.s_Accumulator();
        Assert.Equal(5, asOnce.m_call_once(5));
    }

    [Fact]
    public void Fn_Call_IsReadOnly()
    {
        var mul = new mod_my_module.s_Multiplier(3);
        Assert.Equal(9,  mul.m_call(3));
        Assert.Equal(12, mul.m_call(4));
        Assert.Equal(9,  mul.m_call(3));
    }

    [Fact]
    public void Fn_ImplementsFullHierarchy()
    {
        var mul = new mod_my_module.s_Multiplier(2);
        Assert.IsAssignableFrom<t_Fn<mod_my_module.s_Multiplier, int, int>>(mul);
        Assert.IsAssignableFrom<t_FnMut<mod_my_module.s_Multiplier, int, int>>(mul);
        Assert.IsAssignableFrom<t_FnOnce<mod_my_module.s_Multiplier, int, int>>(mul);
    }

    // ════════════════════════════════════════════════════════════════════════
    // T_Fn* tests (dyn dispatch through DynRef)
    // ════════════════════════════════════════════════════════════════════════

    [Fact]
    public void DynFnMut_CallMut_AccumulatesState()
    {
        var box = new Var<mod_my_module.s_Accumulator>();
        var r   = RefHelper.FromVar(box);
        var dyn = new DynRef<T_FnMut<int, int>>(box, r.Offset, new mod_my_module.S_Accumulator());

        Assert.Equal(10, dyn.Vtable.m_call_mut(dyn, 10));
        Assert.Equal(15, dyn.Vtable.m_call_mut(dyn, 5));
    }

    [Fact]
    public void DynFn_CallFn_IsReadOnly()
    {
        var box = new Var<mod_my_module.s_Multiplier>();
        box.f_value = new mod_my_module.s_Multiplier(7);
        var r   = RefHelper.FromVar(box);
        var dyn = new DynRef<T_Fn<int, int>>(box, r.Offset, new mod_my_module.S_Multiplier());

        Assert.Equal(14, dyn.Vtable.m_call(dyn, 2));
        Assert.Equal(21, dyn.Vtable.m_call(dyn, 3));
    }

    [Fact]
    public void DynFnMut_IsThreePointerSizes()
    {
        int expected = 3 * IntPtr.Size;
        int actual   = Unsafe.SizeOf<DynRef<T_FnMut<int, int>>>();
        Assert.Equal(expected, actual);
    }
}
