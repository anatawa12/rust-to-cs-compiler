/// Tests for the generated C# code from tests/inputs/loops.rs.
public class LoopTests
{
    [Fact]
    public void Sum_to_0() => Assert.Equal(0, mod_loops.m_sum_to(0));

    [Fact]
    public void Sum_to_5() => Assert.Equal(15, mod_loops.m_sum_to(5));   // 1+2+3+4+5

    [Fact]
    public void Sum_to_100() => Assert.Equal(5050, mod_loops.m_sum_to(100));

    [Fact]
    public void Product_to_5() => Assert.Equal(120, mod_loops.m_product_to(5));

    [Fact]
    public void Product_to_1() => Assert.Equal(1, mod_loops.m_product_to(1));

    [Fact]
    public void Count_down_5() => Assert.Equal(5, mod_loops.m_count_down(5));

    [Fact]
    public void Count_down_0() => Assert.Equal(0, mod_loops.m_count_down(0));

    [Fact]
    public void First_multiple_of_7_above_10() =>
        Assert.Equal(14, mod_loops.m_first_multiple_of_7_above(10));

    [Fact]
    public void First_multiple_of_7_above_14() =>
        Assert.Equal(21, mod_loops.m_first_multiple_of_7_above(14));

    [Fact]
    public void Collatz_steps_1() => Assert.Equal(0ul, mod_loops.m_collatz_steps(1));

    [Fact]
    public void Collatz_steps_6() => Assert.Equal(8ul, mod_loops.m_collatz_steps(6));

    [Fact]
    public void Max_in_range() => Assert.Equal(4, mod_loops.m_max_in_range(1, 5));
}
