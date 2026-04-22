/// Tests for the generated C# code from tests/inputs/traits.rs.
public class TraitTests
{
    [Fact]
    public void Get_value() =>
        Assert.Equal(42, mod_traits.m_get_value(new mod_traits.s_Wrapper { f_0 = 42 }));

    [Fact]
    public void Get_doubled() =>
        Assert.Equal(10, mod_traits.m_get_doubled(new mod_traits.s_Wrapper { f_0 = 5 }));

    [Fact]
    public void Triple_count() =>
        Assert.Equal(15u, mod_traits.m_triple_count(new mod_traits.s_Counter3 { f_0 = 5u }));

    [Fact]
    public void Seven_is_even_false() =>
        Assert.False(mod_traits.m_seven_is_even(new mod_traits.s_Counter7 { f_0 = 7u }));

    [Fact]
    public void Seven_is_even_even_input() =>
        Assert.True(mod_traits.m_seven_is_even(new mod_traits.s_Counter7 { f_0 = 6u }));
}
