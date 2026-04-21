using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for t_FnOnce / t_FnMut / t_Fn trait interfaces.
///
/// We use a simple class closure to verify the interface hierarchy and
/// method dispatch, mirroring the closure classes the compiler would generate.
/// </summary>
public class FnTests
{
    // ── Minimal closure implementations ────────────────────────────────────

    /// <summary>
    /// FnOnce closure: adds two numbers, consuming the captures.
    /// Implements only t_FnOnce (one-shot call).
    /// </summary>
    private sealed class AddOnce : t_FnOnce<AddOnce, (int, int), int>
    {
        public int m_call_once((int, int) args) => args.Item1 + args.Item2;
    }

    /// <summary>
    /// FnMut closure: accumulates a running total.
    /// Implements t_FnMut (and transitively t_FnOnce).
    /// </summary>
    private sealed class Accumulator : t_FnMut<Accumulator, int, int>
    {
        private int _sum;

        public int m_call_once(int args) => m_call_mut(args);
        public int m_call_mut(int args)
        {
            _sum += args;
            return _sum;
        }
    }

    /// <summary>
    /// Fn closure: multiplies the captured factor with the argument.
    /// Implements t_Fn (and transitively t_FnMut and t_FnOnce).
    /// </summary>
    private sealed class Multiplier : t_Fn<Multiplier, int, int>
    {
        private readonly int _factor;

        public Multiplier(int factor) => _factor = factor;

        public int m_call_once(int args) => m_call(args);
        public int m_call_mut(int args)  => m_call(args);
        public int m_call(int args)      => _factor * args;
    }

    // ── Tests ──────────────────────────────────────────────────────────────

    [Fact]
    public void FnOnce_CallOnce_ReturnsResult()
    {
        var closure = new AddOnce();
        Assert.Equal(7, closure.m_call_once((3, 4)));
    }

    [Fact]
    public void FnMut_CallMut_AccumulatesState()
    {
        var acc = new Accumulator();
        Assert.Equal(10, acc.m_call_mut(10));
        Assert.Equal(15, acc.m_call_mut(5));
        Assert.Equal(16, acc.m_call_mut(1));
    }

    [Fact]
    public void FnMut_IsAssignableToFnOnce()
    {
        t_FnOnce<Accumulator, int, int> asOnce = new Accumulator();
        Assert.Equal(5, asOnce.m_call_once(5));
    }

    [Fact]
    public void Fn_Call_IsReadOnly()
    {
        var mul = new Multiplier(3);
        Assert.Equal(9,  mul.m_call(3));
        Assert.Equal(12, mul.m_call(4));
        // repeated calls give consistent results (no mutation)
        Assert.Equal(9,  mul.m_call(3));
    }

    [Fact]
    public void Fn_IsAssignableToFnMut()
    {
        t_FnMut<Multiplier, int, int> asMut = new Multiplier(5);
        Assert.Equal(25, asMut.m_call_mut(5));
    }

    [Fact]
    public void Fn_IsAssignableToFnOnce()
    {
        t_FnOnce<Multiplier, int, int> asOnce = new Multiplier(7);
        Assert.Equal(14, asOnce.m_call_once(2));
    }

    /// <summary>
    /// Verifies the trait hierarchy: Fn ⊆ FnMut ⊆ FnOnce.
    /// </summary>
    [Fact]
    public void Fn_ImplementsFullHierarchy()
    {
        var mul = new Multiplier(2);
        Assert.IsAssignableFrom<t_Fn<Multiplier, int, int>>(mul);
        Assert.IsAssignableFrom<t_FnMut<Multiplier, int, int>>(mul);
        Assert.IsAssignableFrom<t_FnOnce<Multiplier, int, int>>(mul);
    }
}
