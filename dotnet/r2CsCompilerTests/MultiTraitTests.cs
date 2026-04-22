/// Tests for the generated C# code from tests/inputs/multi_traits.rs.
public class MultiTraitTests
{
    [Fact]
    public void Circle_measure_3() {
        var c = new mod_multi_traits.s_Circle { f_radius = 3 };
        Assert.Equal(9L, mod_multi_traits.m_circle_measure(c));
    }

    [Fact]
    public void Square_measure_4() {
        var s = new mod_multi_traits.s_Square { f_side = 4 };
        Assert.Equal(16L, mod_multi_traits.m_square_measure(s));
    }

    [Fact]
    public void Scaled_circle_2x() {
        var c = new mod_multi_traits.s_Circle { f_radius = 3 };
        Assert.Equal(36L, mod_multi_traits.m_scaled_circle_measure(c, 2)); // (3*2)^2 = 36
    }

    [Fact]
    public void Scaled_square_3x() {
        var s = new mod_multi_traits.s_Square { f_side = 2 };
        Assert.Equal(36L, mod_multi_traits.m_scaled_square_measure(s, 3)); // (2*3)^2 = 36
    }

    [Fact]
    public void Compare_areas_circle_bigger() {
        var c = new mod_multi_traits.s_Circle { f_radius = 5 };  // area = 25
        var s = new mod_multi_traits.s_Square { f_side = 4 };    // area = 16
        Assert.True(mod_multi_traits.m_compare_areas(c, s));
    }

    [Fact]
    public void Compare_areas_square_bigger() {
        var c = new mod_multi_traits.s_Circle { f_radius = 3 };  // area = 9
        var s = new mod_multi_traits.s_Square { f_side = 4 };    // area = 16
        Assert.False(mod_multi_traits.m_compare_areas(c, s));
    }
}
