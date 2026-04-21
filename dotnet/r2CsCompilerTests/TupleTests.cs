/// Tests for the generated C# code from tests/inputs/tuples.rs.
public class TupleTests
{
    [Fact]
    public void Make_pair_fst() {
        var t = mod_tuples.m_make_pair(3, 10L);
        Assert.Equal(3, t.Item1);
    }

    [Fact]
    public void Make_pair_snd() {
        var t = mod_tuples.m_make_pair(3, 10L);
        Assert.Equal(10L, t.Item2);
    }

    [Fact]
    public void Fst() => Assert.Equal(5, mod_tuples.m_fst((5, 99L)));

    [Fact]
    public void Snd() => Assert.Equal(99L, mod_tuples.m_snd((5, 99L)));

    [Fact]
    public void Swap_pair() {
        var r = mod_tuples.m_swap_pair((3, 7L));
        Assert.Equal(7L, r.Item1);
        Assert.Equal(3, r.Item2);
    }

    [Fact]
    public void Add_pair() => Assert.Equal(7, mod_tuples.m_add_pair((3, 4)));

    [Fact]
    public void Triple_sum() {
        var t = mod_tuples.m_triple(1, 2, 3);
        Assert.Equal(6, mod_tuples.m_sum_triple(t));
    }

    [Fact]
    public void Max_of_pair_first() => Assert.Equal(5, mod_tuples.m_max_of_pair((5, 3)));

    [Fact]
    public void Max_of_pair_second() => Assert.Equal(7, mod_tuples.m_max_of_pair((3, 7)));
}
