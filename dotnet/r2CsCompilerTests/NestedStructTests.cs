/// Tests for the generated C# code from tests/inputs/nested_structs.rs.
public class NestedStructTests
{
    [Fact]
    public void Make_vec2() {
        var v = mod_nested_structs.m_make_vec2(1.0f, 2.0f);
        Assert.Equal(1.0f, v.f_x, 4);
        Assert.Equal(2.0f, v.f_y, 4);
    }

    [Fact]
    public void Add_vecs() {
        var a = mod_nested_structs.m_make_vec2(1.0f, 2.0f);
        var b = mod_nested_structs.m_make_vec2(3.0f, 4.0f);
        var c = mod_nested_structs.m_add_vecs(a, b);
        Assert.Equal(4.0f, c.f_x, 4);
        Assert.Equal(6.0f, c.f_y, 4);
    }

    [Fact]
    public void Dot_product() {
        var a = mod_nested_structs.m_make_vec2(1.0f, 0.0f);
        var b = mod_nested_structs.m_make_vec2(0.0f, 1.0f);
        Assert.Equal(0.0f, mod_nested_structs.m_dot_product(a, b), 4);
    }

    [Fact]
    public void Dot_product_parallel() {
        var a = mod_nested_structs.m_make_vec2(1.0f, 0.0f);
        var b = mod_nested_structs.m_make_vec2(1.0f, 0.0f);
        Assert.Equal(1.0f, mod_nested_structs.m_dot_product(a, b), 4);
    }

    [Fact]
    public void Rect_area() {
        var origin = mod_nested_structs.m_make_vec2(0.0f, 0.0f);
        var size = mod_nested_structs.m_make_vec2(4.0f, 5.0f);
        var r = new mod_nested_structs.s_Rect { f_origin = origin, f_size = size };
        Assert.Equal(20.0f, mod_nested_structs.m_rect_area(r), 4);
    }
}
