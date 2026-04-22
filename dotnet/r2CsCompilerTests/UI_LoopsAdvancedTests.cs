/// UI-style tests for loops_advanced.rs
/// Mirrors patterns from rustc tests/ui/loops/ and tests/ui/break-continue/
public class UI_LoopsAdvancedTests
{
    [Fact] public void SumEvens0To6() =>
        // 0+2+4+6 = 12
        Assert.Equal(12, mod_loops_advanced.m_sum_evens_up_to(6));

    [Fact] public void SumEvens0To1() => Assert.Equal(0, mod_loops_advanced.m_sum_evens_up_to(1));
    [Fact] public void SumEvens0To0() => Assert.Equal(0, mod_loops_advanced.m_sum_evens_up_to(0));

    [Fact] public void FirstGreaterThan_10() =>
        // 1+2+3+4+5 = 15 > 10
        Assert.Equal(15, mod_loops_advanced.m_first_greater_than(15, 10));

    [Fact] public void LoopBreakValue() =>
        // loop breaks when i >= 5, returns i*2 = 10
        Assert.Equal(10, mod_loops_advanced.m_loop_break_value(5));

    [Fact] public void LoopBreakValue1() =>
        // breaks when i >= 1, returns 1*2 = 2
        Assert.Equal(2, mod_loops_advanced.m_loop_break_value(1));

    [Fact] public void NestedLoopSum_2x3() =>
        // rows=2, cols=3: (0*3+0)+(0*3+1)+(0*3+2)+(1*3+0)+(1*3+1)+(1*3+2) = 0+1+2+3+4+5 = 15
        Assert.Equal(15, mod_loops_advanced.m_nested_loop_sum(2, 3));

    [Fact] public void NestedLoopSum_1x1() => Assert.Equal(0, mod_loops_advanced.m_nested_loop_sum(1, 1));

    [Fact] public void ContinueSkipMultiplesOf3_10() =>
        // 1..10 excluding 3,6,9: 1+2+4+5+7+8+10 = 37
        Assert.Equal(37, mod_loops_advanced.m_continue_skip_multiples_of_3(10));

    [Fact] public void ContinueSkipMultiplesOf3_0() =>
        Assert.Equal(0, mod_loops_advanced.m_continue_skip_multiples_of_3(0));
}
