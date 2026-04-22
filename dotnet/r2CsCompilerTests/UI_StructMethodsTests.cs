/// UI-style tests for struct_methods.rs
/// Mirrors patterns from rustc tests/ui/structs/
public class UI_StructMethodsTests
{
    [Fact] public void RectArea_3x4() =>
        Assert.Equal(12, mod_struct_methods.m_rect_area(0, 0, 3, 4));

    [Fact] public void RectArea_10x5() =>
        Assert.Equal(50, mod_struct_methods.m_rect_area(0, 0, 10, 5));

    [Fact] public void RectArea_offset() =>
        // rect from (2,3) to (7,8): width=5, height=5
        Assert.Equal(25, mod_struct_methods.m_rect_area(2, 3, 7, 8));

    [Fact] public void CountUp_0() =>
        Assert.Equal(0, mod_struct_methods.m_count_up(0));

    [Fact] public void CountUp_5() =>
        Assert.Equal(5, mod_struct_methods.m_count_up(5));

    [Fact] public void CountUp_10() =>
        Assert.Equal(10, mod_struct_methods.m_count_up(10));
}
