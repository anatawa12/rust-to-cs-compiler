/// Tests for the generated C# code from tests/inputs/calculator.rs.
public class CalculatorTests
{
    [Fact]
    public void Token_value_number() {
        var t = new mod_calculator.s_Token { f_discriminant = 0, f_Number = new mod_calculator.s_Token.s_Token_Number { f_0 = 42L } };
        Assert.Equal(42L, mod_calculator.m_token_value(t));
    }

    [Fact]
    public void Token_value_plus() {
        var t = new mod_calculator.s_Token { f_discriminant = 1 }; // Plus
        Assert.Equal(0L, mod_calculator.m_token_value(t));
    }

    [Fact]
    public void Is_operator_plus() {
        var t = new mod_calculator.s_Token { f_discriminant = 1 }; // Plus
        Assert.True(mod_calculator.m_is_operator(t));
    }

    [Fact]
    public void Is_operator_number() {
        var t = new mod_calculator.s_Token { f_discriminant = 0, f_Number = new mod_calculator.s_Token.s_Token_Number { f_0 = 1L } };
        Assert.False(mod_calculator.m_is_operator(t));
    }

    [Fact]
    public void Compute_add() => Assert.Equal(7L, mod_calculator.m_compute(3L, 0, 4L));

    [Fact]
    public void Compute_sub() => Assert.Equal(1L, mod_calculator.m_compute(5L, 1, 4L));

    [Fact]
    public void Compute_mul() => Assert.Equal(12L, mod_calculator.m_compute(3L, 2, 4L));

    [Fact]
    public void Compute_div() => Assert.Equal(2L, mod_calculator.m_compute(8L, 3, 4L));

    [Fact]
    public void Multi_op() => Assert.Equal(15L, mod_calculator.m_multi_op(2L, 3L, 3L)); // (2+3)*3
}
