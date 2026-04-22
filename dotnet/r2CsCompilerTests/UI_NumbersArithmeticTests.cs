/// UI-style tests for numbers_arithmetic.rs
/// Mirrors patterns from rustc tests/ui/numbers-arithmetic/
public class UI_NumbersArithmeticTests
{
    [Fact] public void Add() => Assert.Equal(7,  mod_numbers_arithmetic.m_add(3, 4));
    [Fact] public void Add_negative() => Assert.Equal(-1, mod_numbers_arithmetic.m_add(-4, 3));
    [Fact] public void Sub() => Assert.Equal(5,  mod_numbers_arithmetic.m_sub(8, 3));
    [Fact] public void Mul() => Assert.Equal(12, mod_numbers_arithmetic.m_mul(3, 4));
    [Fact] public void Div() => Assert.Equal(3,  mod_numbers_arithmetic.m_div(9, 3));
    [Fact] public void Rem() => Assert.Equal(1,  mod_numbers_arithmetic.m_rem(7, 3));
    [Fact] public void Neg() => Assert.Equal(-5, mod_numbers_arithmetic.m_neg(5));
    [Fact] public void Neg_negative() => Assert.Equal(3,  mod_numbers_arithmetic.m_neg(-3));

    [Fact] public void WrappingAdd_overflow() =>
        Assert.Equal(int.MinValue, mod_numbers_arithmetic.m_wrapping_add(int.MaxValue, 1));
    [Fact] public void WrappingMul() =>
        Assert.Equal(4, mod_numbers_arithmetic.m_wrapping_mul(2, 2));

    [Fact] public void MaxI32() => Assert.Equal(2147483647, mod_numbers_arithmetic.m_max_i32());
    [Fact] public void MinI32() => Assert.Equal(-2147483648, mod_numbers_arithmetic.m_min_i32());

    [Fact] public void AbsPositive() => Assert.Equal(5, mod_numbers_arithmetic.m_abs_val(5));
    [Fact] public void AbsNegative() => Assert.Equal(5, mod_numbers_arithmetic.m_abs_val(-5));
    [Fact] public void AbsZero() => Assert.Equal(0, mod_numbers_arithmetic.m_abs_val(0));

    [Fact] public void ClampBelow() => Assert.Equal(0,  mod_numbers_arithmetic.m_clamp(-5, 0, 10));
    [Fact] public void ClampAbove() => Assert.Equal(10, mod_numbers_arithmetic.m_clamp(15, 0, 10));
    [Fact] public void ClampIn()    => Assert.Equal(5,  mod_numbers_arithmetic.m_clamp(5, 0, 10));

    [Fact] public void Fibonacci0()  => Assert.Equal(0UL,  mod_numbers_arithmetic.m_fibonacci(0));
    [Fact] public void Fibonacci1()  => Assert.Equal(1UL,  mod_numbers_arithmetic.m_fibonacci(1));
    [Fact] public void Fibonacci10() => Assert.Equal(55UL, mod_numbers_arithmetic.m_fibonacci(10));
    [Fact] public void Fibonacci20() => Assert.Equal(6765UL, mod_numbers_arithmetic.m_fibonacci(20));
}
