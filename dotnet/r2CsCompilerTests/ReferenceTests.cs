/// Tests for the generated C# code from tests/inputs/references.rs.
public class ReferenceTests
{
    [Fact]
    public unsafe void Read_ref()
    {
        int x = 42;
        Assert.Equal(42, mod_references.m_read_ref(&x));
    }

    [Fact]
    public unsafe void Add_by_ref()
    {
        int a = 3, b = 4;
        Assert.Equal(7, mod_references.m_add_by_ref(&a, &b));
    }

    [Fact]
    public unsafe void Write_ref()
    {
        int x = 10;
        mod_references.m_write_ref(&x, 99);
        Assert.Equal(99, x);
    }

    [Fact]
    public unsafe void Increment()
    {
        int x = 5;
        mod_references.m_increment(&x);
        Assert.Equal(6, x);
    }

    [Fact]
    public unsafe void Double_in_place()
    {
        int x = 3;
        mod_references.m_double_in_place(&x);
        Assert.Equal(6, x);
    }

    [Fact]
    public unsafe void Swap()
    {
        int a = 10, b = 20;
        mod_references.m_swap(&a, &b);
        Assert.Equal(20, a);
        Assert.Equal(10, b);
    }

    [Fact]
    public unsafe void Sum_via_ref()
    {
        int a = 5, b = 6;
        Assert.Equal(11, mod_references.m_sum_via_ref(&a, &b));
    }
}
