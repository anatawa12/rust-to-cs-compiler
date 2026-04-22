/// UI-style tests for structs_methods2.rs
/// Mirrors patterns from rustc tests/ui/structs/
public class UI_StructsMethods2Tests
{
    [Fact] public void DotProduct_orthogonal() =>
        Assert.Equal(0.0, mod_structs_methods2.m_dot_product(1.0, 0.0, 0.0, 1.0));

    [Fact] public void DotProduct_parallel() =>
        Assert.Equal(1.0, mod_structs_methods2.m_dot_product(1.0, 0.0, 1.0, 0.0));

    [Fact] public void DotProduct_3_4() =>
        Assert.Equal(11.0, mod_structs_methods2.m_dot_product(1.0, 2.0, 3.0, 4.0));

    [Fact] public void LengthSq_3_4() =>
        Assert.Equal(25.0, mod_structs_methods2.m_length_sq(3.0, 4.0));

    [Fact] public void LengthSq_origin() =>
        Assert.Equal(0.0, mod_structs_methods2.m_length_sq(0.0, 0.0));

    [Fact] public void ScaleX() =>
        Assert.Equal(6.0, mod_structs_methods2.m_scale_x(3.0, 1.0, 2.0));

    [Fact] public void Accumulate_3_values() =>
        // (10 + 20 + 30) / 3 = 20
        Assert.Equal(20L, mod_structs_methods2.m_accumulate(10, 20, 30));

    [Fact] public void Accumulate_same() =>
        Assert.Equal(7L, mod_structs_methods2.m_accumulate(7, 7, 7));
}
