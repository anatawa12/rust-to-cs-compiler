/// Tests for the generated C# code from tests/inputs/calls.rs.
public class CallTests
{
    [Fact]
    public void Double_then_triple() =>
        Assert.Equal(18, mod_calls.m_double_then_triple(3));  // double(3)=6, triple(6)=18

    [Fact]
    public void Sum_of_doubles() =>
        Assert.Equal(10, mod_calls.m_sum_of_doubles(2, 3));   // 4 + 6 = 10

    [Fact]
    public void Apply_twice() =>
        Assert.Equal(12, mod_calls.m_apply_twice(3));          // double(double(3)) = 12

    [Fact]
    public void Sum_of_squares() =>
        Assert.Equal(13, mod_calls.m_sum_of_squares(2, 3));    // 4 + 9

    [Fact]
    public void Pythagorean_check_true() =>
        Assert.True(mod_calls.m_pythagorean_check(3, 4, 5));

    [Fact]
    public void Pythagorean_check_false() =>
        Assert.False(mod_calls.m_pythagorean_check(1, 2, 3));
}
