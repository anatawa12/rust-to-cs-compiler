/// Tests for the generated C# code from tests/inputs/closures.rs.
public class ClosureTests
{
    [Fact]
    public void Apply_add_one() =>
        Assert.Equal(6, mod_closures.m_apply_add_one(5));

    [Fact]
    public void Apply_add_one_zero() =>
        Assert.Equal(1, mod_closures.m_apply_add_one(0));

    [Fact]
    public void Apply_mul_two() =>
        Assert.Equal(10, mod_closures.m_apply_mul_two(5));

    [Fact]
    public void Apply_mul_two_negative() =>
        Assert.Equal(-8, mod_closures.m_apply_mul_two(-4));

    [Fact]
    public void Compose_add_then_mul() =>
        // (2 + 3) * 4 = 20
        Assert.Equal(20, mod_closures.m_compose_add_then_mul(2));

    [Fact]
    public void Compose_add_then_mul_zero() =>
        // (0 + 3) * 4 = 12
        Assert.Equal(12, mod_closures.m_compose_add_then_mul(0));
}
