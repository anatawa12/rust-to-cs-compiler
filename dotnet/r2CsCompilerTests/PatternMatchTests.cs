/// Tests for the generated C# code from tests/inputs/pattern_match.rs.
public class PatternMatchTests
{
    private static mod_pattern_match.s_Expr Num(int x) =>
        new mod_pattern_match.s_Expr { f_discriminant = 0, f_Num = new mod_pattern_match.s_Expr.s_Expr_Num { f_0 = x } };
    private static mod_pattern_match.s_Expr Add(int a, int b) =>
        new mod_pattern_match.s_Expr { f_discriminant = 1, f_Add = new mod_pattern_match.s_Expr.s_Expr_Add { f_0 = a, f_1 = b } };
    private static mod_pattern_match.s_Expr Mul(int a, int b) =>
        new mod_pattern_match.s_Expr { f_discriminant = 2, f_Mul = new mod_pattern_match.s_Expr.s_Expr_Mul { f_0 = a, f_1 = b } };
    private static mod_pattern_match.s_Expr Neg(int x) =>
        new mod_pattern_match.s_Expr { f_discriminant = 3, f_Neg = new mod_pattern_match.s_Expr.s_Expr_Neg { f_0 = x } };

    [Fact]
    public void Eval_num() => Assert.Equal(42, mod_pattern_match.m_eval(Num(42)));

    [Fact]
    public void Eval_add() => Assert.Equal(7, mod_pattern_match.m_eval(Add(3, 4)));

    [Fact]
    public void Eval_mul() => Assert.Equal(12, mod_pattern_match.m_eval(Mul(3, 4)));

    [Fact]
    public void Eval_neg() => Assert.Equal(-5, mod_pattern_match.m_eval(Neg(5)));

    [Fact]
    public void Is_zero_true() => Assert.True(mod_pattern_match.m_is_zero(Num(0)));

    [Fact]
    public void Is_zero_false() => Assert.False(mod_pattern_match.m_is_zero(Num(1)));

    private static mod_pattern_match.s_Color Red() => new() { f_discriminant = 0 };
    private static mod_pattern_match.s_Color Green() => new() { f_discriminant = 1 };
    private static mod_pattern_match.s_Color Blue() => new() { f_discriminant = 2 };
    private static mod_pattern_match.s_Color Custom(byte r, byte g, byte b) =>
        new mod_pattern_match.s_Color { f_discriminant = 3, f_Custom = new mod_pattern_match.s_Color.s_Color_Custom { f_0 = r, f_1 = g, f_2 = b } };

    [Fact]
    public void Is_primary_red() => Assert.True(mod_pattern_match.m_is_primary(Red()));

    [Fact]
    public void Is_primary_custom() => Assert.False(mod_pattern_match.m_is_primary(Custom(100, 100, 100)));

    [Fact]
    public void Red_component_red() => Assert.Equal(255, (int)mod_pattern_match.m_red_component(Red()));

    [Fact]
    public void Red_component_custom() => Assert.Equal(128, (int)mod_pattern_match.m_red_component(Custom(128, 0, 0)));

    [Fact]
    public void Brightness_green() => Assert.Equal(150u, mod_pattern_match.m_brightness(Green()));

    [Fact]
    public void Brightness_custom() => Assert.Equal(30u, mod_pattern_match.m_brightness(Custom(10, 10, 10)));
}
