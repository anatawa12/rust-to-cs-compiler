/// Tests for the generated C# code from tests/inputs/recursion.rs.
public class RecursionTests
{
    [Fact]
    public void Factorial_0() => Assert.Equal(1ul, mod_recursion.m_factorial(0));

    [Fact]
    public void Factorial_1() => Assert.Equal(1ul, mod_recursion.m_factorial(1));

    [Fact]
    public void Factorial_5() => Assert.Equal(120ul, mod_recursion.m_factorial(5));

    [Fact]
    public void Factorial_10() => Assert.Equal(3628800ul, mod_recursion.m_factorial(10));

    [Fact]
    public void Fibonacci_0() => Assert.Equal(0ul, mod_recursion.m_fibonacci(0));

    [Fact]
    public void Fibonacci_1() => Assert.Equal(1ul, mod_recursion.m_fibonacci(1));

    [Fact]
    public void Fibonacci_10() => Assert.Equal(55ul, mod_recursion.m_fibonacci(10));

    [Fact]
    public void Power_zero_exponent() => Assert.Equal(1L, mod_recursion.m_power(5L, 0u));

    [Fact]
    public void Power_2_10() => Assert.Equal(1024L, mod_recursion.m_power(2L, 10u));

    [Fact]
    public void Gcd_12_8() => Assert.Equal(4ul, mod_recursion.m_gcd(12ul, 8ul));

    [Fact]
    public void Gcd_100_75() => Assert.Equal(25ul, mod_recursion.m_gcd(100ul, 75ul));

    [Fact]
    public void Sum_digits_single() => Assert.Equal(7ul, mod_recursion.m_sum_digits(7ul));

    [Fact]
    public void Sum_digits_123() => Assert.Equal(6ul, mod_recursion.m_sum_digits(123ul));

    [Fact]
    public void Sum_digits_9999() => Assert.Equal(36ul, mod_recursion.m_sum_digits(9999ul));
}
