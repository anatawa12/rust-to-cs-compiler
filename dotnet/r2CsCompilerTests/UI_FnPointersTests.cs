/// UI-style tests for fn_pointers.rs
/// Mirrors patterns from rustc tests/ui/fn-ptr/ and tests/ui/closures/
public class UI_FnPointersTests
{
    [Fact] public void ApplyTwiceDouble_3() => Assert.Equal(12, mod_fn_pointers.m_apply_twice_double(3));
    [Fact] public void ApplyTwiceDouble_1() => Assert.Equal(4,  mod_fn_pointers.m_apply_twice_double(1));
    [Fact] public void ApplyTwiceInc_5()    => Assert.Equal(7,  mod_fn_pointers.m_apply_twice_inc(5));
    [Fact] public void ApplyTwiceInc_0()    => Assert.Equal(2,  mod_fn_pointers.m_apply_twice_inc(0));
    [Fact] public void ApplyTwiceSquare_2() => Assert.Equal(16, mod_fn_pointers.m_apply_twice_square(2));
    [Fact] public void ApplyTwiceSquare_3() => Assert.Equal(81, mod_fn_pointers.m_apply_twice_square(3));

    [Fact] public void DoubleThenInc_4() => Assert.Equal(9,   mod_fn_pointers.m_double_then_inc(4));
    [Fact] public void IncThenDouble_4() => Assert.Equal(10,  mod_fn_pointers.m_inc_then_double(4));
    [Fact] public void SquareThenNegate_3() => Assert.Equal(-9, mod_fn_pointers.m_square_then_negate(3));
}
