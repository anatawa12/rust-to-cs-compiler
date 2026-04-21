/// Tests for the generated C# code from tests/inputs/generics.rs.
public class GenericsTests
{
    [Fact]
    public void Make_pair_first() =>
        Assert.Equal(3, mod_generics.m_get_first(mod_generics.m_make_pair(3, 100L)));

    [Fact]
    public void Make_pair_second() =>
        Assert.Equal(100L, mod_generics.m_get_second(mod_generics.m_make_pair(3, 100L)));

    [Fact]
    public void Swap_ints_first() {
        var p = mod_generics.m_make_pair(1, 2L);
        var swapped = mod_generics.m_swap_ints(p);
        Assert.Equal(2L, swapped.f_first);
    }

    [Fact]
    public void Swap_ints_second() {
        var p = mod_generics.m_make_pair(1, 2L);
        var swapped = mod_generics.m_swap_ints(p);
        Assert.Equal(1, swapped.f_second);
    }

    [Fact]
    public void Origin_is_zero() {
        var o = mod_generics.m_origin();
        Assert.Equal(0, o.f_x);
        Assert.Equal(0, o.f_y);
    }

    [Fact]
    public void Distance_squared_3_4() {
        var p = new mod_generics.s_Point { f_x = 3, f_y = 4 };
        Assert.Equal(25L, mod_generics.m_distance_squared(p));
    }
}
