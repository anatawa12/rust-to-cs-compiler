/// Tests for the generated C# code from tests/inputs/arithmetic.rs.
/// Each test calls a generated method and asserts the result.
public class ArithmeticTests
{
    [Fact]
    public void Add_positive() =>
        Assert.Equal(5, mod_arithmetic.m_add(2, 3));

    [Fact]
    public void Add_negative() =>
        Assert.Equal(-1, mod_arithmetic.m_add(2, -3));

    [Fact]
    public void Sub() =>
        Assert.Equal(7, mod_arithmetic.m_sub(10, 3));

    [Fact]
    public void Mul() =>
        Assert.Equal(12, mod_arithmetic.m_mul(3, 4));

    [Fact]
    public void Div() =>
        Assert.Equal(3, mod_arithmetic.m_div(9, 3));

    [Fact]
    public void Rem() =>
        Assert.Equal(1, mod_arithmetic.m_rem(10, 3));

    [Fact]
    public void Max_first_larger() =>
        Assert.Equal(7, mod_arithmetic.m_max(7, 3));

    [Fact]
    public void Max_second_larger() =>
        Assert.Equal(9, mod_arithmetic.m_max(4, 9));

    [Fact]
    public void Min_first_smaller() =>
        Assert.Equal(2, mod_arithmetic.m_min(2, 8));

    [Fact]
    public void Min_second_smaller() =>
        Assert.Equal(3, mod_arithmetic.m_min(7, 3));

    [Fact]
    public void Abs_positive() =>
        Assert.Equal(5, mod_arithmetic.m_abs(5));

    [Fact]
    public void Abs_negative() =>
        Assert.Equal(5, mod_arithmetic.m_abs(-5));

    [Fact]
    public void Fib_base_0() =>
        Assert.Equal(0, mod_arithmetic.m_fib(0));

    [Fact]
    public void Fib_base_1() =>
        Assert.Equal(1, mod_arithmetic.m_fib(1));

    [Fact]
    public void Fib_6() =>
        Assert.Equal(8, mod_arithmetic.m_fib(6));

    [Fact]
    public void Fib_10() =>
        Assert.Equal(55, mod_arithmetic.m_fib(10));

    [Fact]
    public void Factorial_0() =>
        Assert.Equal(1UL, mod_arithmetic.m_factorial(0));

    [Fact]
    public void Factorial_5() =>
        Assert.Equal(120UL, mod_arithmetic.m_factorial(5));

    [Fact]
    public void IsEven_true() =>
        Assert.True(mod_arithmetic.m_is_even(4));

    [Fact]
    public void IsEven_false() =>
        Assert.False(mod_arithmetic.m_is_even(7));

    [Fact]
    public void Gcd() =>
        Assert.Equal(6u, mod_arithmetic.m_gcd(12, 18));

    [Fact]
    public void Gcd_coprime() =>
        Assert.Equal(1u, mod_arithmetic.m_gcd(7, 13));
}
