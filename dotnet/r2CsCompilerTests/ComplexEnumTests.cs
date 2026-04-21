/// Tests for the generated C# code from tests/inputs/complex_enums.rs.
public class ComplexEnumTests
{
    [Fact]
    public void Area_circle() {
        var c = mod_complex_enums.m_make_circle(5);
        Assert.Equal(25, mod_complex_enums.m_area_x2(c));  // 5*5
    }

    [Fact]
    public void Area_rect() {
        var r = mod_complex_enums.m_make_rect(3, 4);
        Assert.Equal(12, mod_complex_enums.m_area_x2(r));  // 3*4
    }

    [Fact]
    public void Is_square_true() {
        var r = mod_complex_enums.m_make_rect(4, 4);
        Assert.True(mod_complex_enums.m_is_square(r));
    }

    [Fact]
    public void Is_square_false() {
        var r = mod_complex_enums.m_make_rect(3, 4);
        Assert.False(mod_complex_enums.m_is_square(r));
    }

    [Fact]
    public void Is_square_circle() {
        var c = mod_complex_enums.m_make_circle(5);
        Assert.False(mod_complex_enums.m_is_square(c));
    }

    [Fact]
    public void Opposite_north_is_south() {
        var n = new mod_complex_enums.s_Direction { f_discriminant = 0 }; // North
        var s = mod_complex_enums.m_opposite(n);
        Assert.Equal((byte)1, s.f_discriminant); // South
    }

    [Fact]
    public void Opposite_east_is_west() {
        var e = new mod_complex_enums.s_Direction { f_discriminant = 2 }; // East
        var w = mod_complex_enums.m_opposite(e);
        Assert.Equal((byte)3, w.f_discriminant); // West
    }

    [Fact]
    public void Is_north_true() {
        var n = new mod_complex_enums.s_Direction { f_discriminant = 0 };
        Assert.True(mod_complex_enums.m_is_north(n));
    }

    [Fact]
    public void Is_north_false() {
        var s = new mod_complex_enums.s_Direction { f_discriminant = 1 };
        Assert.False(mod_complex_enums.m_is_north(s));
    }
}
