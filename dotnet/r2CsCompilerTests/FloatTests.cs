/// Tests for the generated C# code from tests/inputs/floats.rs.
public class FloatTests
{
    private static float Tol => 1e-5f;

    [Fact]
    public void Add_f32() => Assert.Equal(5.5f, mod_floats.m_add_f32(2.0f, 3.5f), 4);

    [Fact]
    public void Sub_f32() => Assert.Equal(-1.5f, mod_floats.m_sub_f32(2.0f, 3.5f), 4);

    [Fact]
    public void Mul_f32() => Assert.Equal(7.0f, mod_floats.m_mul_f32(2.0f, 3.5f), 4);

    [Fact]
    public void Div_f32() => Assert.Equal(0.5f, mod_floats.m_div_f32(1.0f, 2.0f), 4);

    [Fact]
    public void Add_f64() => Assert.Equal(5.5, mod_floats.m_add_f64(2.0, 3.5), 10);

    [Fact]
    public void Mul_f64() => Assert.Equal(6.0, mod_floats.m_mul_f64(2.0, 3.0), 10);

    [Fact]
    public void F64_to_f32() => Assert.Equal(3.14f, mod_floats.m_f64_to_f32(3.14), 3);

    [Fact]
    public void F32_to_f64() => Assert.Equal(1.5, mod_floats.m_f32_to_f64(1.5f), 10);

    [Fact]
    public void I32_to_f64() => Assert.Equal(42.0, mod_floats.m_i32_to_f64(42), 10);

    [Fact]
    public void F64_to_i32_truncate() => Assert.Equal(3, mod_floats.m_f64_to_i32(3.9));

    [Fact]
    public void Negate_f32() => Assert.Equal(-1.5f, mod_floats.m_negate_f32(1.5f), 4);

    [Fact]
    public void Is_positive_f64_true() => Assert.True(mod_floats.m_is_positive_f64(0.1));

    [Fact]
    public void Is_positive_f64_false() => Assert.False(mod_floats.m_is_positive_f64(-0.1));
}
