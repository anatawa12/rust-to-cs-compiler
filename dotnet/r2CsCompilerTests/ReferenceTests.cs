using r2CsRuntime;

/// Tests for the generated C# code from tests/inputs/references.rs.
public class ReferenceTests
{
    /// Create a Ref<int> pointing to a heap-allocated Var<int>.
    private static Ref<int> Ref(int value)
    {
        var v = new Var<int>();
        v.f_value = value;
        return RefHelper.FromVar<int>(v);
    }

    [Fact]
    public void Read_ref() =>
        Assert.Equal(42, mod_references.m_read_ref(Ref(42)));

    [Fact]
    public void Add_by_ref() =>
        Assert.Equal(7, mod_references.m_add_by_ref(Ref(3), Ref(4)));

    [Fact]
    public void Write_ref() {
        var v = new Var<int>();
        v.f_value = 10;
        var r = RefHelper.FromVar<int>(v);
        mod_references.m_write_ref(r, 99);
        Assert.Equal(99, r.Get());
    }

    [Fact]
    public void Increment() {
        var v = new Var<int>();
        v.f_value = 5;
        var r = RefHelper.FromVar<int>(v);
        mod_references.m_increment(r);
        Assert.Equal(6, r.Get());
    }

    [Fact]
    public void Double_in_place() {
        var v = new Var<int>();
        v.f_value = 3;
        var r = RefHelper.FromVar<int>(v);
        mod_references.m_double_in_place(r);
        Assert.Equal(6, r.Get());
    }

    [Fact]
    public void Swap() {
        var va = new Var<int>(); va.f_value = 10;
        var vb = new Var<int>(); vb.f_value = 20;
        var ra = RefHelper.FromVar<int>(va);
        var rb = RefHelper.FromVar<int>(vb);
        mod_references.m_swap(ra, rb);
        Assert.Equal(20, ra.Get());
        Assert.Equal(10, rb.Get());
    }

    [Fact]
    public void Sum_via_ref() =>
        Assert.Equal(11, mod_references.m_sum_via_ref(Ref(5), Ref(6)));
}
