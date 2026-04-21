/// Tests for the generated C# code from tests/inputs/structs.rs.
public class StructTests
{
    [Fact]
    public void Point_new()
    {
        var p = mod_structs.s_Point.m_new(3, 4);
        Assert.Equal(3, p.f_x);
        Assert.Equal(4, p.f_y);
    }

    [Fact]
    public void Point_x_accessor()
    {
        var p = mod_structs.s_Point.m_new(7, 2);
        Assert.Equal(7, mod_structs.s_Point.m_x(p));
    }

    [Fact]
    public void Point_y_accessor()
    {
        var p = mod_structs.s_Point.m_new(7, 2);
        Assert.Equal(2, mod_structs.s_Point.m_y(p));
    }

    [Fact]
    public void Point_distance_squared()
    {
        var p = mod_structs.s_Point.m_new(3, 4);
        Assert.Equal(25, mod_structs.s_Point.m_distance_squared(p));
    }

    [Fact]
    public void Point_translate()
    {
        var p = mod_structs.s_Point.m_new(1, 2);
        var q = mod_structs.s_Point.m_translate(p, 3, 4);
        Assert.Equal(4, q.f_x);
        Assert.Equal(6, q.f_y);
    }

    [Fact]
    public void Point_scale()
    {
        var p = mod_structs.s_Point.m_new(2, 3);
        var q = mod_structs.s_Point.m_scale(p, 5);
        Assert.Equal(10, q.f_x);
        Assert.Equal(15, q.f_y);
    }

    [Fact]
    public void Rectangle_area()
    {
        var r = mod_structs.s_Rectangle.m_new(4, 5);
        Assert.Equal(20, mod_structs.s_Rectangle.m_area(r));
    }

    [Fact]
    public void Rectangle_perimeter()
    {
        var r = mod_structs.s_Rectangle.m_new(3, 7);
        Assert.Equal(20, mod_structs.s_Rectangle.m_perimeter(r));
    }

    [Fact]
    public void Rectangle_is_square_true()
    {
        var r = mod_structs.s_Rectangle.m_new(4, 4);
        Assert.True(mod_structs.s_Rectangle.m_is_square(r));
    }

    [Fact]
    public void Rectangle_is_square_false()
    {
        var r = mod_structs.s_Rectangle.m_new(3, 4);
        Assert.False(mod_structs.s_Rectangle.m_is_square(r));
    }
}
