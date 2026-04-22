/// Tests for the generated C# code from tests/inputs/bitwise.rs.
public class BitwiseTests
{
    [Fact]
    public void BitAnd() => Assert.Equal(0b1010u, mod_bitwise.m_bit_and(0b1111u, 0b1010u));

    [Fact]
    public void BitOr() => Assert.Equal(0b1111u, mod_bitwise.m_bit_or(0b1010u, 0b0101u));

    [Fact]
    public void BitXor() => Assert.Equal(0b1111u, mod_bitwise.m_bit_xor(0b1010u, 0b0101u));

    [Fact]
    public void BitXor_same() => Assert.Equal(0u, mod_bitwise.m_bit_xor(0b1010u, 0b1010u));

    [Fact]
    public void BitNot() => Assert.Equal(~3u, mod_bitwise.m_bit_not(3u));

    [Fact]
    public void Shl_by_one() => Assert.Equal(4u, mod_bitwise.m_shl(2u, 1u));

    [Fact]
    public void Shr_by_one() => Assert.Equal(1u, mod_bitwise.m_shr(2u, 1u));

    [Fact]
    public void LogicalNot_true() => Assert.False(mod_bitwise.m_logical_not(true));

    [Fact]
    public void LogicalNot_false() => Assert.True(mod_bitwise.m_logical_not(false));

    [Fact]
    public void CountSetBits_zero() => Assert.Equal(0u, mod_bitwise.m_count_set_bits(0u));

    [Fact]
    public void CountSetBits_all() => Assert.Equal(32u, mod_bitwise.m_count_set_bits(uint.MaxValue));

    [Fact]
    public void CountSetBits_one() => Assert.Equal(1u, mod_bitwise.m_count_set_bits(16u));

    [Fact]
    public void IsPowerOfTwo_true() => Assert.True(mod_bitwise.m_is_power_of_two(8u));

    [Fact]
    public void IsPowerOfTwo_false() => Assert.False(mod_bitwise.m_is_power_of_two(7u));

    [Fact]
    public void IsPowerOfTwo_zero() => Assert.False(mod_bitwise.m_is_power_of_two(0u));
}
