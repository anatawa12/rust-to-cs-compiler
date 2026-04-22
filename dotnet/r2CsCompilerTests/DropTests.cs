/// Tests for the generated C# code from tests/inputs/drop.rs.
public class DropTests
{
    [Fact]
    public void Use_resource_returns_doubled() =>
        Assert.Equal(84, mod_drop.m_use_resource(42));

    [Fact]
    public void Use_resource_zero() =>
        Assert.Equal(0, mod_drop.m_use_resource(0));

    [Fact]
    public void Nested_drop_sums() =>
        Assert.Equal(7, mod_drop.m_nested_drop(3, 4));

    [Fact]
    public void Nested_drop_negative() =>
        Assert.Equal(-1, mod_drop.m_nested_drop(-5, 4));
}
