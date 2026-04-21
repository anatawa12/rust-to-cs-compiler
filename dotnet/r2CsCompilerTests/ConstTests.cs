/// Tests for the generated C# code from tests/inputs/consts.rs.
public class ConstTests
{
    [Fact]
    public void Zero_const_value() =>
        Assert.Equal(0, mod_consts.k_ZERO);

    [Fact]
    public void One_const_value() =>
        Assert.Equal(1, mod_consts.k_ONE);

    [Fact]
    public void MaxByte_const_value() =>
        Assert.Equal((byte)255, mod_consts.k_MAX_BYTE);

    [Fact]
    public void NegOne_const_value() =>
        Assert.Equal(-1, mod_consts.k_NEG_ONE);

    [Fact]
    public void Big_const_value() =>
        Assert.Equal(1_000_000_000_000L, mod_consts.k_BIG);

    [Fact]
    public void Use_const_returns_one() =>
        Assert.Equal(1, mod_consts.m_use_const());

    [Fact]
    public void Scale_by_const_identity() =>
        Assert.Equal(7, mod_consts.m_scale_by_const(7));

    [Fact]
    public void Is_max_byte_true() =>
        Assert.True(mod_consts.m_is_max_byte(255));

    [Fact]
    public void Is_max_byte_false() =>
        Assert.False(mod_consts.m_is_max_byte(100));

    [Fact]
    public void Get_big() =>
        Assert.Equal(1_000_000_000_000L, mod_consts.m_get_big());
}
