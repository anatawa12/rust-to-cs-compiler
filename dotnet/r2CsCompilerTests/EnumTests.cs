/// Tests for the generated C# code from tests/inputs/enums.rs.
public class EnumTests
{
    // Helpers to construct Shape values (inner payload structs are nested inside s_Shape).
    static mod_enums.s_Shape Circle(int radius) =>
        new mod_enums.s_Shape
        {
            f_discriminant = mod_enums.s_Shape.k_Circle,
            f_Circle = new mod_enums.s_Shape.s_Shape_Circle { f_radius = radius },
        };

    static mod_enums.s_Shape Rectangle(int w, int h) =>
        new mod_enums.s_Shape
        {
            f_discriminant = mod_enums.s_Shape.k_Rectangle,
            f_Rectangle = new mod_enums.s_Shape.s_Shape_Rectangle { f_width = w, f_height = h },
        };

    static mod_enums.s_Shape Triangle(int b, int h) =>
        new mod_enums.s_Shape
        {
            f_discriminant = mod_enums.s_Shape.k_Triangle,
            f_Triangle = new mod_enums.s_Shape.s_Shape_Triangle { f_base = b, f_height = h },
        };

    static mod_enums.s_Direction North() =>
        new mod_enums.s_Direction { f_discriminant = mod_enums.s_Direction.k_North };
    static mod_enums.s_Direction South() =>
        new mod_enums.s_Direction { f_discriminant = mod_enums.s_Direction.k_South };
    static mod_enums.s_Direction East() =>
        new mod_enums.s_Direction { f_discriminant = mod_enums.s_Direction.k_East };
    static mod_enums.s_Direction West() =>
        new mod_enums.s_Direction { f_discriminant = mod_enums.s_Direction.k_West };

    [Fact]
    public void Area_circle() =>
        Assert.Equal(18, mod_enums.m_area_times_2(Circle(3)));   // 3*3*2=18

    [Fact]
    public void Area_rectangle() =>
        Assert.Equal(24, mod_enums.m_area_times_2(Rectangle(3, 4)));  // 3*4*2=24

    [Fact]
    public void Area_triangle() =>
        Assert.Equal(12, mod_enums.m_area_times_2(Triangle(4, 3)));  // 4*3=12

    [Fact]
    public void Perimeter_circle() =>
        Assert.Equal(18, mod_enums.m_perimeter_approx(Circle(3)));  // 3*6=18

    [Fact]
    public void Perimeter_rectangle() =>
        Assert.Equal(14, mod_enums.m_perimeter_approx(Rectangle(3, 4)));  // 2*(3+4)=14

    [Fact]
    public void Perimeter_triangle() =>
        Assert.Equal(10, mod_enums.m_perimeter_approx(Triangle(4, 3)));  // 4 + 3*2 = 10

    [Fact]
    public void Opposite_north() =>
        Assert.Equal(mod_enums.s_Direction.k_South,
            mod_enums.m_opposite(North()).f_discriminant);

    [Fact]
    public void Opposite_south() =>
        Assert.Equal(mod_enums.s_Direction.k_North,
            mod_enums.m_opposite(South()).f_discriminant);

    [Fact]
    public void Opposite_east() =>
        Assert.Equal(mod_enums.s_Direction.k_West,
            mod_enums.m_opposite(East()).f_discriminant);

    [Fact]
    public void Opposite_west() =>
        Assert.Equal(mod_enums.s_Direction.k_East,
            mod_enums.m_opposite(West()).f_discriminant);

    [Fact]
    public void DirectionValue_north() =>
        Assert.Equal(0, mod_enums.m_direction_value(North()));

    [Fact]
    public void DirectionValue_west() =>
        Assert.Equal(3, mod_enums.m_direction_value(West()));
}
