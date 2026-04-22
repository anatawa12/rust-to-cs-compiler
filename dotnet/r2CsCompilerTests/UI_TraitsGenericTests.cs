/// UI-style tests for traits_generic.rs
/// Mirrors patterns from rustc tests/ui/traits/ and tests/ui/generics/
public class UI_TraitsGenericTests
{
    [Fact] public void SumWrapper_3_4() =>
        Assert.Equal(7, mod_traits_generic.m_sum_wrapper(3, 4));

    [Fact] public void SumWrapper_neg() =>
        Assert.Equal(-1, mod_traits_generic.m_sum_wrapper(-4, 3));

    [Fact] public void DoubleWrapper_5() =>
        Assert.Equal(10, mod_traits_generic.m_double_wrapper(5));

    [Fact] public void DoubleWrapper_0() =>
        Assert.Equal(0, mod_traits_generic.m_double_wrapper(0));

    [Fact] public void ApplyTransform_3x5() =>
        Assert.Equal(15, mod_traits_generic.m_apply_transform(3, 5));

    [Fact] public void ApplyTransform_zero() =>
        Assert.Equal(0, mod_traits_generic.m_apply_transform(0, 100));
}
