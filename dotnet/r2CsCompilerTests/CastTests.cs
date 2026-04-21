/// Tests for the generated C# code from tests/inputs/casts.rs.
public class CastTests
{
    [Fact]
    public void I32_to_u8_positive() =>
        Assert.Equal((byte)5, mod_casts.m_i32_to_u8(5));

    [Fact]
    public void I32_to_u8_wrapping() =>
        Assert.Equal((byte)0, mod_casts.m_i32_to_u8(256));   // 256 wraps to 0

    [Fact]
    public void I32_to_u8_negative() =>
        Assert.Equal((byte)255, mod_casts.m_i32_to_u8(-1));   // -1 wraps to 255

    [Fact]
    public void U8_to_i32() =>
        Assert.Equal(200, mod_casts.m_u8_to_i32(200));

    [Fact]
    public void I32_to_u32_positive() =>
        Assert.Equal(10u, mod_casts.m_i32_to_u32(10));

    [Fact]
    public void I32_to_u32_negative() =>
        Assert.Equal(uint.MaxValue, mod_casts.m_i32_to_u32(-1));

    [Fact]
    public void U32_to_i32_small() =>
        Assert.Equal(42, mod_casts.m_u32_to_i32(42u));

    [Fact]
    public void U32_to_i32_large() =>
        Assert.Equal(-1, mod_casts.m_u32_to_i32(uint.MaxValue));

    [Fact]
    public void U8_to_u32() =>
        Assert.Equal(255u, mod_casts.m_u8_to_u32(255));

    [Fact]
    public void I64_to_i32_truncate() =>
        Assert.Equal(0, mod_casts.m_i64_to_i32((long)0x1_0000_0000L));

    [Fact]
    public void I32_to_i64_sign_extend() =>
        Assert.Equal(-1L, mod_casts.m_i32_to_i64(-1));

    [Fact]
    public void U64_to_usize() =>
        Assert.Equal((nuint)42u, mod_casts.m_u64_to_usize(42u));

    [Fact]
    public void Saturating_cast_u8_wraps() =>
        Assert.Equal((byte)1, mod_casts.m_saturating_cast_u8(257));
}
