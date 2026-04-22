/// UI-style tests for int_casts.rs
/// Mirrors patterns from rustc tests/ui/cast/ and tests/ui/numbers-arithmetic/
public class UI_IntCastsTests
{
    [Fact] public void I32ToI64_positive()  => Assert.Equal(42L,   mod_int_casts.m_i32_to_i64(42));
    [Fact] public void I32ToI64_negative()  => Assert.Equal(-5L,   mod_int_casts.m_i32_to_i64(-5));
    [Fact] public void I64ToI32_truncates() => Assert.Equal(-2147483647, mod_int_casts.m_i64_to_i32(1L + int.MaxValue + 1L));
    [Fact] public void U8ToI32()            => Assert.Equal(200,   mod_int_casts.m_u8_to_i32(200));
    [Fact] public void I32ToU8_wraps()      => Assert.Equal(0,     mod_int_casts.m_i32_to_u8(256));
    [Fact] public void I32ToU8_255()        => Assert.Equal(255,   mod_int_casts.m_i32_to_u8(255));
    [Fact] public void I32ToF64()           => Assert.Equal(3.0,   mod_int_casts.m_i32_to_f64(3));
    [Fact] public void F64ToI32()           => Assert.Equal(3,     mod_int_casts.m_f64_to_i32(3.7));
    [Fact] public void F64ToI32_negative()  => Assert.Equal(-3,    mod_int_casts.m_f64_to_i32(-3.7));

    [Fact] public void U32ToI32Wrap() =>
        Assert.Equal(int.MinValue, mod_int_casts.m_u32_to_i32_wrap((uint)int.MaxValue + 1));
    [Fact] public void I32ToU32Wrap() =>
        Assert.Equal(uint.MaxValue, mod_int_casts.m_i32_to_u32_wrap(-1));

    [Fact] public void ChainCast() => Assert.Equal(42u, mod_int_casts.m_chain_cast(42));
    [Fact] public void TruncateCycle() => Assert.Equal(1, mod_int_casts.m_truncate_cycle(257));

    [Fact] public void Popcount0()  => Assert.Equal(0u,  mod_int_casts.m_popcount_u32(0));
    [Fact] public void Popcount1()  => Assert.Equal(1u,  mod_int_casts.m_popcount_u32(1));
    [Fact] public void Popcount8()  => Assert.Equal(1u,  mod_int_casts.m_popcount_u32(8));
    [Fact] public void Popcount255() => Assert.Equal(8u, mod_int_casts.m_popcount_u32(255));

    [Fact] public void LeadingZeros1()  => Assert.Equal(31u, mod_int_casts.m_leading_zeros(1));
    [Fact] public void LeadingZeros0()  => Assert.Equal(32u, mod_int_casts.m_leading_zeros(0));
    [Fact] public void LeadingZerosTop() => Assert.Equal(0u, mod_int_casts.m_leading_zeros(0x8000_0000u));
}
